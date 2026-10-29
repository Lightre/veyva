"""Generate the Windows app icon and preview from the geometry in assets/mark.svg.

Requires Pillow. Run from the repository root:
  python tools/generate_icon.py
"""
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
ICO = ASSETS / "veyra.ico"
PREVIEW = ASSETS / "preview.png"
SCALE = 4
SIZE = 256 * SCALE


def xy(points):
    return [(round(x * SCALE), round(y * SCALE)) for x, y in points]


def main():
    # Transparent outside the rounded ink tile, matching the source SVG.
    im = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(im)
    draw.rounded_rectangle((0, 0, SIZE - 1, SIZE - 1), radius=60 * SCALE,
                           fill="#10151C")

    # SVG arc M204 128 A76 76 0 1 1 128 52: clockwise, through
    # bottom, left, and top around a circle centered at (128,128).
    box = (52 * SCALE, 52 * SCALE, 204 * SCALE, 204 * SCALE)
    stroke = 16 * SCALE
    draw.arc(box, start=0, end=270, fill="#70E2BC", width=stroke)
    cap = stroke // 2
    for point in ((204, 128), (128, 52)):
        x, y = xy([point])[0]
        draw.ellipse((x - cap, y - cap, x + cap, y + cap), fill="#70E2BC")

    # SVG arrowhead M108 32 L128 52 L108 72, with round caps and join.
    arrow = xy([(108, 32), (128, 52), (108, 72)])
    draw.line(arrow, fill="#70E2BC", width=stroke, joint="curve")
    for x, y in (arrow[0], arrow[-1]):
        draw.ellipse((x - cap, y - cap, x + cap, y + cap), fill="#70E2BC")

    # SVG play triangle M108 96 L108 160 L160 128 Z.
    draw.polygon(xy([(108, 96), (108, 160), (160, 128)]), fill="#EEF4F7")

    im.resize((512, 512), Image.Resampling.LANCZOS).save(PREVIEW)
    im.resize((256, 256), Image.Resampling.LANCZOS).save(
        ICO, format="ICO", sizes=[(256, 256), (128, 128), (64, 64),
                                   (48, 48), (32, 32), (16, 16)])
    print(f"Wrote {ICO}")
    print(f"Wrote {PREVIEW}")


if __name__ == "__main__":
    main()
