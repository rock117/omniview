"""Rasterize Omniview radar icon to multi-size ICO (matches 02-radar.svg)."""
from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parents[1] / "assets" / "icons"


def render(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = size / 128.0
    rad = max(1, int(round(28 * s)))
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=rad, fill=(15, 23, 42, 255))
    cx, cy = 64 * s, 64 * s

    for radius, width, color in [
        (40, 3.0, (51, 65, 85, 255)),
        (26, 2.5, (71, 85, 105, 255)),
        (12, 2.0, (100, 116, 139, 255)),
    ]:
        w = max(1, int(round(width * s)))
        r = radius * s
        d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=color, width=w)

    R = 40 * s
    # SVG sweep: north -> (98,48). PIL pieslice: 0=east, clockwise.
    end = 360.0 - math.degrees(math.atan2(16.0, 34.0))
    wedge = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    wd = ImageDraw.Draw(wedge)
    wd.pieslice([cx - R, cy - R, cx + R, cy + R], start=270, end=end, fill=(0, 120, 212, 217))
    img = Image.alpha_composite(img, wedge)
    d = ImageDraw.Draw(img)

    cr = 5 * s
    d.ellipse([cx - cr, cy - cr, cx + cr, cy + cr], fill=(56, 189, 248, 255))
    br = 4 * s
    bx, by = 88 * s, 42 * s
    d.ellipse([bx - br, by - br, bx + br, by + br], fill=(248, 250, 252, 255))
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [render(s) for s in sizes]
    ico = OUT / "omniview.ico"
    images[-1].save(ico, format="ICO", sizes=[(s, s) for s in sizes])
    images[-1].save(OUT / "omniview-256.png")
    print(f"wrote {ico} ({ico.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
