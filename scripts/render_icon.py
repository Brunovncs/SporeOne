"""Builds assets/icon.ico from its SVGs (needs Python with Pillow, and Microsoft Edge): sizes up
to 32 come from icon-small.svg, drawn for small sizes, the larger ones from icon.svg. Also writes
assets/icon-256.png for the README.
"""

import io
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from render_svg import render  # noqa: E402

ASSETS = Path(__file__).resolve().parent.parent / "assets"


def ico(images: list) -> bytes:
    """An ICO file with one PNG entry per image, as Windows Vista and later read them."""
    blobs = []
    for im in images:
        buf = io.BytesIO()
        im.save(buf, "PNG")
        blobs.append(buf.getvalue())
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for im, blob in zip(images, blobs):
        w = im.width if im.width < 256 else 0
        out += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    return out + b"".join(blobs)


def main() -> None:
    small, large = ASSETS / "icon-small.svg", ASSETS / "icon.svg"
    app = [render(small if s <= 32 else large, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)]
    (ASSETS / "icon.ico").write_bytes(ico(app))
    app[-1].save(ASSETS / "icon-256.png")
    print("icon.ico, icon-256.png")


if __name__ == "__main__":
    main()
