#!/bin/bash
# packaging/linux/smoketest.sh [--first-run] [AppImage]: the AppImage in a clean ubuntu:22.04 container (no Rust, no
# SDL installed), dummy video and audio, no FUSE (--appimage-extract-and-run).
#   default      --help, then 120 frames from the pack at $GRADIUS_PACK (default assets/gradius.pak), mounted read-only
#   --first-run  first start: the ROM at $GRADIUS_ROM mounted read-only, an empty temporary GRADIUS_DATA,
#                `--rom <path> --frames 120 --no-audio`; gradius.pak must appear in the data folder
#                (needs the first-run screen; before it lands this case fails)
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRST_RUN=0
[ "${1:-}" = --first-run ] && { FIRST_RUN=1; shift; }
PACK=$(realpath "${GRADIUS_PACK:-assets/gradius.pak}" 2>/dev/null || true)  # resolved before leaving the repo root
ROM=${GRADIUS_ROM:+$(realpath "$GRADIUS_ROM")}
APP=$(realpath "${1:-$(ls dist/Gradius-*-x86_64.AppImage | head -1)}")
[ -x "$APP" ] || { echo "smoketest: no AppImage at $APP (scripts/package-appimage.sh)"; exit 2; }

# Own working directory: nothing of the caller's (assets/gradius.pak) can leak in.
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

run() {  # podman args..., then the command
    podman run --rm -v "$APP:/app/Gradius.AppImage:ro" -e SDL_VIDEODRIVER=dummy -e SDL_AUDIODRIVER=dummy \
        "$@"
}
IMG=docker.io/library/ubuntu:22.04
APPRUN="/app/Gradius.AppImage --appimage-extract-and-run"

run $IMG $APPRUN --help >/dev/null
echo "PASS --help"

if [ $FIRST_RUN = 0 ]; then
    run -v "$PACK:/data/gradius.pak:ro" $IMG $APPRUN --pack /data/gradius.pak --frames 120 --no-audio
    echo "PASS 120 frames from the pack"
else
    : "${ROM:?GRADIUS_ROM: the ROM for the first start}"
    DATA=$WORK/data
    mkdir "$DATA"
    run -v "$ROM:/rom/gradius.nes:ro" -v "$DATA:/data" -e GRADIUS_DATA=/data $IMG \
        $APPRUN --rom /rom/gradius.nes --frames 120 --no-audio
    [ -s "$DATA/gradius.pak" ] || { echo "FAIL first start left no gradius.pak in GRADIUS_DATA"; exit 1; }
    echo "PASS first start wrote gradius.pak"
fi
