"use strict";

// Plays the mono 48 kHz chunks gradius_wasm queues after every tick. Chunks
// arrive over the port; muting also drops whatever is still buffered so M is
// heard immediately.
class GradiusAudio extends AudioWorkletProcessor {
  constructor() {
    super();
    this.chunks = [];
    this.offset = 0;
    this.muted = false;
    this.port.onmessage = (event) => {
      const data = event.data;
      if (data.flush) {
        // A savestate load replaced the timeline; drop the old samples.
        this.chunks = [];
        this.offset = 0;
      } else if (data.muted !== undefined) {
        this.muted = data.muted;
        if (this.muted) {
          this.chunks = [];
          this.offset = 0;
        }
      } else if (data.samples) {
        this.chunks.push(data.samples);
      }
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0][0];
    if (this.muted) {
      out.fill(0);
      return true;
    }
    let written = 0;
    while (written < out.length && this.chunks.length > 0) {
      const chunk = this.chunks[0];
      const take = Math.min(out.length - written, chunk.length - this.offset);
      out.set(chunk.subarray(this.offset, this.offset + take), written);
      written += take;
      this.offset += take;
      if (this.offset >= chunk.length) {
        this.chunks.shift();
        this.offset = 0;
      }
    }
    out.fill(0, written);
    return true;
  }
}

registerProcessor("gradius-audio", GradiusAudio);
