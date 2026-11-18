// Extracts the pack off the main thread. In: { rom: Uint8Array }.
// Out: { progress: [step, done, total] } | { pack } | { error }.
import { extractFromBytes } from "./extract.mjs";

self.onmessage = async ({ data }) => {
  try {
    const response = await fetch("gradius_extract_wasm.wasm");
    if (!response.ok) {
      throw new Error(`cannot load gradius_extract_wasm.wasm (${response.status})`);
    }
    const wasm = await response.arrayBuffer();
    const pack = await extractFromBytes(
      data.rom,
      (...progress) => self.postMessage({ progress }),
      wasm,
    );
    self.postMessage({ pack }, [pack.buffer]);
  } catch (error) {
    self.postMessage({ error: error.message });
  }
};
