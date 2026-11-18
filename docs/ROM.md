# ROM policy and extractor contract

## Legal note

Gradius is copyright Konami. A NES cartridge dump is copyrighted material and
must not be distributed with this project. As a repository rule, no ROM image
and no binary asset derived from one may be committed: no built pack, no
extracted CHR sheet, no rendered sprite or OAM sheet, no screenshot and no
audio capture.

To run the port you must supply your own legally obtained dump of
`Gradius (E).nes`. Do not commit it, do not attach it to issues, and do not
publish the pack built from it.

The following are ignored by `.gitignore`:

```
*.nes
*.bin
*.pak
web/gradius_wasm.wasm
web/gradius.pak
oam-*.png
sprite-*.png
sprite_*.png
sprite_groups.png
anim-*.png
/screenshots
.openchamber/screenshots
/out
/analysis/out
```

`assets/gradius.pak` is derived data. It is convenient to keep locally so the
runtime is independent of the ROM, but it is not part of the repository and
must not be redistributed. No pack is checked in; the tests build the packs
they need from synthetic data, without cartridge data.

### No committed ROM or derived asset

Nothing decoded as a usable asset may be committed. That covers the cartridge
image, a pack built from it, web build copies of the pack or wasm module,
generated CHR/tile sheets, OAM or animation sheets, screenshots, audio
captures, and generated C from external recompilers.

The source does contain decoded tables and measured constants (tile indices,
nametable/attribute arrays, sprite tables, palettes and captured register
timelines) that were produced from a cartridge. Those remain copyright Konami,
are excluded from the project's MIT/Apache-2.0 grant, and exist only for
research and interoperability; they are not a redistributable asset set. See
`NOTICE`.

### Removed artifacts in history

`c25c171` ("tools: render labelled OAM sprite sheets for stage 1 frames 400 and
800") added `oam-400.png` and `oam-800.png` at the repository root. `aae20de`
("chore: keep ROM-derived analysis sheets out of the repo") deleted them and
added ignore rules. The blobs still exist in history
(`a8fe627ce98071ab3d5763541b72f9741d2273e0` and
`6adb148544a2f3471440de6aaeb7245fc05d7078`) but no current tree contains
them. History is not rewritten; do not reintroduce the files or their contents.

## Why an asset pack

Emulators read the cartridge at runtime. This port does not. The art is decoded
once into a small container, and the game only sees plain tiles and RGB
colours. That keeps the runtime free of mapper, CHR-bank and iNES handling, and
makes the core easy to embed on platforms without a filesystem.

The pack is not the only persisted output of analysis, but it is the only one
the runtime reads. Screen maps and other decoded tables live in source where
the core can use them without a filesystem; they are never copied into the
pack as cartridge bytes.

## Extractor contract

Command:

```sh
cargo xtask extract <rom> <out.pak>
```

Input: an iNES file. `gradius-cart` validates the 16-byte header, PRG/CHR sizes
and mapper. This port targets mapper 3 (CNROM) with four 8 KiB CHR banks. The
extractor reads the CHR data and attaches the standard 64-entry NES master
palette; it does not need PRG code or the PPU.

Output: a little-endian `GRDPAK01` file:

```text
bytes 0..8    magic "GRDPAK01"
bytes 8..10   format version (u16, currently 1)
bytes 10..12  chunk count (u16)
per chunk:
  u32         chunk id
  u32         payload length
  payload     length bytes
```

Chunks currently written:

| id | name | payload |
| --- | --- | --- |
| 1 | `CHUNK_TILES` | `u16 banks`, `u16 tiles_per_bank` (must be 8 and 256), then `banks * 256` tiles of 64 bytes. Each tile is 8x8, 2 bits per pixel, one byte per pixel with values 0..3, row-major |
| 2 | `CHUNK_PALETTES` | `u8 count`, then `count` RGB triples; the extractor writes the 64-entry NES master palette |
| 3 | `CHUNK_META` | UTF-8 provenance text: project, format, mapper, PRG/CHR sizes, source SHA-256 and the audio summary below |
| 4 | `CHUNK_AUDIO` | Versioned `GAUD` payload: captured register timelines for every music track and sound effect, with ids, tempo, event counts and a payload SHA-256. Decoded by `gradius-audio`; layout in `docs/AUDIO.md` |
| 5 | `CHUNK_STAGE` | A `u8` entry count, then per entry: `u8 name_len`, name, `u8 kind`, `u16` ROM address, `u16` length, bytes. Names include `stage6_wall_erase`, `ending_text_record` (`$CF27`: 8 pointers and the 19-byte ending text record), `ending_screen_stream`, `ending_screen_classes`, `ending_ship_motion` and the ending tile-class tables. Readers skip unknown chunks, so later entries stay backward compatible within version 1 |

Screens are reconstructed separately by `cargo xtask screens <rom> --out
<file>` and written to `crates/gradius-core/src/stage1_bg.rs` (the title screen
and the wide stage-1 map as decoded nametable and attribute arrays). They are
source data, not a pack chunk; regenerate rather than edit them by hand. No
screenshot is committed.

Guarantees:

- The pack is deterministic: the same ROM and the same pinned audio capture
  produce byte-identical output.
- The pack contains no PRG code and no entire-ROM copy. The metadata is a
  short text record and the SHA-256 of the input, not cartridge bytes.
- `gradius-assets` validates magic, version, chunk lengths, tile counts and
  palette bounds; a truncated or malformed pack is rejected with a specific
  error.
- The runtime never opens the `.nes` file.

## Verifying a pack

```sh
cargo xtask inspect "Gradius (E).nes"
cargo xtask extract "Gradius (E).nes" assets/gradius.pak
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak
```

The expected source SHA-256 for the reference dump is recorded in
`docs/analysis/stage1.md`. If it differs, the extractor still works, but the
measured constants may not match what you see.
