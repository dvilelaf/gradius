#!/bin/bash
# Runs inside the build container (scripts/package-appimage.sh), checkout at /src: release `gradius`, the AppDir
# with SDL2 linked statically, and the AppImage in /src/dist. No ROM data goes in: the game builds its pack from the player's ROM.
set -euo pipefail
cd /src
VERSION=$1
export CARGO_TARGET_DIR=/src/target/appimage
APPDIR=/tmp/AppDir
OUT=dist/Gradius-$VERSION-x86_64.AppImage

cargo build --release --locked -p gradius-sdl --features "sdl2/bundled sdl2/static-link"

rm -rf "$APPDIR" && mkdir -p "$APPDIR/usr/bin" dist
cp "$CARGO_TARGET_DIR/release/gradius" "$APPDIR/usr/bin/"
strip "$APPDIR/usr/bin/gradius"
cp packaging/linux/AppRun "$APPDIR/" && chmod +x "$APPDIR/AppRun"
cp packaging/linux/gradius.desktop "$APPDIR/"
cp packaging/linux/icon.svg "$APPDIR/gradius.svg" && ln -sf gradius.svg "$APPDIR/.DirIcon"
doc=$APPDIR/usr/share/doc/gradius
mkdir -p "$doc"
cp LICENSE-MIT LICENSE-APACHE target/licenses/linux/THIRD-PARTY-LICENSES.txt "$doc/"

# SDL2 is static in the binary and loads X11/Wayland/audio from the host (dlopen): nothing else goes in the AppImage
needed() { readelf -d "$1" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'; }
bad=$(needed "$APPDIR/usr/bin/gradius" | grep -Ev '^(libc|libm|libdl|libpthread|librt|libgcc_s)\.so|^ld-linux' || true)
[ -z "$bad" ] || { echo "package: gradius links non-glibc libraries: $bad"; exit 1; }

# appimagetool runs extracted (no FUSE in the container); version and checksums pinned
fetch() {  # url sha256 file
    curl -fsSL -o "/tmp/$3" "$1"
    echo "$2  /tmp/$3" | sha256sum -c --quiet
}
fetch https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
      ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 appimagetool-x86_64.AppImage
fetch https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
      2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d runtime-x86_64
chmod +x /tmp/appimagetool-x86_64.AppImage
rm -f "$OUT"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 /tmp/appimagetool-x86_64.AppImage --no-appstream \
    --runtime-file /tmp/runtime-x86_64 "$APPDIR" "$OUT"
ls -la "$OUT"
