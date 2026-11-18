#!/usr/bin/env python3
"""Check that a headless `--dump-audio` WAV still carries the triangle bass.

The frontend dumps the mixed soundtrack, not one voice at a time. The triangle
is the only bass voice in the captured music, so the check low-passes the mix
and fails when the band below ~200 Hz is silent. It also rejects an empty or
malformed container, so a broken extract or pack cannot pass as "no sound".

    gradius --pack assets/gradius.pak --frames 3000 --autoplay \
        --no-audio --dump-audio /tmp/audio.wav --scale 1
    python3 scripts/check-wav-triangle.py /tmp/audio.wav
"""

import array
import struct
import sys

# Full-scale RMS floor: the mixed soundtrack is much louder, but this rejects a
# muted run. The bass floor is far below the measured stage-1 triangle energy
# yet well above the rounding noise of a silent channel.
MIN_RMS = 0.01
MIN_BASS_RMS = 0.002
BASS_CUTOFF_HZ = 200


def read_wav(data):
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise SystemExit("not a RIFF/WAVE file")
    fmt = None
    pcm = None
    pos = 12
    while pos + 8 <= len(data):
        chunk = data[pos : pos + 4]
        size = struct.unpack("<I", data[pos + 4 : pos + 8])[0]
        body = data[pos + 8 : pos + 8 + size]
        if chunk == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif chunk == b"data":
            pcm = body
        pos += 8 + size + (size & 1)
    if fmt is None or pcm is None:
        raise SystemExit("WAV is missing fmt or data")
    audio_format, channels, sample_rate, _, _, bits = fmt
    if audio_format != 1 or channels != 1 or bits != 16:
        raise SystemExit(
            f"expected mono 16-bit PCM, got format {audio_format}, "
            f"{channels} channels, {bits} bits"
        )
    samples = array.array("h")
    samples.frombytes(pcm[: len(pcm) // 2 * 2])
    if sys.byteorder == "big":
        samples.byteswap()
    return sample_rate, samples


def rms(values):
    if not values:
        return 0.0
    return (sum(float(v) * float(v) for v in values) / len(values)) ** 0.5


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: check-wav-triangle.py PATH")
    with open(sys.argv[1], "rb") as handle:
        sample_rate, samples = read_wav(handle.read())
    if not samples:
        raise SystemExit("WAV has no samples")

    overall = rms(samples) / 32768.0
    span = max(1, round(sample_rate / BASS_CUTOFF_HZ))
    # Boxcar low-pass: the first null is at sample_rate/span = 200 Hz.
    acc = 0.0
    bass = []
    for index, value in enumerate(samples):
        acc += value
        if index >= span:
            acc -= samples[index - span]
        if index >= span - 1:
            bass.append(acc / span)
    low = rms(bass) / 32768.0

    print(
        f"wav: {len(samples)} samples at {sample_rate} Hz, "
        f"rms {overall:.4f}, bass rms {low:.4f}"
    )
    if overall < MIN_RMS:
        raise SystemExit(f"soundtrack is silent (rms {overall:.4f})")
    if low < MIN_BASS_RMS:
        raise SystemExit(f"triangle bass is silent (bass rms {low:.4f})")
    print("wav triangle check: ok")


main()
