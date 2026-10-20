#!/usr/bin/env python3
"""Writes the PNG fixtures of tests/fixtures/png with tools other than spritebox.

Needs FFmpeg; the `sips-*` files need macOS. The patterns are the ones that
tests/png.rs computes, so the tests know every pixel that each file holds.

    python3 scripts/make-png-fixtures.py
"""

import os
import struct
import subprocess
import sys
import tempfile
import zlib

W, H = 13, 7
ALPHAS = [255, 0, 128, 64, 200]
OUT = os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "png")


def rgba(x, y):
    return (x * 19 % 256, y * 36 % 256, x * y * 7 % 256, ALPHAS[(x + 2 * y) % 5])


def gray(x, y):
    return (x * 17 + y * 29) % 256


def wide(x, y, k):
    """A 16-bit sample of channel k."""
    return [(x * 4099 + y * 9001), (x * 1237 + y * 30011), (x * y * 613), (x * 7919 + y * 104729)][k] % 65536


PALETTE = [(0x1a, 0x1c, 0x2c), (0x5d, 0x27, 0x5d), (0xb1, 0x3e, 0x53), (0xef, 0x7d, 0x57),
           (0xff, 0xcd, 0x75), (0xa7, 0xf0, 0x70), (0x38, 0xb7, 0x64), (0x25, 0x71, 0x79),
           (0x29, 0x36, 0x6f), (0x3b, 0x5d, 0xc9), (0x41, 0xa6, 0xf6), (0x73, 0xef, 0xf7)]


def indexed(x, y):
    return PALETTE[(x * 3 + y) % 12]


def raw(fmt):
    out = bytearray()
    for y in range(H):
        if fmt == "monob":
            bits = [int((x + y) % 3 == 0) for x in range(W)]
            bits += [0] * (-len(bits) % 8)
            for i in range(0, len(bits), 8):
                out.append(int("".join(map(str, bits[i:i + 8])), 2))
            continue
        for x in range(W):
            if fmt == "rgba":
                out += bytes(rgba(x, y))
            elif fmt in ("rgb24", "pal8"):
                out += bytes(indexed(x, y) if fmt == "pal8" else rgba(x, y)[:3])
            elif fmt == "gray":
                out.append(gray(x, y))
            elif fmt == "ya8":
                out += bytes([gray(x, y), rgba(x, y)[3]])
            elif fmt == "gray16be":
                out += struct.pack(">H", wide(x, y, 0))
            elif fmt == "ya16be":
                out += struct.pack(">HH", wide(x, y, 0), wide(x, y, 3))
            elif fmt == "rgb48be":
                out += struct.pack(">HHH", *(wide(x, y, k) for k in range(3)))
            elif fmt == "rgba64be":
                out += struct.pack(">HHHH", *(wide(x, y, k) for k in range(4)))
    return bytes(out)


def ffmpeg(fmt, name, extra=()):
    with tempfile.NamedTemporaryFile(suffix=".raw") as f:
        source = "rgb24" if fmt == "pal8" else fmt
        f.write(raw(fmt))
        f.flush()
        command = ["ffmpeg", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", source,
                   "-s", f"{W}x{H}", "-i", f.name, *extra, "-frames:v", "1", "-pix_fmt", fmt,
                   os.path.join(OUT, name)]
        subprocess.run(command, check=True)


def chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


def write_png(name, depth, color_type, rows, interlace=False, extra_chunks=()):
    """rows[y][x] is a tuple of samples. Adam7 when `interlace`; filters cycle 0..4."""
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color_type]
    passes = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2),
              (0, 1, 1, 2)] if interlace else [(0, 0, 1, 1)]
    bpp = max(1, channels * depth // 8)
    data = bytearray()
    n = 0
    for x0, y0, dx, dy in passes:
        xs = range(x0, W, dx)
        prev = None
        for y in range(y0, H, dy):
            if not xs:
                break
            bits = []
            for x in xs:
                for s in rows[y][x]:
                    bits.append(s)
            line = bytearray()
            if depth >= 8:
                for s in bits:
                    line += s.to_bytes(depth // 8, "big")
            else:
                acc, count = 0, 0
                for s in bits:
                    acc = acc << depth | s
                    count += depth
                    if count == 8:
                        line.append(acc)
                        acc, count = 0, 0
                if count:
                    line.append(acc << (8 - count))
            prev = prev or bytearray(len(line))
            filt = n % 5
            n += 1
            out = bytearray()
            for i, v in enumerate(line):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = [0, a, b, (a + b) // 2, paeth(a, b, c)][filt]
                out.append((v - p) % 256)
            data += bytes([filt]) + out
            prev = line
    header = struct.pack(">IIBBBBB", W, H, depth, color_type, 0, 0, int(interlace))
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
    for kind, body in extra_chunks:
        png += chunk(kind, body)
    # Split the data over two IDAT chunks, as some writers do.
    z = zlib.compress(bytes(data), 6)
    png += chunk(b"IDAT", z[: len(z) // 2]) + chunk(b"IDAT", z[len(z) // 2:]) + chunk(b"IEND", b"")
    with open(os.path.join(OUT, name), "wb") as f:
        f.write(png)


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if pa <= pb and pa <= pc else b if pb <= pc else c


def main():
    os.makedirs(OUT, exist_ok=True)
    for fmt in ["rgba", "rgb24", "gray", "ya8", "gray16be", "ya16be", "rgb48be", "rgba64be", "monob"]:
        ffmpeg(fmt, f"ffmpeg-{fmt}.png")
    ffmpeg("rgba", "ffmpeg-rgba-pred-none.png", ["-pred", "none"])
    ffmpeg("pal8", "ffmpeg-pal8.png",
           ["-vf", "split[a][b];[a]palettegen=max_colors=16:reserve_transparent=0:stats_mode=full[p];"
                   "[b][p]paletteuse=dither=none"])
    if sys.platform == "darwin":
        source = os.path.join(OUT, "ffmpeg-rgb24.png")
        subprocess.run(["sips", "-s", "format", "png", source, "--out",
                        os.path.join(OUT, "sips-rgb24.png")], check=True, capture_output=True)
        subprocess.run(["sips", "-s", "format", "png", os.path.join(OUT, "ffmpeg-rgba.png"), "--out",
                        os.path.join(OUT, "sips-rgba.png")], check=True, capture_output=True)
    rows = [[rgba(x, y) for x in range(W)] for y in range(H)]
    write_png("zlib-rgba-adam7.png", 8, 6, rows, interlace=True)
    two_bit = [[((x + 2 * y) % 4,) for x in range(W)] for y in range(H)]
    write_png("zlib-gray2-trns-adam7.png", 2, 0, two_bit, interlace=True,
              extra_chunks=[(b"tRNS", struct.pack(">H", 1))])
    rgb_rows = [[rgba(x, y)[:3] for x in range(W)] for y in range(H)]
    key = rgba(4, 3)[:3]
    write_png("zlib-rgb-trns.png", 8, 2, rgb_rows,
              extra_chunks=[(b"gAMA", struct.pack(">I", 45455)), (b"tRNS", struct.pack(">HHH", *key))])
    four_bit = [[((x * 3 + y) % 12,) for x in range(W)] for y in range(H)]
    plte = b"".join(bytes(c) for c in PALETTE)
    write_png("zlib-indexed4-trns.png", 4, 3, four_bit,
              extra_chunks=[(b"PLTE", plte), (b"tRNS", bytes([0, 128, 255, 7]))])


if __name__ == "__main__":
    main()
