#!/usr/bin/env python3
"""Compare two NESRecomp frames exactly.

Inputs are PNG screenshots, or raw RGBA dumps (row-major, 4 bytes per pixel)
with --size. Exit status: 0 identical, 1 pixels differ, 2 unusable input.

Usage:
  tools/nesrecomp/compare.py native.png interp.png
  tools/nesrecomp/compare.py native.rgba interp.rgba --size 256x240
  tools/nesrecomp/compare.py native.png interp.png --regions 16x15
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def parse_size(value: str) -> tuple[int, int]:
    try:
        width, height = (int(part) for part in value.lower().split("x", 1))
    except ValueError:
        raise argparse.ArgumentTypeError("expected WxH, such as 256x240")
    if width <= 0 or height <= 0:
        raise argparse.ArgumentTypeError("width and height must be positive")
    return width, height


def load(path: Path, size: tuple[int, int] | None) -> tuple[int, int, bytes]:
    if not path.is_file():
        raise SystemExit(f"{path}: not found")
    if path.suffix.lower() == ".png":
        try:
            from PIL import Image
        except ImportError:
            raise SystemExit("PNG input needs Pillow: python3 -m pip install pillow")
        with Image.open(path) as image:
            width, height = image.size
            return width, height, image.convert("RGBA").tobytes()

    if size is None:
        raise SystemExit(f"{path}: raw input needs --size WxH")
    width, height = size
    data = path.read_bytes()
    expected = width * height * 4
    if len(data) != expected:
        raise SystemExit(f"{path}: {len(data)} bytes, expected {expected} for {width}x{height} RGBA")
    return width, height, data


def rgba(data: bytes) -> str:
    return "#" + data.hex().upper()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("a", type=Path, help="reference PNG or raw RGBA")
    parser.add_argument("b", type=Path, help="PNG or raw RGBA to compare")
    parser.add_argument(
        "--size", type=parse_size,
        help="dimensions of raw RGBA inputs (PNG sizes are read from the file)",
    )
    parser.add_argument(
        "--regions", type=parse_size, default=(8, 8), metavar="COLSxROWS",
        help="grid used for the per-region report (default 8x8)",
    )
    args = parser.parse_args()

    aw, ah, a = load(args.a, args.size)
    bw, bh, b = load(args.b, args.size)
    if (aw, ah) != (bw, bh):
        parser.error(f"dimensions differ: {aw}x{ah} and {bw}x{bh}")

    total = aw * ah
    differing = [i for i in range(total) if a[i * 4:i * 4 + 4] != b[i * 4:i * 4 + 4]]
    if not differing:
        print(f"identical: {aw}x{ah}, {total} pixels")
        return 0

    matched = total - len(differing)
    print(f"exact: {100.0 * matched / total:.4f}% ({matched}/{total} pixels match, "
          f"{len(differing)} differ)")

    cols, rows = args.regions
    counts = [[0] * cols for _ in range(rows)]
    for pixel in differing:
        x, y = pixel % aw, pixel // aw
        counts[y * rows // ah][x * cols // aw] += 1
    bad = [(col, row, n) for row, cells in enumerate(counts)
           for col, n in enumerate(cells) if n]
    print(f"per-region {cols}x{rows}: {len(bad)}/{cols * rows} regions differ")
    for col, row, n in bad:
        x0, x1 = aw * col // cols, aw * (col + 1) // cols
        y0, y1 = ah * row // rows, ah * (row + 1) // rows
        region = (x1 - x0) * (y1 - y0)
        print(f"  region ({col},{row}) x={x0}..{x1 - 1} y={y0}..{y1 - 1}: "
              f"{100.0 * (region - n) / region:.2f}% match ({n} differ)")

    pixel = differing[0]
    x, y = pixel % aw, pixel // aw
    at = pixel * 4
    print(f"first difference: pixel {pixel} (x={x}, y={y}) "
          f"{rgba(a[at:at + 4])} -> {rgba(b[at:at + 4])}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
