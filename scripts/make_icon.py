#!/usr/bin/env python3
"""Personal Ops app icon generator (dependency-free, pure-stdlib PNG writer).

Produces the 1024x1024 source icon: a dark rounded square with a concentric
ring and focus dot. The output feeds `pnpm tauri icon`:

    python3 scripts/make_icon.py resources/icon-source.png
    cd apps/desktop && pnpm tauri icon ../../resources/icon-source.png
"""

import struct
import sys
import zlib

SIZE = 1024
BG = (30, 30, 36)          # koyu zemin
RING = (123, 138, 245)     # accent (indigo)
DOT = (236, 236, 241)      # açık nokta
CORNER_R = 232             # macOS squircle hissi için köşe yarıçapı


def smoothstep(edge0: float, edge1: float, x: float) -> float:
    t = max(0.0, min(1.0, (x - edge0) / (edge1 - edge0)))
    return t * t * (3 - 2 * t)


def rounded_rect_alpha(x: float, y: float) -> float:
    """Yuvarlatılmış kare maskesi (kenarda 2px yumuşatma)."""
    half = SIZE / 2
    dx = abs(x - half) - (half - CORNER_R)
    dy = abs(y - half) - (half - CORNER_R)
    dx = max(dx, 0.0)
    dy = max(dy, 0.0)
    dist = (dx * dx + dy * dy) ** 0.5
    return 1.0 - smoothstep(CORNER_R - 2.0, CORNER_R, dist)


def blend(base, over, alpha):
    return tuple(int(b * (1 - alpha) + o * alpha) for b, o in zip(base, over))


def pixel(x: int, y: int):
    mask = rounded_rect_alpha(x + 0.5, y + 0.5)
    if mask <= 0.0:
        return (0, 0, 0, 0)

    cx = cy = SIZE / 2
    d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
    color = BG

    # dış halka (yörünge)
    ring_mid, ring_w = 330.0, 34.0
    ring_a = 1.0 - smoothstep(ring_w / 2 - 2, ring_w / 2 + 2, abs(d - ring_mid))
    if ring_a > 0:
        color = blend(color, RING, ring_a * 0.95)

    # merkez odak noktası
    dot_a = 1.0 - smoothstep(96.0, 104.0, d)
    if dot_a > 0:
        color = blend(color, DOT, dot_a)

    # yörünge üzerindeki uydu nokta (saat ~10:30 yönü)
    sx, sy = cx + ring_mid * -0.7071, cy + ring_mid * -0.7071
    sd = ((x - sx) ** 2 + (y - sy) ** 2) ** 0.5
    sat_a = 1.0 - smoothstep(58.0, 66.0, sd)
    if sat_a > 0:
        color = blend(color, DOT, sat_a)

    return (*color, int(255 * mask))


def write_png(path: str) -> None:
    raw = bytearray()
    for y in range(SIZE):
        raw.append(0)  # filter: none
        for x in range(SIZE):
            raw.extend(pixel(x, y))

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "icon-source.png"
    write_png(out)
    print(f"written: {out}")
