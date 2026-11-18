#!/usr/bin/env bash
# Proves the core stays platform-agnostic.
#
#   scripts/check-targets.sh [all|wasm|guard]
#
#   all    (default) no-OS guard, wasm32 build of gradius-core, host build
#   wasm   only the wasm32-unknown-unknown build of gradius-core
#   guard  only the grep guard for OS APIs in gradius-core
#
# CI runs the two modes separately so each failure has its own job.
set -euo pipefail

target=wasm32-unknown-unknown
core=crates/gradius-core
# Modules a platform-agnostic core must never touch. Textual guard: it also
# catches `use std::fs;` before the compiler would.
os_api='std::(fs|net|process|thread|time)\b'

guard() {
    local hits
    if hits="$(grep -rnE "$os_api" "$core" --include='*.rs')"; then
        echo "check-targets: $core uses an OS API; it must stay platform-agnostic:" >&2
        echo "$hits" >&2
        exit 1
    fi
    echo "check-targets: guard ok: no std::fs/net/process/thread/time in $core"
}

wasm() {
    if ! command -v rustup >/dev/null; then
        echo "check-targets: rustup not found; cannot ensure $target is installed" >&2
        exit 1
    fi
    if ! rustup target list --installed | grep -qx "$target"; then
        echo "check-targets: installing rustup target $target"
        rustup target add "$target"
    fi
    cargo check -p gradius-core --target "$target"
    echo "check-targets: wasm ok: gradius-core builds for $target"
}

host() {
    cargo check -p gradius-core -p gradius-assets -p gradius-pack
    echo "check-targets: host ok: gradius-core, gradius-assets, gradius-pack"
}

case "${1:-all}" in
    all)
        guard
        wasm
        host
        ;;
    wasm) wasm ;;
    guard) guard ;;
    *)
        echo "usage: scripts/check-targets.sh [all|wasm|guard]" >&2
        exit 2
        ;;
esac
