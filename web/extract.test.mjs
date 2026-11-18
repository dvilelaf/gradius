// The extractor wasm in node: the real ROM gives the same pack as the native
// extraction, a fake ROM is refused with the expected SHA.
//
// Usage: GRADIUS_ROM=<rom> node web/extract.test.mjs   (after web/build.sh)
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { ExtractError, extractFromBytes } from "./extract.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const romPath = process.env.GRADIUS_ROM;
assert.ok(romPath, "set GRADIUS_ROM to a Gradius (E) ROM");

const wasm = await readFile(join(here, "gradius_extract_wasm.wasm"));
const reference = await readFile(join(here, "..", "assets", "gradius.pak"));
const rom = new Uint8Array(await readFile(romPath));

// A fake ROM: valid iNES header, wrong content.
const fake = new Uint8Array(16 + 0x8000 + 0x2000);
fake.set([0x4e, 0x45, 0x53, 0x1a, 2, 1]);
await assert.rejects(
  extractFromBytes(fake, () => {}, wasm),
  (error) =>
    error instanceof ExtractError &&
    error.message.includes("3aa97b519a5191766a53ae51c28a51c17778716bc02ef58b9c82afc761e20329"),
);
await assert.rejects(extractFromBytes(new Uint8Array(4), () => {}, wasm), /not an iNES/);

const steps = new Set();
const started = performance.now();
const pack = await extractFromBytes(rom, (step) => steps.add(step), wasm);
const seconds = ((performance.now() - started) / 1000).toFixed(1);

assert.equal(Buffer.compare(Buffer.from(pack), reference), 0, "pack differs from assets/gradius.pak");
assert.equal(steps.size, 9, "progress did not report every step");
console.log(`extract: pack identical to the native one (${pack.length} bytes) in ${seconds}s, wrong ROMs refused`);
