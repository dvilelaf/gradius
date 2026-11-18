#!/bin/bash
# scripts/package-appimage.sh: dist/Gradius-<version>-x86_64.AppImage, built in an Ubuntu 22.04 container (podman;
# packaging/linux/Containerfile) so it runs on glibc 2.35+. SDL2 is built from source into the binary; no ROM data is. Cargo downloads are
# cached in the podman volume gradius-cargo, build output in target/appimage.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version = "\(.*\)"/\1/p' Cargo.toml)
[ -n "$VERSION" ] || { echo "no version in Cargo.toml"; exit 1; }
[ -f LICENSE-MIT ] && [ -f LICENSE-APACHE ] || { echo "package: LICENSE-MIT and LICENSE-APACHE are missing from the repo root"; exit 1; }
scripts/third-party-licenses.py --target x86_64-unknown-linux-gnu --features "sdl2/bundled sdl2/static-link" \
    --out target/licenses/linux/THIRD-PARTY-LICENSES.txt
podman build -q -t localhost/gradius-build:x86_64 -f packaging/linux/Containerfile packaging/linux >/dev/null
podman run --rm -v "$PWD:/src" -v gradius-cargo:/opt/cargo/registry -w /src \
    localhost/gradius-build:x86_64 packaging/linux/build.sh "$VERSION"
echo "dist/Gradius-$VERSION-x86_64.AppImage"
