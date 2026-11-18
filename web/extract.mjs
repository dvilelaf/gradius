// Run the extractor wasm on ROM bytes and return the pack bytes. Shared by the
// Web Worker and the node test; `wasm` is the module's bytes or a compiled
// WebAssembly.Module.

/** The extraction steps, in the order of `gradius_extract::Step`. */
export const STEP_LABELS = [
  "Reading tiles",
  "Reading audio",
  "Reading backgrounds",
  "Reading collision",
  "Reading enemy spawns",
  "Reading sprites",
  "Reading tables",
  "Rendering the stage",
  "Writing the pack",
];

export class ExtractError extends Error {}

/**
 * @param {Uint8Array} rom
 * @param {(step: number, done: number, total: number) => void} onProgress
 * @param {BufferSource | WebAssembly.Module} wasm
 * @returns {Promise<Uint8Array>} a copy of the pack
 */
export async function extractFromBytes(rom, onProgress, wasm) {
  const imports = { env: { progress: (step, done, total) => onProgress(step, done, total) } };
  const instance =
    wasm instanceof WebAssembly.Module
      ? await WebAssembly.instantiate(wasm, imports)
      : (await WebAssembly.instantiate(wasm, imports)).instance;
  const x = instance.exports;

  const ptr = x.alloc(rom.length);
  new Uint8Array(x.memory.buffer, ptr, rom.length).set(rom);
  const code = x.extract(ptr, rom.length);
  x.dealloc(ptr, rom.length);

  if (code !== 0) {
    const message = new Uint8Array(x.memory.buffer, x.error_ptr(), x.error_len());
    throw new ExtractError(new TextDecoder().decode(message));
  }
  return new Uint8Array(x.memory.buffer, x.result_ptr(), x.result_len()).slice();
}
