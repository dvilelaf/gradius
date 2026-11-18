#!/usr/bin/env bash
# Run the generated NESRecomp host headlessly.
#
#   NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref \
#   GRADIUS_ROM="/path/to/Gradius (E).nes" \
#   tools/nesrecomp/run.sh --frames 3600 --input route.txt \
#       --hash-out native.hash --screenshot native.png
#
# --interp adds --interp-only, so the same route can be compared between the
# recompiled program and NESRecomp's interpreter. All other options are passed
# to the generated runner unchanged; see runner/cyc/cyc_host.c.
set -euo pipefail

BUILD="${NESRECOMP_BUILD:-}"
ROM="${GRADIUS_ROM:-}"
BIN=""
ARGS=()

while (($#)); do
    case "$1" in
        --build)   BUILD="${2:?--build needs a directory}"; shift 2 ;;
        --rom)     ROM="${2:?--rom needs a file}"; shift 2 ;;
        --binary)  BIN="${2:?--binary needs a file}"; shift 2 ;;
        --interp)  ARGS+=(--interp-only); shift ;;
        --native)  shift ;;
        -h|--help)
            sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *)         ARGS+=("$1"); shift ;;
    esac
done

: "${BUILD:?set NESRECOMP_BUILD or pass --build DIR}"
: "${ROM:?set GRADIUS_ROM or pass --rom FILE}"
[[ -f "$ROM" ]] || { echo "ROM not found: $ROM" >&2; exit 1; }
BIN="${BIN:-$BUILD/game/nes_game}"
[[ -x "$BIN" ]] || { echo "runner not found: $BIN (run setup.sh)" >&2; exit 1; }

# A bare run should still be a useful headless smoke test.
((${#ARGS[@]})) || ARGS=(--frames 600)

printf 'runner: %q %q' "$BIN" "$ROM" >&2
printf ' %q' "${ARGS[@]}" >&2
printf '\n' >&2
exec "$BIN" "$ROM" "${ARGS[@]}"
