#!/usr/bin/env python3
"""Draws the app icon, a caption panel, as PNG and ICO. Needs nothing beyond Python itself."""

import struct
import zlib
from pathlib import Path

HERE = Path(__file__).parent
SCALE = 4  # Drawn larger, then averaged down for smooth edges.


def inside_round_rect(x, y, left, top, right, bottom, radius):
    nearest_x = min(max(x, left + radius), right - radius)
    nearest_y = min(max(y, top + radius), bottom - radius)
    return (x - nearest_x) ** 2 + (y - nearest_y) ** 2 <= radius**2


# Shapes in a 0 to 1 square, drawn in order: left, top, right, bottom, radius, colour.
PANEL = (0.04, 0.04, 0.96, 0.96, 0.21, (21, 23, 28))
WHITE = (245, 246, 248)
SHAPES = [
    PANEL,
    (0.20, 0.33, 0.80, 0.43, 0.05, WHITE),
    (0.20, 0.57, 0.52, 0.67, 0.05, WHITE),
    (0.58, 0.57, 0.80, 0.67, 0.05, (255, 209, 77)),
]


def draw(size):
    big = size * SCALE
    rows = []
    for row in range(size):
        pixels = bytearray()
        for column in range(size):
            red = green = blue = alpha = 0
            for sub_y in range(SCALE):
                for sub_x in range(SCALE):
                    x = (column * SCALE + sub_x + 0.5) / big
                    y = (row * SCALE + sub_y + 0.5) / big
                    colour = None
                    for left, top, right, bottom, radius, shade in SHAPES:
                        if inside_round_rect(x, y, left, top, right, bottom, radius):
                            colour = shade
                    if colour:
                        red += colour[0]
                        green += colour[1]
                        blue += colour[2]
                        alpha += 1
            if alpha:
                pixels += bytes(
                    (red // alpha, green // alpha, blue // alpha, 255 * alpha // SCALE**2)
                )
            else:
                pixels += bytes(4)
        rows.append(b"\0" + bytes(pixels))
    return b"".join(rows)


def chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


def png(size):
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(draw(size), 9))
        + chunk(b"IEND", b"")
    )


def ico(sizes):
    images = [png(size) for size in sizes]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, image in zip(sizes, images):
        side = 0 if size == 256 else size
        out += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(image), offset)
        offset += len(image)
    return out + b"".join(images)


(HERE / "icon.png").write_bytes(png(256))
(HERE / "windows" / "soundveil.ico").write_bytes(ico([256, 48, 32, 16]))
