// Canvas sizing for the wasm page. The backing buffer stays 256x240; only the
// CSS box changes, so the browser scales the framebuffer with pixelated
// sampling. Modes: integer (whole-pixel steps), fit (largest aspect-correct
// box) and stretch (fills the stage, distorted).
export const SCREEN_WIDTH = 256;
export const SCREEN_HEIGHT = 240;

export function fitCanvas(stageWidth, stageHeight, mode = "integer") {
  const width = Math.max(1, stageWidth);
  const height = Math.max(1, stageHeight);
  if (mode === "stretch") return { width, height };
  const scale = Math.min(width / SCREEN_WIDTH, height / SCREEN_HEIGHT);
  // Whole-pixel steps when they exist. Below 1:1 there is no integer step that
  // fits a phone, so fall back to the largest aspect-correct box.
  const factor = mode === "integer" && scale >= 1 ? Math.floor(scale) : scale;
  return { width: SCREEN_WIDTH * factor, height: SCREEN_HEIGHT * factor };
}
