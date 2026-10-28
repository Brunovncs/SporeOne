"""Makes SporeOne's icons from the logo: the transparent master `assets/icon.png`, the README's
`assets/icon-256.png`, the executable's and installer's `assets/icon.ico`, and the bar's small
icons in `assets/brand/`, one for each common display scale.

    python scripts/make_icon.py                  # from assets/icon.png
    python scripts/make_icon.py path/to/logo.jpg # a new logo on a light background

A new logo's shapes are measured in DISC and GEAR below: what is outside them is background.
"""

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
# The bar's icon is 28 pixels at 100%; GPUI draws images at the screen's pixels.
BRAND = [28, 35, 42, 56]
ICO = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]


# The logo's shapes in the source image: the blue disc (a little taller than wide), and the gear
# that sits over its edge, with the hole in its middle.
DISC = (854.5, 507.0, 287.5, 295.0)
GEAR = (1048.0, 742.5, 106.0)
HOLE = 49.0
GREY = (146, 147, 151)


def clamp(v: float) -> float:
    return max(0.0, min(1.0, v))


def without_background(img: Image.Image) -> Image.Image:
    """The disc as it is, white swirl included; outside it, only the gear, its grey taken from
    the light background by how dark each pixel is."""
    rgb = img.convert("RGB")
    w, h = rgb.size
    px = rgb.load()
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    po = out.load()
    cx, cy, rx, ry = DISC
    gx, gy, gr = GEAR
    for y in range(h):
        for x in range(w):
            p = px[x, y]
            # How far outside the disc's edge, in pixels, one pixel in to leave no light fringe.
            edge = (((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2) ** 0.5
            outside = (edge - 1) * (rx + ry) / 2 + 1
            disc = clamp(0.5 - outside)
            gear = 0.0
            d = ((x - gx) ** 2 + (y - gy) ** 2) ** 0.5
            if d <= HOLE:
                gear = 1.0
            elif d <= gr:
                gear = clamp((253 - min(p)) / (253 - GREY[0]))
            a = max(disc, gear)
            if a <= 0:
                continue
            color = p if disc >= gear or d <= HOLE else GREY
            po[x, y] = (*color, round(a * 255))
    return out.crop(out.getbbox())


def square(img: Image.Image, side: int) -> Image.Image:
    w, h = img.size
    scale = side / max(w, h)
    small = img.resize((max(1, round(w * scale)), max(1, round(h * scale))), Image.LANCZOS)
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    canvas.paste(small, ((side - small.width) // 2, (side - small.height) // 2), small)
    return canvas


def main() -> None:
    if len(sys.argv) > 1:
        master = square(without_background(Image.open(sys.argv[1])), 512)
        master.save(ASSETS / "icon.png")
    else:
        master = Image.open(ASSETS / "icon.png").convert("RGBA")
    square(master, 256).save(ASSETS / "icon-256.png")
    master.save(ASSETS / "icon.ico", sizes=[(s, s) for s in ICO])
    (ASSETS / "brand").mkdir(exist_ok=True)
    for s in BRAND:
        square(master, s).save(ASSETS / "brand" / f"brand-{s}.png")
    print("icons written to", ASSETS)


if __name__ == "__main__":
    main()
