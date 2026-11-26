# Takumi Anything app icon

A mason's trowel laying a two-course brick wall (匠 *takumi*: master craftsman). Flat vector with
hard-edged facets: warm charcoal `#1C1B19`, limestone `#D8D3C8`, terracotta `#C4623A`, walnut
`#B17848`. No gradients, no gloss.

## Files

- `takumi-anything.svg`: the artwork, used for 40 px and up (also the hicolor scalable icon).
- `takumi-anything-small.svg`: the same picture with thicker joints, used for 16-32 px and wherever
  the app draws its icon at 40 points or less (the title bar).
- `takumi-anything-1024.png`, `takumi-anything.icns`, `takumi-anything.ico` (16, 20, 24, 32, 40, 48,
  64, 128, 256).
- `hicolor/<size>/apps/io.github.mitroshenkov87.takumi-anything.png` (16-512) and `hicolor/scalable`.
- `../../docs/brand/takumi-anything-wordmark.svg`: the wordmark (icon and name, letters outlined).

## Regenerate

```sh
python assets/app-icon/source/gen.py   # needs rsvg-convert and Pillow; writes the files above
```
