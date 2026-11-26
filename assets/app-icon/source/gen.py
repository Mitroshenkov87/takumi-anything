"""Regenerate every Takumi Anything icon file from the two hand-written SVGs.

takumi-anything.svg is the artwork (40 px and up); takumi-anything-small.svg is the same picture
drawn with thicker joints for 16-32 px. Needs rsvg-convert (librsvg) and Pillow.

    python assets/app-icon/source/gen.py
"""
import io, subprocess
from pathlib import Path
from PIL import Image

DIR = Path(__file__).resolve().parent.parent
ID = "io.github.mitroshenkov87.takumi-anything"
FULL, SMALL = DIR / "takumi-anything.svg", DIR / "takumi-anything-small.svg"


def render(size):
    svg = SMALL if size <= 32 else FULL
    png = subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), str(svg)], check=True, capture_output=True).stdout
    return Image.open(io.BytesIO(png)).convert("RGBA")


def main():
    render(1024).save(DIR / "takumi-anything-1024.png")
    for s in [16, 24, 32, 48, 64, 128, 256, 512]:
        out = DIR / "hicolor" / f"{s}x{s}" / "apps"
        out.mkdir(parents=True, exist_ok=True)
        render(s).save(out / f"{ID}.png")
    scal = DIR / "hicolor" / "scalable" / "apps"
    scal.mkdir(parents=True, exist_ok=True)
    (scal / f"{ID}.svg").write_bytes(FULL.read_bytes())
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    imgs = {s: render(s) for s in sizes}
    imgs[256].save(DIR / "takumi-anything.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=[imgs[s] for s in sizes if s != 256])
    # macOS: Apple's grid puts the tile at 824/1024 with a transparent margin.
    big = render(824)
    canvas = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    canvas.paste(big, (100, 100), big)
    canvas.save(DIR / "takumi-anything.icns", format="ICNS")
    print("icons written to", DIR)


if __name__ == "__main__":
    main()
