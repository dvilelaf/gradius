#!/usr/bin/env bash
# Build the portable Windows zip: dist/Gradius-<version>-windows-x86_64.zip
# Cross-compiles with mingw from Linux. The C runtime is linked statically
# (crt-static), so the only DLL shipped besides gradius.exe is SDL2.dll.
set -euo pipefail

SDL_VERSION=2.32.10
SDL_SHA256=83a5d74012311edc3c0d40ea6faecbe57ad692aa033fa5dc273cc937e3938ff2
SDL_URL="https://github.com/libsdl-org/SDL/releases/download/release-$SDL_VERSION/SDL2-devel-$SDL_VERSION-mingw.tar.gz"
TARGET=x86_64-pc-windows-gnu

cd "$(dirname "$0")/.."
root=$PWD
[ -f LICENSE-MIT ] && [ -f LICENSE-APACHE ] || { echo "package: LICENSE-MIT and LICENSE-APACHE are missing from the repo root"; exit 1; }
cache=$root/target/windows-pkg
mkdir -p "$cache"

tarball=$cache/SDL2-devel-$SDL_VERSION-mingw.tar.gz
if [ ! -f "$tarball" ] || ! echo "$SDL_SHA256  $tarball" | sha256sum -c --quiet 2>/dev/null; then
    curl -fsSL -o "$tarball" "$SDL_URL"
    echo "$SDL_SHA256  $tarball" | sha256sum -c --quiet
fi
sdl=$cache/SDL2-$SDL_VERSION/x86_64-w64-mingw32
[ -d "$sdl" ] || tar xzf "$tarball" -C "$cache"
scripts/third-party-licenses.py --target "$TARGET" --sdl-license "$cache/SDL2-$SDL_VERSION/LICENSE.txt" \
    --sdl-version "$SDL_VERSION" --out "$root/target/licenses/windows/THIRD-PARTY-LICENSES.txt"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
start=$(date +%s)

RUSTFLAGS="-C target-feature=+crt-static -L native=$sdl/lib ${RUSTFLAGS:-}" \
    cargo build --release --locked -p gradius-sdl --target "$TARGET" \
    --config packaging/windows/config.toml

name=Gradius-$version-windows-x86_64
stage=$root/dist/$name
rm -rf "$stage"
mkdir -p "$stage"
cp "target/$TARGET/release/gradius.exe" "$stage/gradius.exe"
cp "$sdl/bin/SDL2.dll" "$stage/SDL2.dll"
cp packaging/windows/README.txt "$stage/README.txt"
cp LICENSE-MIT LICENSE-APACHE target/licenses/windows/THIRD-PARTY-LICENSES.txt "$stage/"

rm -f "dist/$name.zip"
(cd dist && zip -qr "$name.zip" "$name")
rm -rf "$stage"
echo "built dist/$name.zip in $(( $(date +%s) - start ))s ($(du -h "dist/$name.zip" | cut -f1))"
