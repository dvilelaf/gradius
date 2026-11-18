// Headless wasm smoke test using the extracted pack: tick the title and the
// stage, and confirm the mixer queued audible, bounded samples. The pack now
// carries the version 2 audio chunk (timelines plus the KDRV driver section),
// so this also exercises the same decode path the page uses at startup.
//
// Usage: node web/smoke.mjs [pack]   (after web/build.sh)
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const TWO_SECONDS = 120;
const MIN_SAMPLES = 90_000;
const MIN_RMS = 0.01;
const MAX_RMS = 0.5;
const MIN_BASS_RMS = 0.02;
const START = 0x08;

const here = dirname(fileURLToPath(import.meta.url));
const packPath = process.argv[2] ?? join(here, "gradius.pak");

let pack;
try {
  pack = new Uint8Array(await readFile(packPath));
} catch (error) {
  throw new Error(
    `cannot read ${packPath}: ${error.message}; ` +
      "run cargo xtask extract <rom> web/gradius.pak or web/build.sh first",
  );
}

const { instance } = await WebAssembly.instantiate(
  await readFile(join(here, "gradius_wasm.wasm")),
  {},
);
const wasm = instance.exports;
const memory = wasm.memory;

const ptr = wasm.alloc(pack.length);
new Uint8Array(memory.buffer, ptr, pack.length).set(pack);
const code = wasm.init(ptr, pack.length);
wasm.dealloc(ptr, pack.length);
if (code !== 0) throw new Error("init rejected the pack");

/** A second instance, the same thing a page reload starts with. */
async function newInstance() {
  const { instance } = await WebAssembly.instantiate(
    await readFile(join(here, "gradius_wasm.wasm")),
    {},
  );
  const fresh = instance.exports;
  const buffer = fresh.alloc(pack.length);
  new Uint8Array(fresh.memory.buffer, buffer, pack.length).set(pack);
  if (fresh.init(buffer, pack.length) !== 0) throw new Error("init rejected the pack");
  fresh.dealloc(buffer, pack.length);
  return fresh;
}

/** The `GRSS` image the page writes to localStorage. */
function takeState(instance) {
  const image = instance.state_save();
  const len = instance.state_len();
  if (!image || len === 0) throw new Error("state_save returned no image");
  return new Uint8Array(instance.memory.buffer, image, len).slice();
}

/** Put `bytes` in the instance's linear memory and return the load code. */
function loadState(instance, bytes) {
  const buffer = instance.alloc(bytes.length);
  if (buffer === 0) throw new Error("state_load could not allocate its buffer");
  new Uint8Array(instance.memory.buffer, buffer, bytes.length).set(bytes);
  const status = instance.state_load(buffer, bytes.length);
  instance.dealloc(buffer, bytes.length);
  return status;
}

const sameBytes = (a, b) =>
  a.length === b.length && a.every((value, index) => value === b[index]);

/** Copy the queued samples out before the next tick can grow memory. */
function takeAudio() {
  const available = wasm.audio_len();
  const samples = new Float32Array(
    memory.buffer,
    wasm.audio_ptr(),
    available,
  ).slice();
  wasm.audio_consumed(available);
  return samples;
}

function rms(samples) {
  let sum = 0;
  for (const sample of samples) sum += sample * sample;
  return Math.sqrt(sum / samples.length);
}

/** Two cascaded one-pole low-passes at 200 Hz: the triangle bass band. */
function lowBandRms(samples, cutoff = 200, rate = 48_000) {
  const alpha = 1 - Math.exp((-2 * Math.PI * cutoff) / rate);
  let first = 0;
  let second = 0;
  let sum = 0;
  for (const sample of samples) {
    first += alpha * (sample - first);
    second += alpha * (first - second);
    sum += second * second;
  }
  return Math.sqrt(sum / samples.length);
}

function check(label, samples, cue) {
  if (samples.length < MIN_SAMPLES) {
    throw new Error(`${label}: only ${samples.length} samples (${cue} cue)`);
  }
  let peak = 0;
  let finite = true;
  for (const sample of samples) {
    peak = Math.max(peak, Math.abs(sample));
    finite &&= Number.isFinite(sample) && Math.abs(sample) <= 1;
  }
  if (!finite || !(peak > 0)) {
    throw new Error(`${label}: queued silence or a non-finite sample (${cue})`);
  }
  const level = rms(samples);
  if (!(level >= MIN_RMS && level <= MAX_RMS)) {
    throw new Error(`${label}: RMS ${level.toFixed(3)} outside the audible range`);
  }
  const bass = lowBandRms(samples);
  if (!(bass >= MIN_BASS_RMS)) {
    throw new Error(
      `${label}: bass missing below 200 Hz (RMS ${bass.toFixed(4)}, ` +
        `expected >= ${MIN_BASS_RMS}); the triangle channel is silent`,
    );
  }
  return { level, bass, peak };
}

// Title fanfare: two seconds from init, no input.
for (let i = 0; i < TWO_SECONDS; i += 1) wasm.tick(0, 0);
const titleCue = wasm.music_cue();
if (titleCue !== 0) throw new Error(`expected the title cue, got ${titleCue}`);
const title = check("title", takeAudio(), titleCue);

// Start the stage, then render two more seconds of stage 1 music.
for (let i = 0; i < 3; i += 1) wasm.tick(START, 0);
takeAudio();
for (let i = 0; i < TWO_SECONDS; i += 1) wasm.tick(0, 0);
const stageCue = wasm.music_cue();
if (stageCue !== 1) throw new Error(`expected the stage 1 cue, got ${stageCue}`);
const stage = check("stage 1", takeAudio(), stageCue);

if (wasm.audio_len() !== 0) throw new Error("audio_consumed left samples behind");

// Savestate: capture here, reload it into a fresh instance (what the page does
// after a reload), and compare 60 frames with the run that was never stopped.
// The full `GRSS` image is compared, which is stricter than `state_hash`.
const savedFrame = wasm.frame_count();
const saved = takeState(wasm);
const continued = [];
for (let i = 0; i < 60; i += 1) {
  wasm.tick(0, 0);
  continued.push(takeState(wasm));
}

const restored = await newInstance();
const loadCode = loadState(restored, saved);
if (loadCode !== 0) throw new Error(`state_load refused a saved image (${loadCode})`);
if (restored.frame_count() !== savedFrame) {
  throw new Error(
    `state_load restored frame ${restored.frame_count()}, expected ${savedFrame}`,
  );
}
for (let i = 0; i < 60; i += 1) {
  restored.tick(0, 0);
  if (!sameBytes(takeState(restored), continued[i])) {
    throw new Error(`frame ${i + 1} after load diverged from the uninterrupted run`);
  }
}
if (restored.audio_len() === 0) throw new Error("no audio after a savestate load");

// A corrupt or truncated image is refused and touches neither the game nor the
// frame already on the canvas.
const FRAME_BYTES = 256 * 240 * 4;
const renderedPtr = restored.tick(0, 0);
const rendered = new Uint8Array(restored.memory.buffer, renderedPtr, FRAME_BYTES).slice();
const beforeFrame = restored.frame_count();
const beforeState = takeState(restored);
const junk = new TextEncoder().encode("not a GRSS image");
if (loadState(restored, junk) === 0) throw new Error("state_load accepted a corrupt image");
if (restored.frame_count() !== beforeFrame) throw new Error("a refused image moved the game on");
if (!sameBytes(takeState(restored), beforeState)) throw new Error("a refused image changed the state");
if (loadState(restored, saved.slice(0, saved.length >> 1)) === 0) {
  throw new Error("state_load accepted a truncated image");
}
if (restored.frame_count() !== beforeFrame) throw new Error("a truncated image moved the game on");
if (!sameBytes(new Uint8Array(restored.memory.buffer, renderedPtr, FRAME_BYTES), rendered)) {
  throw new Error("a refused image changed the rendered frame");
}

const format = ({ level, bass, peak }) =>
  `RMS ${level.toFixed(3)}, bass ${bass.toFixed(3)}, peak ${peak.toFixed(3)}`;
console.log(
  `smoke: title ${format(title)}; stage 1 ${format(stage)}; ` +
    `savestate ${saved.length} B, 60/60 frames match, corrupt refused; ` +
    `pack ${pack.length} bytes`,
);
