"""Render the simple SVG mark into ICO/PNG using Pillow; no network required.

The SVG is the only geometry source. Supported elements: rounded rect, polygon.
"""
from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image, ImageDraw

ASSETS = Path(__file__).resolve().parents[1] / "assets"
SCALE = 4

def main():
    image = Image.new("RGBA", (256 * SCALE, 256 * SCALE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    for element in ET.parse(ASSETS / "mark.svg").getroot():
        tag = element.tag.rsplit("}", 1)[-1]
        attrs = element.attrib
        if tag == "rect":
            x, y, width, height, radius = (float(attrs.get(key, 0)) * SCALE for key in ("x", "y", "width", "height", "rx"))
            draw.rounded_rectangle((x, y, x + width, y + height), radius=radius, fill=attrs["fill"])
        elif tag == "polygon":
            points = [tuple(float(value) * SCALE for value in pair.split(",")) for pair in attrs["points"].split()]
            draw.polygon(points, fill=attrs["fill"])
        else:
            raise ValueError(f"Unsupported SVG element: {tag}")
    image.resize((512, 512), Image.Resampling.LANCZOS).save(ASSETS / "preview.png")
    image.resize((256, 256), Image.Resampling.LANCZOS).save(
        ASSETS / "veyra.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("Generated preview.png and veyra.ico from mark.svg")

if __name__ == "__main__":
    main()
