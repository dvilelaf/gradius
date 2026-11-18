#!/usr/bin/env bash
# Records the oracle fixtures in tests/fixtures/ from your cartridge. Each file
# is written by the command its test or its docs name; the .script inputs are
# ours and stay versioned. Recording is deterministic: the same ROM gives the
# same bytes.
#
#   GRADIUS_ROM=<Gradius (E).nes> scripts/gen-fixtures.sh [--force] [DIR]
#
# Only the missing fixtures are recorded unless --force is given. DIR defaults
# to tests/fixtures. scripts/extract-data runs this after building the pack.
set -euo pipefail

force=0
out=tests/fixtures
for arg in "$@"; do
    case "$arg" in
        --force) force=1 ;;
        -*) echo "usage: scripts/gen-fixtures.sh [--force] [DIR]" >&2; exit 2 ;;
        *) out="$arg" ;;
    esac
done

if [[ -z "${GRADIUS_ROM:-}" ]]; then
    echo "gen-fixtures: GRADIUS_ROM is not set; point it at your Gradius (E) iNES image" >&2
    exit 1
fi
if [[ ! -f "$GRADIUS_ROM" ]]; then
    echo "gen-fixtures: no such file: $GRADIUS_ROM" >&2
    exit 1
fi
# Resolve before cd: relative paths mean relative to the caller.
rom_dir="$(cd "$(dirname "$GRADIUS_ROM")" && pwd)"
GRADIUS_ROM="$rom_dir/$(basename "$GRADIUS_ROM")"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { echo "gen-fixtures: cargo not found" >&2; exit 1; }
cargo build --release -p gradius-diff -p xtask

diff_bin=target/release/gradius-diff
xtask_bin=target/release/xtask
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

due() { [[ "$force" == 1 || ! -f "$out/$1" ]]; }

# record NAME [gradius-diff options...]: one oracle run, written under its name.
record() {
    local name=$1
    shift
    due "$name" || { echo "gen-fixtures: keep $name"; return; }
    local start=$SECONDS
    "$diff_bin" --rom "$GRADIUS_ROM" --emit-fixture "$work/$name" "$@" >/dev/null
    mv "$work/$name" "$out/$name"
    echo "gen-fixtures: $name ($((SECONDS - start))s)"
}

# trace NAME STAGE FRAMES FROM TO [HEADER]: a record-ending run kept from FROM to TO.
trace() {
    local name=$1 stage=$2 frames=$3 from=$4 to=$5 header=${6:-}
    due "$name" || { echo "gen-fixtures: keep $name"; return; }
    local start=$SECONDS
    "$xtask_bin" record-ending "$GRADIUS_ROM" --stage "$stage" --frames "$frames" \
        --out "$work/run.txt" >/dev/null
    {
        [[ -n "$header" ]] && printf '%b\n' "$header"
        awk -v from="$from" -v to="$to" \
            '{ split($1, kv, "="); if (kv[1] == "frame" && kv[2] + 0 >= from && kv[2] + 0 <= to) print }' \
            "$work/run.txt"
    } > "$work/$name"
    mv "$work/$name" "$out/$name"
    echo "gen-fixtures: $name ($((SECONDS - start))s)"
}

# Attract and stage 1: the attract input, and the harness default script for 600 frames.
record attract-800.state --frames 800 --from 130 --to 800 --script scripts/attract-800.txt
record stage1-600.state --frames 600

# Held-B power-up and the boss script (both keep-alives are analysis-only).
record powerup-1200.state --frames 1200 --from 292 --to 1199 --script scripts/powerup-1200.txt
record stage2-9601-12000.state --frames 12000 --from 9601 --to 12000 \
    --script scripts/boss-script.txt --keep-alive
record stage2-12001-19430.state --frames 19430 --from 12001 --to 19430 \
    --script scripts/boss-script.txt --keep-alive
record stage2-12161-15000.state --frames 15000 --from 12161 --to 15000 \
    --script scripts/boss-script.txt --keep-alive
record stage2-blocks-9601-19430.state --frames 19430 --from 9601 --to 19430 \
    --script scripts/stage2-blocks-script.txt --keep-alive
record stage3-19429-23000.state --frames 23000 --from 19429 --to 23000 \
    --script scripts/boss-script.txt --keep-alive

# Poked stage windows: --stage pokes $26 until the stage is playing.
record stage4-297-1797.state --stage 4 --frames 1798 --from 297 --to 1797
record stage5-297-8573.state --stage 5 --frames 8573 --from 297 --to 8573 --keep-alive
record stage5-8574-12003.state --stage 5 --frames 12003 --from 8574 --to 12003 --keep-alive
record stage6-297-1500.state --stage 6 --frames 1500 --from 297 --to 1500 --keep-alive
record stage7-297-6050.state --stage 7 --frames 6050 --from 297 --to 6050 --keep-alive

# Stage 7 clear and ending: the record-ending input, fire held and Start at 180.
record stage7-6051-6562.state --script tests/fixtures/stage7-6563-7024.script \
    --stage 7 --frames 6562 --from 6051 --to 6562 --keep-alive
record stage7-6563-7024.state --script tests/fixtures/stage7-6563-7024.script \
    --stage 7 --frames 7024 --from 6563 --to 7024 --keep-alive
record ending-7015-8290.state --script tests/fixtures/ending-7015-8290.script \
    --stage 7 --frames 8290 --from 7015 --to 8290 --keep-alive
trace ending-7015-8290.trace 7 8290 7015 8290 \
'# gradius-xtask record-ending trace, frames 7015-8290
# command: cargo xtask record-ending "$ROM" --stage 7 --frames 8290 --out /tmp/opencode/endfixture/ending7-8290.txt
# fields: frame state phase($1B) stage($19) scroll score lives loop($28) stagebyte($26) px py x39 x3a x66 beams'
trace stage7-6563-7024.trace 7 7024 6563 7024
