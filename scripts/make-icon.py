#!/usr/bin/env python3
"""Generate a 1024x1024 app-icon.png (orange gradient rounded square + spark).
Pure standard library — no Pillow required."""
import struct
import zlib
import math

S = 1024
# brand palette
ACCENT = (255, 106, 43)
ACCENT2 = (255, 176, 46)
BG = (11, 13, 16)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def rounded(x, y, w, h, r):
    # signed-distance style membership for a rounded square
    dx = max(abs(x - w / 2) - (w / 2 - r), 0)
    dy = max(abs(y - h / 2) - (h / 2 - r), 0)
    return math.hypot(dx, dy) <= r


rows = bytearray()
cx, cy = S / 2, S / 2
margin = 96
radius = 220
for y in range(S):
    rows.append(0)  # filter type 0
    for x in range(S):
        inside = rounded(x, y, S, S, 0) and (margin <= x <= S - margin) and (margin <= y <= S - margin)
        # rounded tile
        tile = rounded(x - margin, y - margin, S - 2 * margin, S - 2 * margin, radius)
        if tile:
            t = (x + y) / (2 * S)
            r, g, b = lerp(ACCENT, ACCENT2, t)
            # spark / play triangle in the center (dark cut-out)
            tx, ty = x - cx, y - cy
            tri = (tx > -150 and tx < 200 and abs(ty) < (200 - (tx + 150) * 0.55))
            if tri:
                r, g, b = BG
            rows += bytes((r, g, b))
        else:
            rows += bytes(BG)


def chunk(typ, data):
    c = struct.pack(">I", len(data)) + typ + data
    return c + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)


sig = b"\x89PNG\r\n\x1a\n"
ihdr = struct.pack(">IIBBBBB", S, S, 8, 2, 0, 0, 0)  # 8-bit RGB
idat = zlib.compress(bytes(rows), 9)
png = sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")

with open("src-tauri/app-icon.png", "wb") as f:
    f.write(png)
print("wrote src-tauri/app-icon.png", len(png), "bytes")
