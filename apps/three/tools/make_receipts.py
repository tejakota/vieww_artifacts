#!/usr/bin/env python3
"""Regenerate 3's render receipts (sheet.png, anim.gif).

Two inputs, both produced by the ignored receipt tests:

* `cargo test -p three-social-app -- --ignored --nocapture`
  → examples/social-app/target/screens/*.png (the five app screens)
* `cargo test -p vieww-integration --test previews -- --ignored --nocapture`
  → examples/vieww-integration/target/moment/*.png (the GIF's 30 frames:
    the 3s demo capture played through the real frame loop, rendered by the
    engine)

then:

    python3 tools/make_receipts.py

The receipts follow the film_lab/renders convention: a dark plate carrying
every screen, numbered and captioned, and the motion receipt as a GIF.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent.parent
SCREENS_DIR = HERE / "examples" / "social-app" / "target" / "screens"
MOMENT_DIR = HERE / "examples" / "vieww-integration" / "target" / "moment"
RENDERS = HERE / "renders"

SCREENS = ["1-home", "2-explore", "3-create", "4-activity", "5-profile"]
CAPTIONS = [
    "01 home — the moment card, engine-rendered",
    "02 explore — people, wired follow",
    "03 create — the pipeline timeline",
    "04 activity — the real trail",
    "05 profile — moments and numbers",
]

TITLE = "3 — temporal capture, the social product"
SHEET_W = 1790
MARGIN = 18
GUTTER = 12
TITLE_H = 46
CAPTION_H = 34

FPS = 10


def load_font(bold: bool, size: int) -> ImageFont.FreeTypeFont:
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    return ImageFont.truetype(f"/usr/share/fonts/truetype/dejavu/{name}", size)


def make_sheet() -> Path:
    screens = [Image.open(SCREENS_DIR / f"{n}.png").convert("RGB") for n in SCREENS]

    budget_w = SHEET_W - MARGIN * 2 - GUTTER * (len(screens) - 1)
    scale = budget_w / (480.0 * len(screens))
    size = (round(480 * scale), round(860 * scale))
    screens = [s.resize(size, Image.LANCZOS) for s in screens]

    sheet_h = TITLE_H + size[1] + CAPTION_H + MARGIN * 2
    plate = Image.new("RGB", (SHEET_W, sheet_h), (8, 10, 13))
    draw = ImageDraw.Draw(plate)
    title_font = load_font(bold=True, size=17)
    caption_font = load_font(bold=False, size=13)

    draw.text((MARGIN, 14), TITLE, font=title_font, fill=(232, 236, 240))

    x = MARGIN
    y = TITLE_H
    for index, screen in enumerate(screens):
        draw.rectangle(
            [x - 1, y - 1, x + size[0], y + size[1]],
            outline=(48, 52, 58),
            width=1,
        )
        plate.paste(screen, (x, y))
        draw.text((x, y + size[1] + 8), CAPTIONS[index], font=caption_font, fill=(150, 156, 164))
        x += size[0] + GUTTER

    out = RENDERS / "sheet.png"
    plate.save(out, optimize=True)
    return out


def make_gif() -> Path:
    frames = [Image.open(p).convert("RGB") for p in sorted(MOMENT_DIR.glob("moment-*.png"))]
    if not frames:
        print("no moment frames; run the previews receipt test first", file=sys.stderr)
        raise SystemExit(1)

    # The shared palette: median-cut over the frames themselves, then every
    # frame quantised onto it with floyd-steinberg dithering.
    sample_img = Image.new("RGB", (160, 273))
    pixels = []
    for frame in frames[::2]:
        pixels.extend(frame.resize((160, 273)).getdata())
    sample_img.putdata(pixels[:160 * 273])
    palette = sample_img.quantize(colors=255, method=Image.MEDIANCUT, dither=Image.NONE)

    out = RENDERS / "anim.gif"
    frames_p = [f.convert("RGB").quantize(palette=palette, dither=Image.FLOYDSTEINBERG)
                for f in frames]
    frames_p[0].save(
        out,
        save_all=True,
        append_images=frames_p[1:],
        duration=1000 // FPS,
        loop=0,
        optimize=True,
    )
    return out


def main() -> int:
    missing = [n for n in SCREENS if not (SCREENS_DIR / f"{n}.png").exists()]
    if missing:
        print(f"missing screens: {missing}", file=sys.stderr)
        return 1
    RENDERS.mkdir(exist_ok=True)
    sheet = make_sheet()
    gif = make_gif()
    print(f"{sheet} ({sheet.stat().st_size} bytes)")
    print(f"{gif} ({gif.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
