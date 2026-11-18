#!/usr/bin/env bash
# Compare NESRecomp, the cartridge oracle and the native port pixel by pixel.
#
#   NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref \
#   GRADIUS_ROM="/path/to/Gradius (E).nes" \
#   tools/nesrecomp/compare-port.sh --frames 601 --every 10
#
# Runs three pictures for the same frame window and compares them with
# compare.py: NESRecomp vs oracle, native vs oracle, native vs NESRecomp.
# Without --script every run is attract/no input. A --script in gradius-diff
# form (FRAME:buttons[:duration]) also drives NESRecomp: the script converts it
# to NESRecomp's `<frame> [buttons]` change schedule. The standalone gradius
# binary has no input flag, so it is only checked in the no-input window.
#
# PNGs, hashes and report.txt stay in --out (default /tmp/opencode/xcompare).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"

BUILD="${NESRECOMP_BUILD:-}"
ROM="${GRADIUS_ROM:-}"
PACK="$ROOT/assets/gradius.pak"
OUT=/tmp/opencode/xcompare
FRAMES=601
EVERY=10
SCRIPT=""
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
NATIVE="$TARGET/release/gradius"
DIFF="$TARGET/release/gradius-diff"

usage() {
    cat <<'EOF'
usage: compare-port.sh [OPTIONS]

  --build DIR    NESRecomp build root (default $NESRECOMP_BUILD)
  --rom FILE     Gradius (E) iNES image (default $GRADIUS_ROM)
  --pack FILE    native asset pack (default assets/gradius.pak)
  --out DIR      output directory (default /tmp/opencode/xcompare)
  --frames N     frames to run and compare, 0..N-1 (default 601)
  --every N      sample every Nth frame (default 10)
  --script FILE  gradius-diff FRAME:buttons[:duration] input, converted for
                 NESRecomp; default is attract/no input
  --native FILE  gradius binary (default $CARGO_TARGET_DIR/release/gradius)
  --diff FILE    gradius-diff binary (default $CARGO_TARGET_DIR/release/gradius-diff)
EOF
}

while (($#)); do
    case "$1" in
        --build)  BUILD="${2:?--build needs a directory}"; shift 2 ;;
        --rom)    ROM="${2:?--rom needs a file}"; shift 2 ;;
        --pack)   PACK="${2:?--pack needs a file}"; shift 2 ;;
        --out)    OUT="${2:?--out needs a directory}"; shift 2 ;;
        --frames) FRAMES="${2:?--frames needs a number}"; shift 2 ;;
        --every)  EVERY="${2:?--every needs a number}"; shift 2 ;;
        --script) SCRIPT="${2:?--script needs a file}"; shift 2 ;;
        --native) NATIVE="${2:?--native needs a file}"; shift 2 ;;
        --diff)   DIFF="${2:?--diff needs a file}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

: "${BUILD:?set NESRECOMP_BUILD or pass --build DIR}"
: "${ROM:?set GRADIUS_ROM or pass --rom FILE}"
[[ -f "$ROM" ]] || { echo "ROM not found: $ROM" >&2; exit 1; }
[[ -x "$BUILD/game/nes_game" ]] || { echo "NESRecomp host missing (run setup.sh)" >&2; exit 1; }
[[ -f "$PACK" ]] || { echo "asset pack not found: $PACK (run cargo xtask extract)" >&2; exit 1; }
[[ -x "$NATIVE" ]] || { echo "native binary not found: $NATIVE" >&2; exit 1; }
[[ -x "$DIFF" ]] || { echo "gradius-diff not found: $DIFF" >&2; exit 1; }
[[ "$FRAMES" =~ ^[0-9]+$ ]] && ((FRAMES > 0)) || { echo "--frames must be positive" >&2; exit 1; }
[[ "$EVERY" =~ ^[0-9]+$ ]] && ((EVERY > 0)) || { echo "--every must be positive" >&2; exit 1; }
if [[ -n "$SCRIPT" ]]; then
    [[ -f "$SCRIPT" ]] || { echo "script not found: $SCRIPT" >&2; exit 1; }
fi
python3 -c 'import PIL' 2>/dev/null ||
    { echo "compare.py needs Pillow: python3 -m pip install pillow" >&2; exit 1; }

mkdir -p "$OUT/nesrecomp" "$OUT/png" "$OUT/port"
rm -f "$OUT"/nesrecomp/*.png "$OUT"/png/*.png "$OUT"/port/*.bmp "$OUT"/port/*.png


# Attract: an empty gradius-diff script (its built-in default presses Start).
EMPTY="$OUT/attract.script"
: > "$EMPTY"
DIFF_SCRIPT="${SCRIPT:-$EMPTY}"

# NESRecomp's --input holds each line's buttons until the next line, so one
# change line per boundary is enough. An empty script means no --input at all.
NES_INPUT=()
if [[ -n "$SCRIPT" ]]; then
    NES_INPUT_FILE="$OUT/nesrecomp-input.txt"
    python3 - "$SCRIPT" "$NES_INPUT_FILE" <<'PY'
import sys

presses, out = [], []
for raw in open(sys.argv[1], encoding="utf-8"):
    line = raw.split("#", 1)[0].strip()
    if not line:
        continue
    at, buttons, *rest = line.split(":")
    duration = int(rest[0]) if rest else 6
    if duration <= 0:
        raise SystemExit(f"line {line!r}: duration must be positive")
    name = "+".join(b.strip().upper() for b in buttons.replace(",", "+").split("+") if b.strip())
    presses.append((int(at), duration, name))

def held(frame):
    return "+".join(sorted({n for at, d, n in presses if at <= frame < at + d})) or "-"

# Emit a line only where the held set changes, starting at frame 0.
for frame in range(0, max([at + d for at, d, _ in presses] or [0])):
    buttons = held(frame)
    if not out or out[-1][1] != buttons:
        out.append((frame, buttons))
with open(sys.argv[2], "w", encoding="utf-8") as f:
    for frame, buttons in out:
        f.write(f"{frame} {buttons}\n")
PY
    NES_INPUT=(--input "$NES_INPUT_FILE")
fi

echo "== NESRecomp (recompiled host)"
"$HERE/run.sh" --build "$BUILD" --rom "$ROM" \
    --frames "$FRAMES" --screenshot "$OUT/nesrecomp/frame.png" \
    --shot-every "$EVERY" --hash-out "$OUT/nesrecomp.hash" "${NES_INPUT[@]+"${NES_INPUT[@]}"}"

echo "== oracle and native lockstep (gradius-diff)"
# It exits 1 when the state comparison diverges; the pixels are still written.
"$DIFF" --rom "$ROM" --pack "$PACK" --frames "$FRAMES" --script "$DIFF_SCRIPT" \
    --png-dir "$OUT/png" --png-every "$EVERY" > "$OUT/diff.txt" 2>&1 || true
tail -3 "$OUT/diff.txt" | sed 's/^/   /'

# The shipped binary only writes its final frame, and has no input flag.
if [[ -z "$SCRIPT" ]]; then
    echo "== standalone port (final frame)"
    "$NATIVE" --pack "$PACK" --frames "$FRAMES" --no-audio \
        --screenshot "$OUT/port/final.bmp" | sed 's/^/   /'
fi

sampled() {
    local frame
    for ((frame = 0; frame < FRAMES; frame += EVERY)); do
        printf '%s\n' "$frame"
    done
}

exact() {
    python3 "$HERE/compare.py" "$1" "$2" | awk '
        /^identical:/ { print "100.0"; exit }
        /^exact:/    { gsub(/%/, "", $2); print $2; exit }'
    return 0
}

csv="$OUT/pixels.csv"
echo "frame,a_nesrecomp_vs_oracle,b_native_vs_oracle,c_native_vs_nesrecomp" > "$csv"
report="$OUT/report.txt"
: > "$report"
while read -r frame; do
    nes="$OUT/nesrecomp/frame_$(printf '%05d' "$frame").png"
    oracle="$OUT/png/oracle-$(printf '%04d' "$frame").png"
    native="$OUT/png/native-$(printf '%04d' "$frame").png"
    for file in "$nes" "$oracle" "$native"; do
        [[ -f "$file" ]] || { echo "missing $file (sample window out of step)" >&2; exit 1; }
    done
    a="$(exact "$nes" "$oracle")"
    b="$(exact "$native" "$oracle")"
    c="$(exact "$native" "$nes")"
    echo "$frame,$a,$b,$c" >> "$csv"
    printf 'frame %4d: NESRecomp/oracle %6s%%  native/oracle %6s%%  native/NESRecomp %6s%%\n' \
        "$frame" "$a" "$b" "$c" >> "$report"
done < <(sampled)

# Per-region detail for the worst frames of each pair.
worst="$OUT/worst.txt"
python3 - "$csv" <<'PY' > "$worst"
import sys

rows = []
with open(sys.argv[1], encoding="utf-8") as f:
    next(f)
    for line in f:
        frame, a, b, c = line.strip().split(",")
        rows.append((int(frame), float(a), float(b), float(c)))

names = ["NESRecomp vs oracle", "native vs oracle", "native vs NESRecomp"]
for column, name in enumerate(names, start=1):
    rated = sorted(rows, key=lambda row: (row[column], row[0]))
    print(f"{name}: mean {sum(r[column] for r in rows) / len(rows):.4f}%, "
          f"worst {rated[0][column]:.4f}% at frame {rated[0][0]}, "
          f"best {rated[-1][column]:.4f}%")
    for frame, a, b, c in rated[:5]:
        print(f"  F{frame}: {a:.4f} / {b:.4f} / {c:.4f}")
PY
cat "$worst" >> "$report"

for column in 1 2 3; do
    frame="$(python3 - "$csv" "$column" <<'PY'
import sys
rows = []
with open(sys.argv[1], encoding="utf-8") as f:
    next(f)
    rows = [line.strip().split(",") for line in f]
column = int(sys.argv[2])
print(min(rows, key=lambda r: (float(r[column]), int(r[0])))[0])
PY
)"
    nes="$OUT/nesrecomp/frame_$(printf '%05d' "$frame").png"
    oracle="$OUT/png/oracle-$(printf '%04d' "$frame").png"
    native="$OUT/png/native-$(printf '%04d' "$frame").png"
    {
        echo
        case "$column" in
            1)
                echo "== worst frame $frame: NESRecomp vs oracle"
                python3 "$HERE/compare.py" "$nes" "$oracle" --regions 16x15 || true
                ;;
            2)
                echo "== worst frame $frame: native vs oracle"
                python3 "$HERE/compare.py" "$native" "$oracle" --regions 16x15 || true
                ;;
            3)
                echo "== worst frame $frame: native vs NESRecomp"
                python3 "$HERE/compare.py" "$native" "$nes" --regions 16x15 || true
                ;;
        esac
    } >> "$report"
done

# The standalone binary must agree with the in-process native frames.
if [[ -z "$SCRIPT" ]] && (( (FRAMES - 1) % EVERY == 0 )); then
    last=$((FRAMES - 1))
    {
        echo
        echo "== standalone port final frame (BMP) vs NESRecomp/oracle"
        bmp="$OUT/port/final.bmp"
        python3 - "$bmp" "$OUT/port/final.png" <<'PY'
import sys
from PIL import Image
Image.open(sys.argv[1]).convert("RGBA").save(sys.argv[2])
PY
        final_native="$OUT/png/native-$(printf '%04d' "$last").png"
        final_oracle="$OUT/png/oracle-$(printf '%04d' "$last").png"
        final_nes="$OUT/nesrecomp/frame_$(printf '%05d' "$last").png"
        echo "port vs lockstep native: $(exact "$OUT/port/final.png" "$final_native")%"
        echo "port vs oracle (F$last): $(exact "$OUT/port/final.png" "$final_oracle")%"
        echo "port vs NESRecomp (F$last): $(exact "$OUT/port/final.png" "$final_nes")%"
    } | tee -a "$report"
fi

echo
cat "$worst"
echo
echo "report: $report"
echo "frames: $csv"
