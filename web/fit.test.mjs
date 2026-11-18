// Canvas fit math. Run with: node web/fit.test.mjs
import assert from "node:assert/strict";
import { fitCanvas, SCREEN_WIDTH, SCREEN_HEIGHT } from "./layout.mjs";

function ratio(box) {
  return box.width / box.height;
}

// Whole-pixel steps, letterboxed inside a 16:9 stage.
assert.deepEqual(fitCanvas(1920, 1080, "integer"), {
  width: SCREEN_WIDTH * 4,
  height: SCREEN_HEIGHT * 4,
});
// A fractional scale still preserves the 16:15 screen aspect.
const wide = fitCanvas(1920, 1080, "fit");
assert.deepEqual(wide, { width: 1152, height: 1080 });
assert.equal(ratio(wide), ratio({ width: SCREEN_WIDTH, height: SCREEN_HEIGHT }));
// 21:9 and portrait letterbox without distortion.
for (const [w, h] of [[2560, 1080], [390, 844], [844, 390]]) {
  const box = fitCanvas(w, h, "fit");
  assert.equal(ratio(box), SCREEN_WIDTH / SCREEN_HEIGHT);
  assert.ok(box.width <= w && box.height <= h);
}
// Integer drop just below a whole step; fit keeps the leftover pixels.
assert.deepEqual(fitCanvas(1023, 960, "integer"), {
  width: SCREEN_WIDTH * 3,
  height: SCREEN_HEIGHT * 3,
});
assert.deepEqual(fitCanvas(1023, 960, "fit"), {
  width: 1023,
  height: 959.0625,
});
// A stage smaller than the screen still fits instead of overflowing.
assert.deepEqual(fitCanvas(200, 200, "integer"), {
  width: 200,
  height: 187.5,
});
// Stretch fills the stage in both axes.
assert.deepEqual(fitCanvas(1920, 1080, "stretch"), {
  width: 1920,
  height: 1080,
});
// Degenerate layout must not produce a zero-sized canvas.
assert.deepEqual(fitCanvas(0, 0, "fit"), {
  width: 1,
  height: 0.9375,
});

console.log("fit: integer, fit, stretch and letterbox cases pass");
