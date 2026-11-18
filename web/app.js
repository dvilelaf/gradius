"use strict";

import {
  fitCanvas,
  SCREEN_WIDTH as WIDTH,
  SCREEN_HEIGHT as HEIGHT,
} from "./layout.mjs";
import { loadPack, savePack, deletePack } from "./pack-store.mjs";
import { STEP_LABELS } from "./extract.mjs";

// Minimal canvas backend for crates/gradius-wasm. The core renders 256x240
// RGBA8; this file owns the clock, the keyboard, the touch controls and the
// CSS scaling. The backing buffer never changes size.
const FRAME_BYTES = WIDTH * HEIGHT * 4;
// gradius_core::TICKS_PER_SECOND, the NTSC frame rate.
const TICKS_PER_SECOND = 60.09880005644298;
const STEP_MS = 1000 / TICKS_PER_SECOND;
// Same quarter-second stall cap as gradius_app::Runner.
const MAX_CATCH_UP_MS = 250;
// gradius_audio::SAMPLE_RATE, the rate of the mixer's mono output.
const SAMPLE_RATE = 48000;

// Input byte, NES controller order (see crates/gradius-wasm/src/lib.rs).
const FIRE = 0x01;
const POWER = 0x02;
const SELECT = 0x04;
const START = 0x08;
const UP = 0x10;
const DOWN = 0x20;
const LEFT = 0x40;
const RIGHT = 0x80;

// Same keys as the SDL frontend, plus P for pause.
const KEY_BITS = new Map([
  ["ArrowUp", UP], ["KeyW", UP],
  ["ArrowDown", DOWN], ["KeyS", DOWN],
  ["ArrowLeft", LEFT], ["KeyA", LEFT],
  ["ArrowRight", RIGHT], ["KeyD", RIGHT],
  ["KeyZ", FIRE], ["KeyJ", FIRE], ["Space", FIRE],
  ["KeyX", POWER], ["KeyK", POWER],
  ["Enter", START],
  ["Backspace", SELECT], ["Tab", SELECT],
]);

// Player 2 uses the numeric keypad. These are physical `code`s, so NumLock
// does not matter; a second gamepad covers keyboards without a keypad.
// (T / F / G / H cannot be reused: F toggles fullscreen here and the SDL
// bindings' J / K are player 1's fire and power aliases.)
const P2_KEY_BITS = new Map([
  ["Numpad8", UP], ["Numpad5", DOWN],
  ["Numpad4", LEFT], ["Numpad6", RIGHT],
  ["Numpad0", FIRE], ["NumpadDecimal", POWER],
  ["NumpadAdd", START], ["NumpadSubtract", SELECT],
]);

const stage = document.getElementById("stage");
const canvas = document.getElementById("screen");
const statusLine = document.getElementById("status");
const fitSelect = document.getElementById("fit");
const fullscreenButton = document.getElementById("fullscreen");
const saveButton = document.getElementById("save");
const loadButton = document.getElementById("load");
const installButton = document.getElementById("install");
const optionsButton = document.getElementById("options-open");
const optionsPanel = document.getElementById("options");
const hitsInput = document.getElementById("hits");
const startStageInput = document.getElementById("start-stage");
const invincibleBox = document.getElementById("invincible");
const twoAtOnceBox = document.getElementById("two-at-once");
const stage5SlowdownBox = document.getElementById("stage5-slowdown");
const ctx = canvas.getContext("2d");
const image = ctx.createImageData(WIDTH, HEIGHT);

// Fit mode is a page preference, not game state.
const FIT_KEY = "gradius.fit";
// One savestate slot per origin, versioned so a format change can drop it.
const SAVE_KEY = "gradius.savestate.v1";
const FIT_MODES = new Set(["integer", "fit", "stretch"]);
const savedFit = (() => {
  try {
    const value = localStorage.getItem(FIT_KEY);
    if (FIT_MODES.has(value)) return value;
  } catch {
    // Private mode or a blocked origin: keep the default.
  }
  return "integer";
})();
fitSelect.value = savedFit;

// Options panel values, a page preference like fit: ship hits (1..9, 1 is the
// cartridge), the stage a new game begins at (1..7), both ships at once in 2P
// and stage 5's slowdown (off is the port's default; on is the cartridge). The
// game takes them on its own schedule (see Backend::set_mods in
// crates/gradius-wasm/src/lib.rs).
const OPTIONS_KEY = "gradius.options.v1";
const HITS_MAX = 99;
const STAGE_MAX = 7;
// Whole hits in 1..=99; anything else reads as the cartridge's 1.
function hitsFrom(value) {
  const hits = Math.round(Number(value));
  return Number.isFinite(hits) ? Math.min(Math.max(hits, 1), HITS_MAX) : 1;
}
// Whole stage in 1..=7; anything else reads as the cartridge's 1.
function stageFrom(value) {
  const stage = Math.round(Number(value));
  return Number.isFinite(stage)
    ? Math.min(Math.max(stage, 1), STAGE_MAX)
    : 1;
}
const savedOptions = (() => {
  try {
    const value = JSON.parse(localStorage.getItem(OPTIONS_KEY));
    return {
      hits: hitsFrom(value?.hits),
      invincible: value?.invincible === true,
      stage: stageFrom(value?.stage),
      twoAtOnce: value?.twoAtOnce === true,
      stage5Slowdown: value?.stage5Slowdown === true,
    };
  } catch {
    // Private mode, a blocked origin or a damaged entry: keep the defaults.
    return {
      hits: 1,
      invincible: false,
      stage: 1,
      twoAtOnce: false,
      stage5Slowdown: false,
    };
  }
})();
hitsInput.value = String(savedOptions.hits);
invincibleBox.checked = savedOptions.invincible;
hitsInput.disabled = savedOptions.invincible;
startStageInput.value = String(savedOptions.stage);
twoAtOnceBox.checked = savedOptions.twoAtOnce;
stage5SlowdownBox.checked = savedOptions.stage5Slowdown;

const touch =
  navigator.maxTouchPoints > 0 ||
  window.matchMedia("(pointer: coarse)").matches;
if (touch) document.body.classList.add("touch");

let wasm = null;
let memory = null;
let framePtr = 0;
let paused = false;
let accumulator = 0;
let last = 0;
const keyHeld = new Set();
const p2Held = new Set();
const touchHeld = new Set();

// WebAudio state; samples stay in wasm until the first key or click.
let audioCtx = null;
let worklet = null;
let fallback = null;
let fallbackQueue = [];
let fallbackOffset = 0;
let muted = false;

// The status line shows the current mode unless a save/load notice replaces it
// for a moment, like the SDL frontend's SAVED/LOADED messages.
let noticeTimer = 0;
function baseStatus() {
  return muted ? "muted" : paused ? "paused" : "";
}
function showStatus(text) {
  clearTimeout(noticeTimer);
  statusLine.textContent = text;
}
function notice(text) {
  showStatus(text);
  noticeTimer = setTimeout(() => showStatus(baseStatus()), 2000);
}

// Size the canvas's CSS box from the free space in #stage. The 256x240
// backing buffer is unchanged; #stage centers it, so the leftover area is the
// letterbox. integer keeps whole-pixel steps (falling back to a fit below
// 1:1), fit uses the largest aspect-correct box, stretch fills the stage.
function applyFit() {
  const box = fitCanvas(stage.clientWidth, stage.clientHeight, fitSelect.value);
  canvas.style.width = `${box.width}px`;
  canvas.style.height = `${box.height}px`;
}

// Hand the saved options to the game; before the first frame and after every
// change. Before init there is no game yet, so start() calls this again.
function applyOptions() {
  if (!wasm) return;
  // 0 hits is the game's Invincible.
  const hits = invincibleBox.checked ? 0 : hitsFrom(hitsInput.value);
  wasm.set_mods(
    hits,
    twoAtOnceBox.checked ? 1 : 0,
    stageFrom(startStageInput.value),
    stage5SlowdownBox.checked ? 1 : 0,
  );
}

function saveOptions() {
  // A typed value outside its range (or blank) is shown as the value it means.
  hitsInput.value = String(hitsFrom(hitsInput.value));
  hitsInput.disabled = invincibleBox.checked;
  startStageInput.value = String(stageFrom(startStageInput.value));
  try {
    localStorage.setItem(
      OPTIONS_KEY,
      JSON.stringify({
        hits: hitsFrom(hitsInput.value),
        invincible: invincibleBox.checked,
        stage: stageFrom(startStageInput.value),
        twoAtOnce: twoAtOnceBox.checked,
        stage5Slowdown: stage5SlowdownBox.checked,
      }),
    );
  } catch (error) {
    console.warn("options write failed", error);
  }
  applyOptions();
}

function setFit(mode) {
  fitSelect.value = mode;
  try {
    localStorage.setItem(FIT_KEY, mode);
  } catch {
    // A blocked origin only loses the preference.
  }
  applyFit();
}

async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) await document.exitFullscreen();
    else await stage.requestFullscreen();
  } catch (error) {
    console.warn("fullscreen failed", error);
  }
}

// Standard-mapping pad bits: A fire, B power, Start, Back to select, d-pad or
// left stick to move. Same layout as the SDL frontend's controller layer.
const DEAD_ZONE = 0.5;
function padBits(pad) {
  if (!pad) return 0;
  const pressed = (index) => {
    const button = pad.buttons[index];
    return button !== undefined && button.pressed === true;
  };
  let bits = 0;
  if (pressed(0)) bits |= FIRE;
  if (pressed(1)) bits |= POWER;
  if (pressed(8)) bits |= SELECT;
  if (pressed(9)) bits |= START;
  if (pressed(12)) bits |= UP;
  if (pressed(13)) bits |= DOWN;
  if (pressed(14)) bits |= LEFT;
  if (pressed(15)) bits |= RIGHT;
  const x = pad.axes.length > 0 ? pad.axes[0] : 0;
  const y = pad.axes.length > 1 ? pad.axes[1] : 0;
  if (x < -DEAD_ZONE) bits |= LEFT;
  if (x > DEAD_ZONE) bits |= RIGHT;
  if (y < -DEAD_ZONE) bits |= UP;
  if (y > DEAD_ZONE) bits |= DOWN;
  return bits;
}

// Pad #1 drives player 1 and pad #2 player 2, like the SDL frontend.
function readButtons() {
  const pads = navigator.getGamepads ? navigator.getGamepads() : [];
  const open = [...pads].filter(Boolean);
  let p1 = padBits(open[0]);
  let p2 = padBits(open[1]);
  for (const bit of keyHeld) p1 |= bit;
  for (const bit of p2Held) p2 |= bit;
  for (const bit of touchHeld) p1 |= bit;
  return { p1, p2 };
}

// Copy the samples the last ticks rendered out of wasm, then release them.
// Nothing is scheduled before the first key or click (autoplay policy), and
// muted frames are consumed but not played so unmuting resumes the live cue.
function pumpAudio() {
  const available = wasm.audio_len();
  if (available === 0) return;
  const samples = new Float32Array(
    memory.buffer,
    wasm.audio_ptr(),
    available,
  ).slice();
  wasm.audio_consumed(available);
  if (muted) return;
  if (worklet) {
    worklet.port.postMessage({ samples }, [samples.buffer]);
  } else if (fallback) {
    fallbackQueue.push(samples);
  }
}

function takeFallback(out) {
  let written = 0;
  while (written < out.length && fallbackQueue.length > 0) {
    const head = fallbackQueue[0];
    const take = Math.min(out.length - written, head.length - fallbackOffset);
    out.set(head.subarray(fallbackOffset, fallbackOffset + take), written);
    written += take;
    fallbackOffset += take;
    if (fallbackOffset >= head.length) {
      fallbackQueue.shift();
      fallbackOffset = 0;
    }
  }
  out.fill(0, written);
}

async function startAudio() {
  if (audioCtx) {
    if (audioCtx.state === "suspended") await audioCtx.resume();
    return;
  }
  const Context = window.AudioContext || window.webkitAudioContext;
  if (!Context) return;
  try {
    audioCtx = new Context({ sampleRate: SAMPLE_RATE });
  } catch {
    audioCtx = new Context();
  }
  // Resume inside the gesture that created the context.
  audioCtx.resume().catch(() => {});
  try {
    await audioCtx.audioWorklet.addModule("audio-worklet.js");
    worklet = new AudioWorkletNode(audioCtx, "gradius-audio", {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [1],
    });
    worklet.connect(audioCtx.destination);
  } catch (error) {
    console.warn("AudioWorklet unavailable; using ScriptProcessor", error);
    fallback = audioCtx.createScriptProcessor(2048, 0, 1);
    fallback.onaudioprocess = (event) => {
      const out = event.outputBuffer.getChannelData(0);
      if (muted) {
        out.fill(0);
        return;
      }
      takeFallback(out);
    };
    fallback.connect(audioCtx.destination);
  }
}

function setMuted(value) {
  muted = value;
  if (worklet) worklet.port.postMessage({ muted });
  if (muted) {
    fallbackQueue = [];
    fallbackOffset = 0;
  }
  showStatus(baseStatus());
}

// Drop audio already handed to the worklet or ScriptProcessor for the timeline
// that was just replaced; the wasm queue is cleared by the load itself.
function flushAudio() {
  fallbackQueue = [];
  fallbackOffset = 0;
  if (worklet) worklet.port.postMessage({ flush: true });
}

function draw() {
  const rgba = new Uint8ClampedArray(memory.buffer, framePtr, FRAME_BYTES);
  image.data.set(rgba);
  ctx.putImageData(image, 0, 0);
}

// Encode the `GRSS` bytes as base64 for the one localStorage slot. Chunked so
// a long image never overflows String.fromCharCode's argument list.
function toBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

function fromBase64(text) {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function hasSavedState() {
  try {
    return localStorage.getItem(SAVE_KEY) !== null;
  } catch {
    return false;
  }
}

function saveState() {
  if (!wasm) return;
  const ptr = wasm.state_save();
  const len = wasm.state_len();
  if (!ptr || len === 0) {
    notice("SAVE ERROR");
    return;
  }
  const bytes = new Uint8Array(memory.buffer, ptr, len).slice();
  try {
    localStorage.setItem(SAVE_KEY, toBase64(bytes));
  } catch (error) {
    console.warn("savestate write failed", error);
    notice("SAVE ERROR");
    return;
  }
  loadButton.disabled = false;
  notice("SAVED");
}

// A corrupt entry (bad base64 or a refused image) reports LOAD ERROR and
// leaves the running game and the last drawn frame untouched.
function loadState() {
  if (!wasm) return;
  let bytes;
  try {
    const text = localStorage.getItem(SAVE_KEY);
    if (text === null) {
      notice("NO SAVE");
      return;
    }
    bytes = fromBase64(text);
  } catch (error) {
    console.warn("savestate read failed", error);
    notice("LOAD ERROR");
    return;
  }
  const ptr = wasm.alloc(bytes.length);
  if (ptr === 0) {
    notice("LOAD ERROR");
    return;
  }
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  const code = wasm.state_load(ptr, bytes.length);
  wasm.dealloc(ptr, bytes.length);
  if (code !== 0) {
    notice("LOAD ERROR");
    return;
  }
  flushAudio();
  accumulator = 0;
  notice("LOADED");
}

function frame(now) {
  if (paused) {
    accumulator = 0;
  } else {
    accumulator = Math.min(accumulator + (now - last), MAX_CATCH_UP_MS);
    const input = readButtons();
    let stepped = false;
    while (accumulator >= STEP_MS) {
      accumulator -= STEP_MS;
      framePtr = wasm.tick(input.p1, input.p2);
      stepped = true;
    }
    if (stepped) {
      pumpAudio();
      draw();
    }
  }
  last = now;
  requestAnimationFrame(frame);
}

window.addEventListener("keydown", (event) => {
  // The dialog owns the keyboard while it is open; the game stays paused.
  if (optionsPanel.open) return;
  if (event.code === "KeyP" && !event.repeat) {
    paused = !paused;
    showStatus(baseStatus());
    return;
  }
  if (event.code === "KeyM" && !event.repeat) {
    setMuted(!muted);
    return;
  }
  if (event.code === "KeyF" && !event.repeat) {
    event.preventDefault();
    toggleFullscreen();
    return;
  }
  if (event.code === "F5") {
    // F5 is the browser reload key; F5/F9 match the SDL frontend instead.
    event.preventDefault();
    saveState();
    return;
  }
  if (event.code === "F9") {
    event.preventDefault();
    loadState();
    return;
  }
  const bit = KEY_BITS.get(event.code);
  if (bit !== undefined) {
    keyHeld.add(bit);
    event.preventDefault();
    return;
  }
  const p2Bit = P2_KEY_BITS.get(event.code);
  if (p2Bit !== undefined) {
    p2Held.add(p2Bit);
    event.preventDefault();
  }
});

window.addEventListener("keyup", (event) => {
  const bit = KEY_BITS.get(event.code);
  if (bit !== undefined) {
    keyHeld.delete(bit);
    event.preventDefault();
    return;
  }
  const p2Bit = P2_KEY_BITS.get(event.code);
  if (p2Bit !== undefined) {
    p2Held.delete(p2Bit);
    event.preventDefault();
  }
});

for (const button of document.querySelectorAll("#touch button")) {
  const bit = Number(button.dataset.bit);
  button.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    touchHeld.add(bit);
    button.setPointerCapture(event.pointerId);
  });
  const release = (event) => {
    event.preventDefault();
    touchHeld.delete(bit);
  };
  button.addEventListener("pointerup", release);
  button.addEventListener("pointercancel", release);
  button.addEventListener("lostpointercapture", () => touchHeld.delete(bit));
}

if (!stage.requestFullscreen) fullscreenButton.hidden = true;
fullscreenButton.addEventListener("click", toggleFullscreen);
saveButton.addEventListener("click", saveState);
loadButton.addEventListener("click", loadState);
loadButton.disabled = !hasSavedState();

// Opening the panel pauses the game unless the player paused it already; the
// close resumes it only when the panel was what paused it.
let pausedForOptions = false;
optionsButton.addEventListener("click", () => {
  for (const held of [keyHeld, p2Held, touchHeld]) held.clear();
  pausedForOptions = !paused;
  paused = true;
  showStatus(baseStatus());
  optionsPanel.showModal();
});
optionsPanel.addEventListener("close", () => {
  if (pausedForOptions) paused = false;
  pausedForOptions = false;
  showStatus(baseStatus());
});
hitsInput.addEventListener("change", saveOptions);
invincibleBox.addEventListener("change", saveOptions);
startStageInput.addEventListener("change", saveOptions);
twoAtOnceBox.addEventListener("change", saveOptions);
stage5SlowdownBox.addEventListener("change", saveOptions);

// A browser that supports installation fires beforeinstallprompt; keep the
// event and replay it from the page's own button (it cannot be called later).
let installPrompt = null;
window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  installPrompt = event;
  installButton.hidden = false;
});
window.addEventListener("appinstalled", () => {
  installPrompt = null;
  installButton.hidden = true;
});
installButton.addEventListener("click", async () => {
  if (!installPrompt) return;
  await installPrompt.prompt();
  installPrompt = null;
  installButton.hidden = true;
});

// The service worker precaches the page, wasm module and pack for offline use.
if ("serviceWorker" in navigator) {
  navigator.serviceWorker
    .register("service-worker.js")
    .catch((error) => console.warn("service worker registration failed", error));
}
fitSelect.addEventListener("change", () => setFit(fitSelect.value));
document.addEventListener("fullscreenchange", () => {
  fullscreenButton.textContent =
    document.fullscreenElement ? "Exit fullscreen" : "Fullscreen";
  requestAnimationFrame(applyFit);
});

window.addEventListener("blur", () => {
  keyHeld.clear();
  p2Held.clear();
  touchHeld.clear();
});
window.addEventListener("resize", applyFit);
window.addEventListener("keydown", () => {
  startAudio().catch((error) => console.warn("audio start failed", error));
}, { once: true });
window.addEventListener("pointerdown", () => {
  startAudio().catch((error) => console.warn("audio start failed", error));
}, { once: true });

const forgetButton = document.getElementById("forget");
forgetButton.addEventListener("click", async () => {
  if (!confirm("Delete the game data from this browser? You will need your ROM again.")) return;
  await deletePack();
  localStorage.removeItem(SAVE_KEY);
  location.reload();
});

// The pack comes from this browser's storage, then from a server copy (local
// use), and otherwise from the ROM the player gives us.
async function obtainPack() {
  const stored = await loadPack().catch(() => null);
  if (stored) return stored;
  const served = await fetch("gradius.pak").catch(() => null);
  if (served?.ok) return new Uint8Array(await served.arrayBuffer());
  const pack = await askForRom();
  await savePack(pack).catch((error) => console.warn("could not store the pack", error));
  return pack;
}

// Show the setup screen until a ROM extracts; a wrong ROM asks again.
function askForRom() {
  const setup = document.getElementById("setup");
  const ask = document.getElementById("setup-ask");
  const busy = document.getElementById("setup-busy");
  const errorLine = document.getElementById("setup-error");
  const detailLine = document.getElementById("setup-detail");
  const stepLine = document.getElementById("setup-step");
  const bar = document.getElementById("setup-bar");
  const input = document.getElementById("setup-file");
  setup.hidden = false;
  statusLine.textContent = "";

  return new Promise((resolve) => {
    let working = false;
    async function take(file) {
      if (!file || working) return;
      working = true;
      errorLine.textContent = "";
      detailLine.textContent = "";
      ask.hidden = true;
      busy.hidden = false;
      try {
        const rom = new Uint8Array(await file.arrayBuffer());
        const pack = await extractInWorker(rom, (step, done, total) => {
          stepLine.textContent = `${STEP_LABELS[step] ?? "Working"}…`;
          bar.value = (step + done / Math.max(total, 1)) / STEP_LABELS.length;
        });
        setup.hidden = true;
        resolve(pack);
      } catch (error) {
        const explained = explainRomError(error.message);
        errorLine.textContent = explained.message;
        detailLine.textContent = explained.detail;
        ask.hidden = false;
        busy.hidden = true;
        working = false;
      }
    }
    document.getElementById("setup-pick").addEventListener("click", () => input.click());
    input.addEventListener("change", () => {
      const file = input.files[0];
      input.value = "";
      take(file);
    });
    setup.addEventListener("dragover", (event) => {
      event.preventDefault();
      setup.classList.add("over");
    });
    setup.addEventListener("dragleave", () => setup.classList.remove("over"));
    setup.addEventListener("drop", (event) => {
      event.preventDefault();
      setup.classList.remove("over");
      take(event.dataTransfer.files[0]);
    });
  });
}

// The extractor's messages are for developers; say what the player should do.
function explainRomError(text) {
  if (text.includes("not an iNES ROM")) {
    return { message: "This file is not a NES ROM.", detail: "" };
  }
  if (text.includes("not Gradius (E)")) {
    const expected = /expected SHA-256 ([0-9a-f]{64})/.exec(text);
    return {
      message: "This ROM is not Gradius (E). You need the European dump.",
      detail: expected ? `Expected SHA-256 ${expected[1]}` : "",
    };
  }
  return { message: text, detail: "" };
}

function extractInWorker(rom, onProgress) {
  return new Promise((resolve, reject) => {
    const worker = new Worker("extract-worker.js", { type: "module" });
    worker.onmessage = ({ data }) => {
      if (data.progress) return onProgress(...data.progress);
      worker.terminate();
      if (data.error) reject(new Error(data.error));
      else resolve(data.pack);
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message || "the extractor failed to start"));
    };
    worker.postMessage({ rom }, [rom.buffer]);
  });
}

async function start() {
  try {
    const wasmResponse = await fetch("gradius_wasm.wasm");
    if (!wasmResponse.ok) {
      throw new Error(`cannot load gradius_wasm.wasm (${wasmResponse.status})`);
    }
    const { instance } = await WebAssembly.instantiate(
      await wasmResponse.arrayBuffer(),
      {},
    );

    const pack = await obtainPack();

    wasm = instance.exports;
    memory = wasm.memory;

    // Copy the pack into a buffer the wasm allocator owns, then parse it.
    const ptr = wasm.alloc(pack.length);
    if (ptr === 0) {
      throw new Error("wasm could not allocate the pack buffer");
    }
    new Uint8Array(memory.buffer, ptr, pack.length).set(pack);
    const code = wasm.init(ptr, pack.length);
    wasm.dealloc(ptr, pack.length);
    if (code !== 0) {
      throw new Error("the pack was rejected by the wasm backend");
    }

    applyOptions();
    framePtr = wasm.tick(0, 0);
    draw();
    applyFit();
    // Never restore automatically: a saved state loads only on F9 or Load.
    showStatus(hasSavedState() ? "saved state ready (F9 or Load)" : "");
    last = performance.now();
    requestAnimationFrame(frame);
  } catch (error) {
    statusLine.textContent = error.message;
    console.error(error);
  }
}

start();
