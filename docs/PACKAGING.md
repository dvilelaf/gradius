# Packaging

Two downloads are built from this tree, and neither one contains game data:

| artifact | built by | what is in it | what is not |
| --- | --- | --- | --- |
| `dist/Gradius-<version>-x86_64.AppImage` (Linux) | `scripts/package-appimage.sh` | the `gradius` binary with SDL2 linked in, `AppRun`, a `.desktop` file and a simple icon | no `.pak`, no `.nes`, no fixtures |
| `dist/Gradius-<version>-windows-x86_64.zip` (Windows) | `scripts/package-windows.sh` | `gradius.exe`, `SDL2.dll` and a short `README.txt`, in one folder | no `.pak`, no `.nes`, no fixtures, no icons |

The game builds its own data on the player's machine: the first run asks for
the player's ROM and writes `gradius.pak` to the data folder (see the README,
"How to play"). So the packages never need the ROM, and the release workflow
never sees one.

The version comes from `[workspace.package]` in `Cargo.toml`. Both scripts
write to `dist/`, which is git-ignored.

## Requirements

Linux host, for the AppImage:

- `podman`. Rust is not needed on the host: the build runs in an Ubuntu 22.04
  container with Rust 1.90.0 installed by rustup (`packaging/linux/Containerfile`).
- Network access on the first build, for the base image, the Rust toolchain and
  the pinned `appimagetool`.

Linux host, for the Windows zip:

- Rust with the Windows target: `rustup target add x86_64-pc-windows-gnu`.
- The mingw-w64 toolchain: `sudo apt install mingw-w64`.
- `zip`, `curl` and `sha256sum`.
- Network access on the first build, for the pinned SDL2 development package
  (`SDL2-devel-2.32.10-mingw.tar.gz`, checked against its SHA-256 in the script).

For the smoke tests:

- `packaging/linux/smoketest.sh` needs `podman`, and pulls `ubuntu:22.04`.
- `packaging/windows/smoketest.sh` needs `wine` and `unzip`.
- The `--first-run` cases need your own Gradius (E) ROM. Set `GRADIUS_ROM` to
  its path for the Linux one; pass it as the last argument for the Windows one.

## Building

```sh
scripts/package-appimage.sh    # -> dist/Gradius-<version>-x86_64.AppImage
scripts/package-windows.sh     # -> dist/Gradius-<version>-windows-x86_64.zip
```

Both build from the working tree, so commit or stash first when you want the
artifact to match a commit. Caches are kept between runs: Cargo downloads in the
podman volume `gradius-cargo`, the Linux build output in `target/appimage`, and
the SDL2 archive and extracted files in `target/windows-pkg`.

The AppImage is built with SDL2 compiled in and linked statically. The script
fails if the binary needs any library other than glibc and the loader, so the
result runs on distributions with glibc 2.35 or newer. At run time SDL loads the
host's X11, Wayland and audio libraries itself; those come from the host, not
from the AppImage.

The Windows zip is cross-built with `x86_64-pc-windows-gnu`. The C runtime is
linked statically (`crt-static`), so `SDL2.dll` is the only DLL beside
`gradius.exe`. The release build has no console window.

## Smoke tests

Run them on the artifact you are about to publish, not on a build from a
different commit.

Linux, in a clean `ubuntu:22.04` container (no Rust, no SDL installed), with
dummy video and audio and no FUSE (`--appimage-extract-and-run`):

```sh
packaging/linux/smoketest.sh                 # --help, then 120 frames from assets/gradius.pak
GRADIUS_PACK=/path/gradius.pak packaging/linux/smoketest.sh
GRADIUS_ROM="/path/Gradius (E).nes" packaging/linux/smoketest.sh --first-run
```

The default case mounts the pack read-only. `--first-run` starts with an empty
temporary `GRADIUS_DATA` and `--rom`, and checks that `gradius.pak` was written.

Windows, under `wine` with a fresh prefix in `/tmp`:

```sh
packaging/windows/smoketest.sh dist/Gradius-<version>-windows-x86_64.zip assets/gradius.pak
packaging/windows/smoketest.sh dist/Gradius-<version>-windows-x86_64.zip assets/gradius.pak \
    --first-run "/path/Gradius (E).nes"
```

The script first checks that the zip holds no `.pak`, `.nes`, `.grs`, `.state`,
`.trace` or `icons/` entry, then runs `--help`, `--pack ... --frames 120` and,
with `--first-run`, `--rom` next to the `.exe`.

The `--first-run` checks need the first-run screen, which is in the tree. They
run locally with your ROM; the release workflow does not run them.

## Release workflow

`.github/workflows/release.yml` builds both artifacts on every run. A tag
`v*` also publishes them as assets of a GitHub Release. Each job fails if its
artifact holds a `.pak` or `.nes` file. The workflow needs no secrets and no ROM.
`workflow_dispatch` builds the same artifacts without publishing. The steps
for cutting a release are in [`RELEASE.md`](RELEASE.md).
