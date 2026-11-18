#!/usr/bin/env python3
"""Render the oracle OAM dumps of Gradius stage 1 as labelled 8x16 sprite sheets.

The ROM is only read, never written. OAM dumps are produced by the xtask probe
at frames 400 and 800; the raw CHR banks come from the iNES image itself.
The sprite palettes are read from the cartridge's text record 7 ($879D).

Usage: python3 tools/oam_sheet.py "/path/to/Gradius (E).nes"
"""

from __future__ import annotations

import argparse
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FRAMES = (400, 800)
SPRITE_TABLE = 0x1000  # PPUCTRL = $A8: sprites from $1000
CHR_BANK_BYTES = 8192
SCALE = 4

# NES 2C02 master palette (RGB approximation).
NES_PALETTE = [
    0x7C7C7C, 0x0000FC, 0x0000BC, 0x4428BC, 0x940084, 0xA80020, 0xA81000, 0x881400,
    0x503000, 0x007800, 0x006800, 0x005800, 0x004058, 0x000000, 0x000000, 0x000000,
    0xBCBCBC, 0x0078F8, 0x0058F8, 0x6844FC, 0xD800CC, 0xE40058, 0xF83800, 0xE45C10,
    0xAC7C00, 0x00B800, 0x00A800, 0x00A844, 0x008888, 0x000000, 0x000000, 0x000000,
    0xF8F8F8, 0x3CBCFC, 0x6888FC, 0x9878F8, 0xF878F8, 0xF85898, 0xF87858, 0xFCA044,
    0xF8B800, 0xB8F818, 0x58D854, 0x58F898, 0x00E8D8, 0x787878, 0x000000, 0x000000,
    0xFCFCFC, 0xA4E4FC, 0xB8B8F8, 0xD8B8F8, 0xF8B8F8, 0xF8A4C0, 0xF0D0B0, 0xFCE0A8,
    0xF8D878, 0xD8F878, 0xB8F8B8, 0xB8F8D8, 0x00FCFC, 0xF8D8F8, 0x000000, 0x000000,
]

TEXT_RECORDS = 0x8653  # the $85ED record pointer table, indexed by record id
SPR_PALETTE_RECORD = 7  # $879D: the stage's $3F10 sprite palettes

PARKED_Y = 0xEF  # stored Y $EF..$FF hide the sprite


def rgb(index: int) -> tuple[int, int, int]:
    value = NES_PALETTE[index]
    return (value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF


def sprite_palettes(rom: bytes) -> list[tuple[int, ...]]:
    """The four $3F10 sprite palettes of the stage, from text record 7."""
    prg = rom[16:16 + 32768]
    slot = TEXT_RECORDS - 0x8000 + 2 * SPR_PALETTE_RECORD
    at = (prg[slot] | (prg[slot + 1] << 8)) - 0x8000
    if prg[at:at + 2] != b"\x3f\x10":
        raise SystemExit("text record 7 does not write $3F10")
    data = prg[at + 2:at + 18]
    return [tuple(data[i:i + 4]) for i in range(0, 16, 4)]


def extract_chr_banks(rom_path: Path) -> list[bytes]:
    rom = rom_path.read_bytes()
    chr_offset = 16 + 32768  # 16-byte header + 32 KiB PRG
    banks = [rom[chr_offset + i * CHR_BANK_BYTES:chr_offset + (i + 1) * CHR_BANK_BYTES]
             for i in range(4)]
    if any(len(bank) != CHR_BANK_BYTES for bank in banks):
        raise SystemExit("ROM has no 32 KiB CHR (expected iNES CHR size 32 KiB)")
    return banks


def tile_pixels(chr_bank: bytes, tile: int) -> list[list[int]]:
    """8x8 tile as rows of 2-bit palette indices."""
    base = SPRITE_TABLE + tile * 16
    rows = []
    for y in range(8):
        low = chr_bank[base + y]
        high = chr_bank[base + y + 8]
        rows.append([((low >> (7 - x)) & 1) | (((high >> (7 - x)) & 1) << 1)
                     for x in range(8)])
    return rows


def sprite_pixels(chr_bank: bytes, tile_byte: int, attr: int) -> list[list[int]]:
    """Decode an 8x16 sprite: tile&$FE top, tile+1 bottom, honouring flips."""
    top_tile = tile_byte & 0xFE
    top = tile_pixels(chr_bank, top_tile)
    bottom = tile_pixels(chr_bank, top_tile + 1)
    rows = top + bottom
    if attr & 0x80:  # vertical flip: swap and flip the two halves
        rows = rows[::-1]
    if attr & 0x40:  # horizontal flip
        rows = [row[::-1] for row in rows]
    return rows


def draw_sprite(draw: ImageDraw.ImageDraw, x0: int, y0: int,
                pixels: list[list[int]], palette: tuple[int, ...]) -> None:
    for y, row in enumerate(pixels):
        for x, index in enumerate(row):
            colour = rgb(palette[index])
            draw.rectangle(
                [x0 + x * SCALE, y0 + y * SCALE,
                 x0 + x * SCALE + SCALE - 1, y0 + y * SCALE + SCALE - 1],
                fill=colour,
            )


def font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    path = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
    try:
        return ImageFont.truetype(path, size)
    except OSError:
        return ImageFont.load_default()


def render_sheet(frame: int, oam: bytes, chr_bank: bytes, palettes: list[tuple[int, ...]],
                 out_path: Path) -> int:
    entries = []
    for slot in range(64):
        y, tile, attr, x = oam[slot * 4:slot * 4 + 4]
        if y >= PARKED_Y:
            continue
        entries.append((slot, x, y + 1, tile, attr))

    cell_w, cell_h = 250, 104
    cols = 4
    rows = (len(entries) + cols - 1) // cols
    margin = 20
    width = cols * cell_w + margin * 2
    height = 76 + rows * cell_h + margin
    image = Image.new("RGB", (width, height), (24, 24, 32))
    draw = ImageDraw.Draw(image)
    title_font, text_font, small_font = font(22), font(13), font(11)

    draw.text((margin, 14), f"Gradius stage 1 - OAM sprite sheet, frame {frame}",
              fill=(240, 240, 240), font=title_font)
    draw.text((margin, 44),
              f"{len(entries)} non-parked entries; 8x16 sprites, CHR bank 0 ($1000); "
              "labels: slot, x, y(top), tile, attr, palette",
              fill=(170, 170, 180), font=small_font)

    for i, (slot, x, y, tile, attr) in enumerate(entries):
        col, row = i % cols, i // cols
        cx = margin + col * cell_w
        cy = 76 + row * cell_h

        draw.rectangle([cx, cy, cx + cell_w - 8, cy + cell_h - 8],
                       fill=(38, 38, 48), outline=(80, 80, 96))
        palette = palettes[attr & 3]
        sx, sy = cx + 8, cy + 14
        draw.rectangle([sx, sy, sx + 8 * SCALE, sy + 16 * SCALE], fill=rgb(0x0F))
        draw_sprite(draw, sx, sy, sprite_pixels(chr_bank, tile, attr), palette)

        tx = sx + 8 * SCALE + 10
        flags = ("H" if attr & 0x40 else "-") + ("V" if attr & 0x80 else "-")
        lines = [
            f"slot {slot}",
            f"x={x} y={y}",
            f"tile ${tile:02X} attr ${attr:02X}",
            f"pair ${tile & 0xFE:02X}/${tile | 1:02X} pal {attr & 3}",
            f"flip {flags}",
        ]
        for j, line in enumerate(lines):
            draw.text((tx, sy + j * 13), line, fill=(225, 225, 232), font=text_font)
        for j, index in enumerate(palette):
            px = tx + j * 18
            py = sy + 66
            draw.rectangle([px, py, px + 14, py + 14], fill=rgb(index),
                           outline=(110, 110, 120))

    image.save(out_path)
    return len(entries)


def capture_oam(repo: Path, rom: Path, frame: int, oam_path: Path) -> None:
    command = [
        "cargo", "xtask", "probe", str(rom), "--frame", str(frame),
        "--press", "180:start", "--press", "240:start", "--oam", str(oam_path),
    ]
    result = subprocess.run(command, cwd=repo)
    if result.returncode != 0:
        raise SystemExit(f"probe failed for frame {frame} (exit {result.returncode})")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rom", type=Path)
    parser.add_argument("--out", type=Path, default=Path.cwd(),
                        help="output directory for oam-400.png / oam-800.png")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parent.parent
    banks = extract_chr_banks(args.rom)
    palettes = sprite_palettes(args.rom.read_bytes())
    args.out.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="oam-sheet-") as tmp:
        for frame in FRAMES:
            oam_path = Path(tmp) / f"oam-{frame}.bin"
            capture_oam(repo, args.rom, frame, oam_path)
            count = render_sheet(frame, oam_path.read_bytes(), banks[0], palettes,
                                 args.out / f"oam-{frame}.png")
            print(f"frame {frame}: {count} non-parked sprites -> {args.out / f'oam-{frame}.png'}")


if __name__ == "__main__":
    main()
