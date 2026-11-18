#!/usr/bin/env bash
# Build the game wasm and the extractor wasm and copy them next to the page.
set -euo pipefail
cd "$(dirname "$0")/.."

target=wasm32-unknown-unknown
if ! rustup target list --installed | grep -qx "$target"; then
    echo "installing rustup target $target"
    rustup target add "$target"
fi

cargo build --release -p gradius-wasm --target "$target"

target_dir="${CARGO_TARGET_DIR:-target}"
cp "$target_dir/$target/release/gradius_wasm.wasm" web/gradius_wasm.wasm
echo "web/gradius_wasm.wasm: $(wc -c < web/gradius_wasm.wasm) bytes"

cargo build --release -p gradius-extract-wasm --target "$target"
cp "$target_dir/$target/release/gradius_extract_wasm.wasm" web/gradius_extract_wasm.wasm
echo "web/gradius_extract_wasm.wasm: $(wc -c < web/gradius_extract_wasm.wasm) bytes"

# The page asks for the ROM and extracts in the browser, so no pack is needed.
# A web/gradius.pak, if present, is used as a local shortcut (never precached).

for asset in manifest.webmanifest service-worker.js; do
    if [[ ! -f "web/$asset" ]]; then
        echo "missing web/$asset" >&2
        exit 1
    fi
done
