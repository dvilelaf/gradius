#!/usr/bin/env bash
# Measures the 60.0988 Hz (16.639 ms) frame budget on this machine.
#
#   scripts/bench.sh [--frames N] [--repeat N] [--pack PATH] [--rom PATH]
#                    [--no-build] [--no-wasm]
#
# Workloads, each repeated (default 3) with the median reported:
#   core     Game::step 10k times, no window, no audio, no hashing per frame
#   sdl      gradius --frames 10000 --no-audio, the real frontend runner
#   oracle   gradius-nes reference emulator, 10k frames (development only)
#
# Also prints the wasm module size, the asset pack size and gzip sizes, which
# are what a browser or mobile build actually downloads.
#
# GRADIUS_ROM supplies the cartridge for the oracle row; without it the row is
# skipped. CARGO_TARGET_DIR defaults to /tmp/opencode/target-bench; OUT to
# /tmp/opencode/bench. The pack defaults to assets/gradius.pak.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/tmp/opencode/target-bench}
target=$CARGO_TARGET_DIR
frames=10000
repeat=3
pack=${GRADIUS_PACK:-assets/gradius.pak}
rom=${GRADIUS_ROM:-}
out=${OUT:-/tmp/opencode/bench}
build=1
wasm=1
budget_ms=16.6393

while (($#)); do
    case $1 in
        --frames) frames=${2:?--frames needs a value}; shift 2 ;;
        --repeat) repeat=${2:?--repeat needs a value}; shift 2 ;;
        --pack) pack=${2:?--pack needs a value}; shift 2 ;;
        --rom) rom=${2:?--rom needs a value}; shift 2 ;;
        --out) out=${2:?--out needs a value}; shift 2 ;;
        --no-build) build=0; shift ;;
        --no-wasm) wasm=0; shift ;;
        -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
        *) echo "bench: unknown argument $1" >&2; exit 2 ;;
    esac
done

if ((repeat < 1 || repeat % 2 == 0)); then
    echo "bench: --repeat must be an odd number (median of N)" >&2
    exit 2
fi
if [[ ! -f $pack ]]; then
    echo "bench: missing pack: $pack (create it with cargo xtask extract)" >&2
    exit 2
fi
mkdir -p "$out"

step() { printf '\n== %s ==\n' "$*" >&2; }

median() {
    sort -n | awk '{ v[NR] = $1 } END { print v[(NR + 1) / 2] }'
}

# One sample of a workload: CPU time (user+sys) and max RSS. CPU time is used
# rather than wall clock because this host is shared; it equals wall clock on
# an idle machine and does not turn another job's scheduling into our number.
collect() { # NAME command...
    local name=$1
    shift
    local user sys rss ms rc=0
    /usr/bin/time -f "%U %S %M" -o "$out/time.tmp" "$@" >"$out/run.out" 2>"$out/run.err" || rc=$?
    if ((rc != 0)); then
        echo "bench: command failed (exit $rc): $*" >&2
        cat "$out/run.err" >&2
        exit 1
    fi
    read -r user sys rss <"$out/time.tmp"
    ms=$(awk -v u="$user" -v s="$sys" 'BEGIN { printf "%.0f", (u + s) * 1000 }')
    echo "$ms" >>"$out/$name.times"
    echo "$rss" >>"$out/$name.rss"
}

if ((build)); then
    step "build release"
    cargo build --release -p gradius-sdl -p gradius-nes -p gradius-cart
    if ((wasm)); then
        if rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
            cargo build --release -p gradius-wasm --target wasm32-unknown-unknown
        else
            echo "bench: wasm32-unknown-unknown not installed; skipping wasm size" >&2
            wasm=0
        fi
    fi

    step "build loop harness"
    newest() { ls -t "$target"/release/deps/lib"$1"-*.rlib 2>/dev/null | head -1; }
    core_rlib=$(newest gradius_core)
    assets_rlib=$(newest gradius_assets)
    nes_rlib=$(newest gradius_nes)
    cart_rlib=$(newest gradius_cart)
    if [[ -z $core_rlib || -z $assets_rlib || -z $nes_rlib || -z $cart_rlib ]]; then
        echo "bench: cannot find release rlibs under $target/release/deps" >&2
        exit 1
    fi
    rustc --edition 2021 -C opt-level=3 -L "dependency=$target/release/deps" \
        --extern "gradius_core=$core_rlib" \
        --extern "gradius_assets=$assets_rlib" \
        --extern "gradius_nes=$nes_rlib" \
        --extern "gradius_cart=$cart_rlib" \
        -o "$target/release/gradius-bench" scripts/bench.rs
fi

bench="$target/release/gradius-bench"
sdl="$target/release/gradius"
for binary in "$bench" "$sdl"; do
    [[ -x $binary ]] || { echo "bench: missing $binary (run without --no-build)" >&2; exit 1; }
done

step "measure"
declare -A total_ms rss_kb
rm -f "$out"/*.times "$out"/*.rss
# Interleave the workloads so a load spike hits every row, not just one.
for ((i = 0; i < repeat; i++)); do
    collect core "$bench" core "$pack" "$frames"
    collect sdl env SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
        "$sdl" --pack "$pack" --frames "$frames" --no-audio --scale 1
    if [[ -n $rom ]]; then
        [[ -f $rom ]] || { echo "bench: no ROM at $rom" >&2; exit 2; }
        collect oracle "$bench" oracle "$rom" "$frames"
    fi
done
for name in core sdl oracle; do
    [[ -s $out/$name.times ]] || continue
    total_ms[$name]=$(median <"$out/$name.times")
    rss_kb[$name]=$(median <"$out/$name.rss")
    printf 'bench: %-6s %6s ms  %5s kB max rss\n' \
        "$name" "${total_ms[$name]}" "${rss_kb[$name]}" >&2
done
if [[ -z $rom ]]; then
    echo "bench: oracle skipped (set GRADIUS_ROM or pass --rom)" >&2
fi

step "sizes"
wasm_file="$target/wasm32-unknown-unknown/release/gradius_wasm.wasm"
if ((wasm)) && [[ -f $wasm_file ]]; then
    wasm_bytes=$(wc -c <"$wasm_file")
    wasm_gzip=$(gzip -9 -c "$wasm_file" | wc -c)
else
    wasm_bytes="n/a"
    wasm_gzip="n/a"
fi
pack_bytes=$(wc -c <"$pack")
pack_gzip=$(gzip -9 -c "$pack" | wc -c)

report="$out/results.md"
{
    echo "# Performance budget on $(hostname)"
    echo
    echo "Machine: $(grep -m1 'model name' /proc/cpuinfo | sed 's/.*: //'), $(nproc) threads."
    echo "Budget: 60.0988 Hz = $budget_ms ms/frame. Median of $repeat runs, $frames frames each."
    echo "Time is CPU (user+sys) from /usr/bin/time, which equals wall clock on an idle host."
    echo
    echo "| workload | CPU time (ms) | ms/frame | budget used | headroom | max RSS |"
    echo "| --- | ---: | ---: | ---: | ---: | ---: |"
    for name in core sdl oracle; do
        [[ -n ${total_ms[$name]:-} ]] || continue
        case $name in
            core) label="core Game::step (loop)" ;;
            sdl) label="SDL headless (whole process)" ;;
            oracle) label="gradius-nes oracle (loop)" ;;
        esac
        awk -v n="$label" -v t="${total_ms[$name]}" -v f="$frames" \
            -v r="${rss_kb[$name]}" -v b="$budget_ms" 'BEGIN {
                per = t / f
                printf "| %s | %d | %.4f | %.2f%% | %.0fx | %.1f MB |\n",
                    n, t, per, 100 * per / b, b / per, r / 1024
            }'
    done
    echo
    echo "| artifact | bytes | gzip -9 |"
    echo "| --- | ---: | ---: |"
    echo "| wasm module | $wasm_bytes | $wasm_gzip |"
    echo "| asset pack | $pack_bytes | $pack_gzip |"
} >"$report"

cat "$report"
echo
echo "bench: wrote $report and raw runs under $out" >&2
