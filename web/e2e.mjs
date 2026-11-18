// End-to-end check of the first run in a real browser: the setup screen with no
// pack, two wrong files refused, the real ROM extracted in the worker and stored
// in IndexedDB, a reload that starts without asking, the Options panel (a 2P
// start from its values, and they survive a reload), and "Delete game data".
//
// Usage (after web/build.sh). Nothing is added to the repo: install Playwright
// outside it, pointing at the chromium already in ~/.cache/ms-playwright
// (version 1223 is playwright-core 1.60.0):
//   npm i --prefix /tmp/pw playwright-core@1.60.0
//   GRADIUS_ROM=/path/to/Gradius-E.nes \
//     PLAYWRIGHT_CORE=/tmp/pw/node_modules/playwright-core/index.mjs \
//     node web/e2e.mjs
// PLAYWRIGHT_CORE may be omitted when playwright-core resolves from web/.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, extname, join, normalize, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const romPath = process.env.GRADIUS_ROM;
assert.ok(romPath, "set GRADIUS_ROM to a Gradius (E) ROM");
const { chromium } = await import(
  process.env.PLAYWRIGHT_CORE ? pathToFileURL(process.env.PLAYWRIGHT_CORE).href : "playwright-core"
);

const GRADIUS_E_SHA256 = "3aa97b519a5191766a53ae51c28a51c17778716bc02ef58b9c82afc761e20329";
const EXTRACT_TIMEOUT_MS = 10 * 60_000;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const TYPES = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".webmanifest": "application/manifest+json",
};

// Serves web/ like a static host, but never gradius.pak: the first run has to
// come from the ROM this test hands the page.
const server = createServer(async (request, response) => {
  const url = new URL(request.url, "http://127.0.0.1");
  const name = decodeURIComponent(url.pathname).replace(/^\/+/, "") || "index.html";
  const file = normalize(join(here, name));
  if (name === "gradius.pak" || !file.startsWith(here + sep)) {
    response.writeHead(404).end();
    return;
  }
  try {
    const body = await readFile(file);
    response.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}/`;

// A digest of everything the game canvas shows, to see that the picture moved.
const screenHash = (page) =>
  page.evaluate(() => {
    const canvas = document.getElementById("screen");
    const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    let hash = 0;
    for (let i = 0; i < data.length; i += 1) hash = (Math.imul(hash, 31) + data[i]) | 0;
    return hash;
  });

// Pixels of the game canvas that are not black: the title screen draws text.
const litPixels = (page) =>
  page.evaluate(() => {
    const canvas = document.getElementById("screen");
    const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    let lit = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] | data[i + 1] | data[i + 2]) lit += 1;
    }
    return lit;
  });

// Wait until the game has drawn the title (or any picture) on the canvas.
async function waitForPicture(page) {
  const deadline = Date.now() + 60_000;
  while ((await litPixels(page)) <= 1000) {
    if (Date.now() > deadline) throw new Error("the game canvas stayed blank");
    await sleep(250);
  }
}

// The pack as the page stored it: its length and SHA-256.
const storedPack = (page) =>
  page.evaluate(async () => {
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open("gradius", 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const pack = await new Promise((resolve, reject) => {
      const request = db.transaction("pack").objectStore("pack").get("gradius.pak");
      request.onsuccess = () => resolve(request.result ?? null);
      request.onerror = () => reject(request.error);
    });
    db.close();
    if (!pack) return null;
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", pack));
    return {
      length: pack.byteLength,
      sha256: [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join(""),
    };
  });

async function waitForRefusal(page, message) {
  await page.waitForFunction(
    (expected) => document.getElementById("setup-error").textContent === expected,
    message,
  );
  assert.equal(await page.locator("#setup-ask").isVisible(), true, "the page must ask again");
  assert.equal(await page.locator("#setup-busy").isVisible(), false);
}

const started = Date.now();
const browser = await chromium.launch();
const errors = [];
try {
  const page = await browser.newPage();
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto(origin);
  await page.waitForSelector("#setup-ask", { state: "visible" });
  assert.equal(await page.locator("#setup-busy").isVisible(), false);
  console.log("setup: the page asks for a ROM when no pack is stored");

  await page.setInputFiles("#setup-file", {
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("hello"),
  });
  await waitForRefusal(page, "This file is not a NES ROM.");

  const otherGame = Buffer.alloc(16 + 0x8000 + 0x2000);
  otherGame.set([0x4e, 0x45, 0x53, 0x1a, 2, 1]);
  await page.setInputFiles("#setup-file", {
    name: "other.nes",
    mimeType: "application/octet-stream",
    buffer: otherGame,
  });
  await waitForRefusal(page, "This ROM is not Gradius (E). You need the European dump.");
  assert.match(await page.textContent("#setup-detail"), new RegExp(GRADIUS_E_SHA256));
  console.log("refusals: a non-ROM and another game are named, and the page asks again");

  const stepsSeen = new Set();
  const extractStarted = Date.now();
  await page.setInputFiles("#setup-file", romPath);
  while (await page.locator("#setup").isVisible()) {
    const refusal = await page.textContent("#setup-error");
    if (refusal) throw new Error(`the real ROM was refused: ${refusal}`);
    if (Date.now() - extractStarted > EXTRACT_TIMEOUT_MS) {
      throw new Error("the extraction did not finish in time");
    }
    stepsSeen.add(await page.textContent("#setup-step"));
    await sleep(500);
  }
  assert.ok(stepsSeen.size >= 2, `progress never moved: ${[...stepsSeen]}`);
  const extractSeconds = ((Date.now() - extractStarted) / 1000).toFixed(1);

  await waitForPicture(page);
  const native = await readFile(join(here, "..", "assets", "gradius.pak"));
  const stored = await storedPack(page);
  assert.ok(stored, "no pack in IndexedDB after the extraction");
  assert.equal(stored.length, native.length);
  assert.equal(stored.sha256, createHash("sha256").update(native).digest("hex"));
  console.log(`extract: the game started after ${extractSeconds}s and the pack is stored`);

  await page.reload();
  await waitForPicture(page);
  assert.equal(await page.locator("#setup").isVisible(), false, "a reload asked for the ROM again");
  console.log("reload: the game starts without asking for the ROM");

  // The title lasts about two seconds before its attract demo, so the 2P start
  // comes first: the options saved below must already be in force at boot.
  await page.click("#options-open");
  await page.fill("#hits", "3");
  await page.dispatchEvent("#hits", "change");
  await page.check("#two-at-once");
  await page.click("#options-close");
  await page.waitForFunction(() => !document.getElementById("options").open);
  await page.reload();
  await waitForPicture(page);
  const titleHash = await screenHash(page);
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Enter");
  await sleep(2000);
  assert.notEqual(await screenHash(page), titleHash, "the 2P start left the canvas unchanged");
  console.log("options: a 2P game starts from the saved options and the canvas changes");

  await page.reload();
  await waitForPicture(page);
  await page.click("#options-open");
  assert.equal(await page.inputValue("#hits"), "3", "the ship hits did not survive a reload");
  assert.equal(await page.isChecked("#two-at-once"), true, "2 players at once did not survive a reload");
  await page.click("#options-close");
  await page.waitForFunction(() => !document.getElementById("options").open);
  console.log("options: ship hits 3 and 2 players at once survive a reload");

  page.once("dialog", (dialog) => dialog.accept());
  await Promise.all([page.waitForEvent("load"), page.click("#forget")]);
  await page.waitForSelector("#setup-ask", { state: "visible" });
  assert.equal(await storedPack(page), null, "the pack survived Delete game data");
  console.log("delete: the page is back at the setup screen and the pack is gone");

  assert.deepEqual(errors, [], "uncaught page errors");
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  console.log(`e2e: first run passes in ${seconds}s`);
} finally {
  await browser.close();
  server.close();
}
