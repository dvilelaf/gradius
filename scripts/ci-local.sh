#!/usr/bin/env bash
# Local mirror of .github/workflows/ci.yml.
#
#   scripts/ci-local.sh fast   formatting, clippy and the parity test
#   scripts/ci-local.sh full   fast plus tests, wasm, web build and SDL (default)
#
# With GRADIUS_ROM set, scripts/extract-data builds assets/gradius.pak first (a
# no-op when it is current). Without a pack, the steps that need extracted data
# are skipped and listed at the end. Cargo runs are serialized with the same lock
# as scripts/verify. The SDL steps are skipped when the SDL2 development headers
# are missing.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/node-env.sh
. scripts/node-env.sh

mode="${1:-full}"
lock="${TMPDIR:-/tmp}/gradius-verify.lock"
skipped=()
has_data=0

step() { printf '\n== %s ==\n' "$*"; }

skip() {
    echo "skipping: $1"
    skipped+=("$1")
}

# Runs only when the extracted pack exists; otherwise records the step as skipped.
data_step() {
    if ((has_data)); then
        step "$1"
        return 0
    fi
    skip "$1 (needs extracted data)"
    return 1
}

run() {
    step "file size"
    scripts/check-file-size.sh

    step "rom data guard"
    scripts/check-rom-data.sh

    step "fmt"
    cargo fmt --all -- --check

    step "clippy (default members)"
    cargo clippy -- -D warnings -A clippy::too_many_arguments

    if [[ -n "${GRADIUS_ROM:-}" ]]; then
        step "extract data"
        scripts/extract-data
    fi
    if [[ -f assets/gradius.pak ]]; then
        has_data=1
    fi

    if data_step "parity (gradius-diff)"; then
        cargo test -p gradius-diff
    fi
    if [[ "$mode" == "fast" ]]; then
        return
    fi

    if data_step "tests (default members)"; then
        cargo test
    fi

    step "portability guard"
    scripts/check-targets.sh guard

    step "wasm core and demo"
    scripts/check-targets.sh wasm
    if node_bin=$(find_node); then
        # web/fit.test.mjs runs under node too; put the chosen version first on
        # PATH so it does not pick up an older system node.
        PATH="$(dirname "$node_bin"):$PATH"
        export PATH
        web/build.sh
        node web/fit.test.mjs
        if data_step "wasm demo smoke (web/smoke.mjs)"; then
            node web/smoke.mjs
        fi
    else
        skip "web build and node tests: node >= $node_min_major not found ($(node --version 2>/dev/null || echo none))"
    fi

    step "SDL frontend"
    if command -v pkg-config >/dev/null && pkg-config --exists sdl2; then
        cargo build -p gradius-sdl
        cargo clippy -p gradius-sdl --all-targets --no-deps -- -D warnings
        if data_step "SDL tests (gradius-sdl)"; then
            cargo test -p gradius-sdl
        fi
        if data_step "SDL headless smoke"; then
            target="${CARGO_TARGET_DIR:-target}"
            SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
                "$target/debug/gradius" --pack assets/gradius.pak \
                --fit aspect --frames 60 --no-audio --scale 1
        fi
    else
        skip "SDL frontend: SDL2 development headers not found"
    fi
}

if command -v flock >/dev/null; then
    exec 9>"$lock"
    flock 9
fi
run

if ((${#skipped[@]})); then
    printf '\nci-local: passed, with %d skipped step(s):\n' "${#skipped[@]}"
    printf '  - %s\n' "${skipped[@]}"
fi
if ((!has_data)); then
    echo "ci-local: no extracted data; run GRADIUS_ROM=<your Gradius (E) ROM> scripts/ci-local.sh for the full mirror"
fi
