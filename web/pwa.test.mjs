// PWA checks without a browser: the manifest parses, the referenced icon is
// real SVG, every runtime file serves over HTTP, and the service worker
// precaches the shell and answers a navigation while the network is down.
//
// Usage: node web/pwa.test.mjs   (after web/build.sh)
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const here = dirname(fileURLToPath(import.meta.url));
const BASE = "https://example.test/";
const REQUIRED = [
  "./",
  "./index.html",
  "./app.js",
  "./layout.mjs",
  "./audio-worklet.js",
  "./manifest.webmanifest",
  "./gradius_wasm.wasm",
  "./icon.svg",
  "./extract.mjs",
  "./extract-worker.js",
  "./pack-store.mjs",
];

const read = (name) => readFile(join(here, name));

// ---- Manifest and page markup -------------------------------------------

const manifest = JSON.parse(await readFile(join(here, "manifest.webmanifest"), "utf8"));
assert.match(manifest.name, /Gradius/);
assert.equal(manifest.display, "standalone");
assert.equal(manifest.start_url, "./");
assert.equal(manifest.scope, "./");
assert.deepEqual(manifest.icons.map((icon) => icon.src), ["icon.svg"]);
for (const icon of manifest.icons) {
  assert.equal(icon.sizes, "any");
  assert.match(await readFile(join(here, icon.src), "utf8"), /<svg/);
}

const html = await readFile(join(here, "index.html"), "utf8");
assert.match(html, /rel="manifest" href="manifest\.webmanifest"/);
assert.match(html, /id="install"/);
for (const id of ["fit", "fullscreen", "touch", "save", "load"]) {
  assert.match(html, new RegExp(`id="${id}"`), `the ${id} control is missing`);
}

const script = await readFile(join(here, "app.js"), "utf8");
assert.match(script, /beforeinstallprompt/);
assert.match(script, /extract-worker\.js/, "the page cannot extract a ROM");
assert.match(script, /serviceWorker[\s\S]*register\("service-worker\.js"\)/);
assert.match(script, /gradius\.savestate\.v1/, "the versioned save key is missing");
assert.match(script, /state_save\(\)/);
assert.match(script, /state_load\(/);
assert.match(script, /localStorage\.setItem\(SAVE_KEY/);
assert.match(script, /Numpad8/, "player 2 keyboard bindings are missing");
assert.match(script, /tick\(input\.p1, input\.p2\)/, "the second pad never reaches wasm");
assert.match(script, /getGamepads/, "second-gamepad support is missing");

// ---- Service worker in a minimal sandbox --------------------------------

const swSource = await readFile(join(here, "service-worker.js"), "utf8");
assert.doesNotMatch(swSource, /gradius\.pak"/, "the service worker must not precache the pack");
const listeners = Object.create(null);
const cachesByName = new Map();

function keyOf(request) {
  return typeof request === "string"
    ? new URL(request, BASE).href
    : new URL(request.url, BASE).href;
}

function makeCache() {
  const entries = new Map();
  return {
    entries,
    async addAll(requests) {
      const loaded = await Promise.all(
        requests.map(async (request) => {
          const url = keyOf(request);
          const response = await sandbox.fetch(url);
          if (!response.ok) throw new Error(`precache ${url}: ${response.status}`);
          return [url, response];
        }),
      );
      for (const [url, response] of loaded) entries.set(url, response);
    },
    async put(request, response) {
      entries.set(keyOf(request), response);
    },
    async match(request) {
      return entries.get(keyOf(request));
    },
  };
}

const sandbox = {
  Response,
  URL,
  Promise,
  console,
  fetch: async (url) => {
    const path = new URL(url, BASE).pathname.replace(/^\/$/, "/index.html");
    return new Response(await read(path.slice(1)), { status: 200 });
  },
  caches: {
    async open(name) {
      if (!cachesByName.has(name)) cachesByName.set(name, makeCache());
      return cachesByName.get(name);
    },
    async match(request) {
      for (const cache of cachesByName.values()) {
        const hit = await cache.match(request);
        if (hit) return hit;
      }
      return undefined;
    },
    async keys() {
      return [...cachesByName.keys()];
    },
    async delete(name) {
      return cachesByName.delete(name);
    },
  },
};
sandbox.self = {
  location: { origin: new URL(BASE).origin },
  addEventListener(type, listener) {
    (listeners[type] ??= []).push(listener);
  },
  skipWaiting: async () => {},
  clients: { claim: async () => {} },
};

vm.runInNewContext(swSource, sandbox, { filename: "service-worker.js" });

async function dispatch(type, event) {
  const waits = [];
  for (const listener of listeners[type] ?? []) {
    listener({ ...event, waitUntil: (promise) => waits.push(promise) });
  }
  await Promise.all(waits);
}

await dispatch("install", {});

const cached = await sandbox.caches.match(BASE);
for (const entry of REQUIRED) {
  const hit = await sandbox.caches.match(new URL(entry, BASE).href);
  assert.ok(hit, `the service worker did not precache ${entry}`);
}
assert.equal(cached.status, 200);

// Offline reload: a navigation request misses the network but is answered by
// the cached index page.
sandbox.fetch = () => Promise.reject(new TypeError("offline"));
let responsePromise;
await dispatch("fetch", {
  request: { method: "GET", url: BASE, mode: "navigate" },
  respondWith: (promise) => (responsePromise = promise),
});
const response = await responsePromise;
assert.equal(response.status, 200);
assert.match(await response.text(), /rel="manifest"/);

// A missing non-page asset fails as a network error, not as HTML.
let missPromise;
await dispatch("fetch", {
  request: { method: "GET", url: new URL("missing.js", BASE).href, mode: "no-cors" },
  respondWith: (promise) => (missPromise = promise),
});
assert.equal((await missPromise).type, "error");

// ---- HTTP: the page and its assets still load ---------------------------

const TYPES = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".webmanifest": "application/manifest+json",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

const server = createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, BASE).pathname));
  const relative = path === "/" ? "index.html" : path.slice(1);
  if (relative.startsWith("..")) {
    response.writeHead(403).end();
    return;
  }
  try {
    const body = await readFile(join(here, relative));
    response.writeHead(200, {
      "content-type": TYPES[extname(relative)] ?? "application/octet-stream",
      "content-length": body.length,
    });
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

try {
  for (const entry of REQUIRED) {
    const path = entry === "./" ? "/" : `/${entry.slice(2)}`;
    const got = await fetch(origin + path);
    assert.equal(got.status, 200, `${path} did not load`);
    assert.ok(Number(got.headers.get("content-length")) > 0, `${path} is empty`);
  }
} finally {
  server.close();
}

console.log(
  `pwa: ${manifest.icons.length} icons, ${REQUIRED.length} precached files, offline navigation and HTTP load pass`,
);
