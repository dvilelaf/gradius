#!/usr/bin/env sh
# Headless smoke test: exactly 180 ticks, no window, no audio device.
#
# Usage: scripts/smoke.sh [pack]
#        PACK=other.pak scripts/smoke.sh
#
# Requires a pack built with `cargo xtask extract`; the pack is not committed.

set -eu

pack="${1:-${PACK:-assets/gradius.pak}}"
target="${CARGO_TARGET_DIR:-target}"
binary="$target/debug/gradius"

if [ ! -f "$pack" ]; then
    echo "smoke: missing pack: $pack" >&2
    echo "      create it with: cargo xtask extract \"<rom>\" $pack" >&2
    exit 1
fi

if [ ! -x "$binary" ]; then
    cargo build -p gradius-sdl
fi

# The headless path never calls SDL_Init, but set the dummy drivers anyway so
# the script is safe to reuse for a quick windowed check.
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
    "$binary" --pack "$pack" --frames 180 --no-audio --scale 1
