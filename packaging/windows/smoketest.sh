#!/usr/bin/env bash
# Smoke test of the Windows zip under wine with a fresh prefix.
#   packaging/windows/smoketest.sh <zip> <pack> [--first-run <rom>]
# --first-run also checks that `--rom` builds gradius.pak next to the .exe
# (needs the first-run screen; not expected to pass before it lands).
set -euo pipefail

zip=$(realpath "${1:?usage: smoketest.sh <zip> <pack> [--first-run <rom>]}")
pack=$(realpath "${2:?pack path}")
first_run=
rom=
if [ "${3:-}" = "--first-run" ]; then
    first_run=1
    rom=$(realpath "${4:?--first-run needs a rom path}")
fi

work=$(mktemp -d /tmp/gradius-winsmoke.XXXXXX)
trap 'rm -rf "$work"' EXIT
export WINEPREFIX=$work/prefix WINEDEBUG=-all
export SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy

if unzip -l "$zip" | grep -Ei '\.(pak|nes|grs|state|trace)$|icons/'; then
    echo "FAIL: zip carries ROM-derived data" >&2
    exit 1
fi

# Run from the empty work folder: a caller's assets/gradius.pak would select development mode.
cd "$work"
unzip -q "$zip" -d "$work/unz"
exe=$(echo "$work"/unz/*/gradius.exe)

wine "$exe" --help >/dev/null
echo "ok: --help"
wine "$exe" --pack "$pack" --frames 120 --no-audio
echo "ok: --pack --frames 120"

if [ -n "$first_run" ]; then
    rm -f "$(dirname "$exe")/gradius.pak"
    wine "$exe" --rom "$rom" --frames 120 --no-audio
    [ -s "$(dirname "$exe")/gradius.pak" ] || { echo "FAIL: no gradius.pak next to the exe" >&2; exit 1; }
    echo "ok: first run created gradius.pak"
fi
