#!/usr/bin/env bash
# Release artifact smoke: build what we ship and run it the way a user would.
#
#   scripts/release-smoke.sh
#
# Builds the SDL binary and the wasm PWA in release, then checks the real pack
# alone: a headless boot to the title, Start accepted, the same hash from a
# directory holding nothing but the pack, the F5/F9 slot behind Xvfb, and the
# node web gates. The output of a run is recorded in docs/RELEASE.md.
#
# Needs python3 for the GRSS header, and Xvfb + xdotool for the F5/F9 check;
# that check is skipped (not failed) when either is missing.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/node-env.sh
. scripts/node-env.sh

target="${CARGO_TARGET_DIR:-target}"
pack="${PACK:-assets/gradius.pak}"
out="${OUT:-$target/release-smoke}"
mkdir -p "$out"
export CARGO_TARGET_DIR="$target"

step() { printf '\n== %s ==\n' "$*"; }

# The `frame=` field of a GRSS header: a u64 at offset 20.
grss_frame() {
    python3 -c 'import struct,sys;d=open(sys.argv[1],"rb").read();
assert d[:4]==b"GRSS", "not a GRSS image"
print(struct.unpack("<Q", d[20:28])[0])' "$1"
}

step "release build: SDL"
cargo build --release -p gradius-sdl
sdl=$(cd "$(dirname "$target/release/gradius")" && pwd)/gradius

step "release build: wasm and PWA"
PATH="$(dirname "$(find_node)"):$PATH" web/build.sh

step "headless boot to the title (60 frames)"
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
    "$sdl" --pack "$pack" --frames 60 --no-audio --scale 1 \
    --screenshot "$out/title.bmp" | tee "$out/title.txt"

step "Start accepted: autoplay scores, the attract run does not"
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
    "$sdl" --pack "$pack" --frames 600 --autoplay --no-audio --scale 1 \
    --screenshot "$out/start.bmp" | tee "$out/start.txt"
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
    "$sdl" --pack "$pack" --frames 600 --no-audio --scale 1 >"$out/attract.txt"
play_score=$(sed -n 's/.* score=\([0-9]*\) .*/\1/p' "$out/start.txt")
play_lives=$(sed -n 's/.* lives=\([0-9]*\) .*/\1/p' "$out/start.txt")
attract_score=$(sed -n 's/.* score=\([0-9]*\) .*/\1/p' "$out/attract.txt")
[[ $play_score -gt 0 && $play_lives -eq 3 ]] || {
    echo "start: autoplay did not reach gameplay: $(cat "$out/start.txt")" >&2
    exit 1
}
[[ $attract_score -eq 0 ]] || {
    echo "start: the attract run scored $attract_score without input" >&2
    exit 1
}
echo "start: gameplay score=$play_score lives=$play_lives, attract score=$attract_score"

step "deterministic: two runs print the same hash"
first=$(sed -n 's/.* hash=\(0x[0-9a-f]*\).*/\1/p' "$out/start.txt")
second=$(SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
    "$sdl" --pack "$pack" --frames 600 --autoplay --no-audio --scale 1 \
    | sed -n 's/.* hash=\(0x[0-9a-f]*\).*/\1/p')
[[ $first == "$second" ]] || { echo "hash $first != $second" >&2; exit 1; }
echo "hash: $first twice"

step "self-contained pack: only the pack in the working directory"
clean="$out/clean"
mkdir -p "$clean"
cp "$pack" "$clean/gradius.pak"
clean_hash=$(cd "$clean" && SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
    "$sdl" --pack gradius.pak --frames 600 --autoplay --no-audio --scale 1 \
    | sed -n 's/.* hash=\(0x[0-9a-f]*\).*/\1/p')
[[ $clean_hash == "$first" ]] || {
    echo "clean dir: hash $clean_hash != $first" >&2
    exit 1
}
echo "clean dir: hash $clean_hash matches a repo-root run"

step "F5/F9: save, play on, load, save again"
if ! command -v Xvfb >/dev/null || ! command -v xdotool >/dev/null; then
    echo "skip: Xvfb and xdotool are needed for the windowed hotkey check"
else
    slot="$clean/savestate.grs"
    xvfb-run -a --server-args="-screen 0 800x600x24" bash -eu -c '
        cd "$1"
        export SDL_VIDEODRIVER=x11 SDL_AUDIODRIVER=dummy
        "$2" --pack gradius.pak --scale 2 --no-audio >sdl.log 2>&1 &
        pid=$!
        wid=
        for _ in $(seq 1 50); do
            wid=$(xdotool search --name Gradius 2>/dev/null | head -1) || true
            [[ -n $wid ]] && break
            sleep 0.1
        done
        [[ -n $wid ]] || { echo "no window" >&2; kill $pid; exit 1; }
        xdotool windowfocus --sync "$wid"
        sleep 1
        xdotool key Return
        sleep 1
        xdotool key F5
        sleep 0.3
        cp savestate.grs saved.grs
        sleep 1.5
        xdotool key F9
        sleep 0.15
        xdotool key F5
        sleep 0.3
        xdotool key Escape
        wait $pid || true
    ' _ "$clean" "$sdl"
    [[ -f $slot ]] || { echo "F5 wrote no $slot" >&2; exit 1; }
    saved=$(grss_frame "$clean/saved.grs")
    reloaded=$(grss_frame "$slot")
    delta=$((reloaded - saved))
    # 1.5 s of play is ~90 frames. After F9 the slot must be the earlier frame.
    ((delta >= 0 && delta < 20)) || {
        echo "F9 did not restore the saved frame: $saved -> $reloaded" >&2
        exit 1
    }
    echo "savestate: frame $saved saved, frame $reloaded after F9 (delta $delta), $(wc -c <"$slot") bytes"
fi

step "PWA: fit, wasm smoke and manifest"
node_bin=$(find_node)
PATH="$(dirname "$node_bin"):$PATH"
node web/fit.test.mjs | tail -1
node web/smoke.mjs "$pack" | tail -1
node web/pwa.test.mjs | tail -1

step "artifacts"
ls -l "$sdl" web/gradius_wasm.wasm web/gradius.pak
