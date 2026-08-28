#!/usr/bin/env python3
"""Иконка Sol Flow для Mac — та же волна из пяти полосок, что на Android
(ic_launcher_foreground.xml): тёмная плашка со скруглением, зелёные
полоски по центру. Pillow на этой машине нет, поэтому PNG собирается
вручную: сглаживание — рендер в четыре раза крупнее с усреднением.

Запуск: python3 make-icon.py, дальше iconutil из скрипта сборки.
"""

import struct
import zlib
from pathlib import Path

CANVAS = (0x1C, 0x1D, 0x20)
ACCENT = (0x7B, 0xC7, 0x2E)

# Геометрия с Android: полоски шириной 10 с шагом 13 в поле 108×108.
BARS = [(23, 49, 59), (36, 41, 67), (49, 33, 75), (62, 41, 67), (75, 49, 59)]
BAR_W = 10
FIELD = 108.0

SS = 4  # кратность суперсэмплинга


def rounded_rect(x, y, w, h, r, px, py):
    """Точка внутри прямоугольника со скруглением радиуса r."""
    if px < x or px > x + w or py < y or py > y + h:
        return False
    cx = min(max(px, x + r), x + w - r)
    cy = min(max(py, y + r), y + h - r)
    return (px - cx) ** 2 + (py - cy) ** 2 <= r * r


def render(size, margin_ratio=0.0, corner_ratio=0.0):
    """Рисует иконку размером size. margin_ratio — поле вокруг плашки,
    corner_ratio — радиус её скругления в долях стороны."""
    big = size * SS
    scale = big / FIELD
    margin = big * margin_ratio
    plate = big - 2 * margin
    corner = plate * corner_ratio

    rows = []
    for y in range(big):
        row = bytearray()
        for x in range(big):
            inside_plate = rounded_rect(margin, margin, plate, plate, corner, x + 0.5, y + 0.5)
            if not inside_plate:
                row += bytes((0, 0, 0, 0))
                continue

            # Координаты внутри поля 108×108 с учётом полей плашки.
            fx = (x + 0.5 - margin) / (plate / FIELD)
            fy = (y + 0.5 - margin) / (plate / FIELD)
            color = CANVAS
            for bx, top, bottom in BARS:
                if rounded_rect(bx, top, BAR_W, bottom - top, BAR_W / 2, fx, fy):
                    color = ACCENT
                    break
            row += bytes(color + (255,))
        rows.append(bytes(row))

    # Усреднение блоков SS×SS — так края полосок и плашки выходят гладкими.
    out = []
    for y in range(size):
        line = bytearray()
        for x in range(size):
            r = g = b = a = 0
            for dy in range(SS):
                src = rows[y * SS + dy]
                for dx in range(SS):
                    i = ((x * SS + dx) * 4)
                    r += src[i]
                    g += src[i + 1]
                    b += src[i + 2]
                    a += src[i + 3]
            n = SS * SS
            line += bytes((r // n, g // n, b // n, a // n))
        out.append(bytes(line))
    return out


def write_png(path, rows, size):
    raw = b"".join(b"\x00" + row for row in rows)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    Path(path).write_bytes(png)


def main():
    icons = Path(__file__).parent / "src-tauri/icons"
    iconset = icons / "icon.iconset"
    iconset.mkdir(parents=True, exist_ok=True)

    # Иконка приложения: плашка со скруглением как у macOS, с полями.
    sizes = [16, 32, 64, 128, 256, 512, 1024]
    rendered = {}
    for size in sizes:
        rendered[size] = render(size, margin_ratio=0.06, corner_ratio=0.225)
        print(f"нарисовал {size}")

    write_png(icons / "icon.png", rendered[512], 512)
    write_png(icons / "icon_1024.png", rendered[1024], 1024)

    # Раскладка iconset: обычные и @2x.
    for name, size in [
        ("icon_16x16", 16),
        ("icon_16x16@2x", 32),
        ("icon_32x32", 32),
        ("icon_32x32@2x", 64),
        ("icon_128x128", 128),
        ("icon_128x128@2x", 256),
        ("icon_256x256", 256),
        ("icon_256x256@2x", 512),
        ("icon_512x512", 512),
        ("icon_512x512@2x", 1024),
    ]:
        write_png(iconset / f"{name}.png", rendered[size], size)

    # Значок в меню-баре — только волна, без плашки: система красит его
    # сама (icon_as_template), поэтому полоски должны быть непрозрачными
    # на прозрачном фоне.
    tray = []
    big = 44 * SS
    scale = big / FIELD
    for y in range(big):
        row = bytearray()
        for x in range(big):
            fx = (x + 0.5) / scale
            fy = (y + 0.5) / scale
            hit = any(
                rounded_rect(bx, top, BAR_W, bottom - top, BAR_W / 2, fx, fy)
                for bx, top, bottom in BARS
            )
            row += bytes((0, 0, 0, 255) if hit else (0, 0, 0, 0))
        tray.append(bytes(row))

    out = []
    for y in range(44):
        line = bytearray()
        for x in range(44):
            r = g = b = a = 0
            for dy in range(SS):
                src = tray[y * SS + dy]
                for dx in range(SS):
                    i = (x * SS + dx) * 4
                    r += src[i]
                    g += src[i + 1]
                    b += src[i + 2]
                    a += src[i + 3]
            n = SS * SS
            line += bytes((r // n, g // n, b // n, a // n))
        out.append(bytes(line))
    write_png(icons / "tray.png", out, 44)
    print("готово")


if __name__ == "__main__":
    main()
