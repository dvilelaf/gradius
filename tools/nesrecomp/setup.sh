#!/usr/bin/env bash
# Build the pinned NESRecomp recompiler and a headless Gradius (E) host.
#
#   NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref \
#   GRADIUS_ROM="/path/to/Gradius (E).nes" \
#   tools/nesrecomp/setup.sh
#
# Everything (checkout, generated C, binaries) stays below $NESRECOMP_BUILD.
set -euo pipefail

# NESRecomp commit used for the Gradius comparison. Update only with the doc.
PIN=466249489898be9f92aa369de4bbc5aad0d8feb2
URL=https://github.com/mstan/nesrecomp.git

: "${NESRECOMP_BUILD:?set NESRECOMP_BUILD to the artifact/build directory}"
: "${GRADIUS_ROM:?set GRADIUS_ROM to the Gradius (E) iNES image}"
[[ -f "$GRADIUS_ROM" ]] || { echo "ROM not found: $GRADIUS_ROM" >&2; exit 1; }
ROM="$(cd "$(dirname "$GRADIUS_ROM")" && pwd -P)/$(basename "$GRADIUS_ROM")"

for tool in git cmake cc; do
    command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done
command -v python3 >/dev/null || { echo "missing python3" >&2; exit 1; }
python3 -c 'import sys; raise SystemExit(sys.version_info < (3, 11))' ||
    { echo "NESRecomp's CMake bridge needs Python 3.11+" >&2; exit 1; }

SRC="${NESRECOMP_SRC:-$NESRECOMP_BUILD/nesrecomp}"
COMMIT="${NESRECOMP_COMMIT:-$PIN}"
JOBS="${NESRECOMP_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}"
mkdir -p "$NESRECOMP_BUILD"

if [[ -e "$SRC/.git" ]]; then
    if ! git -C "$SRC" cat-file -e "$COMMIT^{commit}" 2>/dev/null; then
        git -C "$SRC" fetch origin
    fi
else
    git clone "$URL" "$SRC"
fi
git -C "$SRC" checkout --detach "$COMMIT"
actual="$(git -C "$SRC" rev-parse HEAD)"
[[ "$actual" == "$COMMIT" ]] ||
    { echo "checkout is $actual, expected $COMMIT" >&2; exit 1; }
echo "nesrecomp: $actual"

# 1. Recompiler (pure C11, no dependencies).
if command -v ninja >/dev/null; then
    cmake -S "$SRC/recompiler" -B "$NESRECOMP_BUILD/recompiler" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release
else
    cmake -S "$SRC/recompiler" -B "$NESRECOMP_BUILD/recompiler" \
        -DCMAKE_BUILD_TYPE=Release
fi
cmake --build "$NESRECOMP_BUILD/recompiler" --parallel "$JOBS"

# 2. Game configuration: cycle backend, fixed prefix, no title-specific hooks.
CONFIG="$NESRECOMP_BUILD/game.toml"
cat > "$CONFIG" <<'EOF'
[game]
output_prefix = "gradius-e"
cycle_accurate = true
EOF

# 3. Headless host. Configure-time generation writes the recompiled C below
#    $NESRECOMP_BUILD/game/cycle-nes_game/<key>/generated/.
GAME_BUILD="$NESRECOMP_BUILD/game"
if command -v ninja >/dev/null; then
    cmake -S "$SRC/runner/cyc/project" -B "$GAME_BUILD" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DNESRECOMP_ROM="$ROM" \
        -DNESRECOMP_GAME_CONFIG="$CONFIG" \
        -DNESRECOMP_HOST_COMPILER="$NESRECOMP_BUILD/recompiler/NESRecomp" \
        -DNESRECOMP_HEADLESS=ON
else
    cmake -S "$SRC/runner/cyc/project" -B "$GAME_BUILD" \
        -DCMAKE_BUILD_TYPE=Release \
        -DNESRECOMP_ROM="$ROM" \
        -DNESRECOMP_GAME_CONFIG="$CONFIG" \
        -DNESRECOMP_HOST_COMPILER="$NESRECOMP_BUILD/recompiler/NESRecomp" \
        -DNESRECOMP_HEADLESS=ON
fi
cmake --build "$GAME_BUILD" --parallel "$JOBS"

echo "host:     $GAME_BUILD/nes_game"
echo "generated: $GAME_BUILD/cycle-nes_game/*/generated"
echo "rom:      $ROM"
