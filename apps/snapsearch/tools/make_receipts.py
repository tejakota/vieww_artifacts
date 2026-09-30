#!/usr/bin/env python3
"""Regenerate snapsearch's render receipts (sheet.png, anim.gif) from shots/.

Run after `cargo run --bin screenshot --features native -- shots/`:

    python3 tools/make_receipts.py

The receipts follow the film_lab/renders convention: a dark plate carrying
every screen, numbered and captioned, and a seamless-loop GIF of the flow.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent.parent
SHOTS = HERE / "shots"
RENDERS = HERE / "renders"

SCREENS = [
    "01-library",
    "02-search-sheet",
    "03-photo-mode",
    "04-searching",
    "05-results",
    "06-stagger",
    "07-scrolled",
    "08-detail",
    "09-no-match",
    "10-space-map",
    "11-space-matches",
]

CAPTIONS = [
    "01 the library, alive on its own clock",
    "02 find a photo — the sheet",
    "03 use a photo — the reference picker",
    "04 searching, the sparkle medallion",
    "05 matches, gradient relevance badges",
    "06 the staggered reveal, mid-flight",
    "07 scrolled — the collapsing header",
    "08 the detail view, chips and ramp exit",
    "09 no match — the framed illustration",
    "10 the space view — the library as a similarity map",
    "11 space + matches — ringed in viridis, the rest dimmed",
]

TITLE = "snapsearch — describe a photo, find the photo"
SHEET_W = 1790
MARGIN = 18
GUTTER = 12
TITLE_H = 46
CAPTION_H = 34
ROW_GAP = 26


def load_font(bold: bool, size: int) -> ImageFont.FreeTypeFont:
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    return ImageFont.truetype(f"/usr/share/fonts/truetype/dejavu/{name}", size)


def make_sheet() -> Path:
    screens = [Image.open(SHOTS / f"{n}.png").convert("RGB") for n in SCREENS]

    # Two rows: six then five. Screens are 390x844; fit the width.
    budget_w = SHEET_W - MARGIN * 2 - GUTTER * (6 - 1)
    scale = budget_w / (390.0 * 6)
    size = (round(390 * scale), round(844 * scale))
    screens = [s.resize(size, Image.LANCZOS) for s in screens]
    rows = [screens[:6], screens[6:]]

    sheet_h = TITLE_H + 2 * (size[1] + CAPTION_H) + ROW_GAP + MARGIN * 2
    plate = Image.new("RGB", (SHEET_W, sheet_h), (8, 10, 13))
    draw = ImageDraw.Draw(plate)
    title_font = load_font(bold=True, size=17)
    caption_font = load_font(bold=False, size=13)

    draw.text((MARGIN, 14), TITLE, font=title_font, fill=(232, 236, 240))

    y = TITLE_H
    index = 0
    for row in rows:
        x = MARGIN
        for screen in row:
            draw.rectangle(
                [x - 1, y - 1, x + size[0], y + size[1]],
                outline=(48, 52, 58),
                width=1,
            )
            plate.paste(screen, (x, y))
            draw.text((x, y + size[1] + 8), CAPTIONS[index], font=caption_font, fill=(150, 156, 164))
            x += size[0] + GUTTER
            index += 1
        y += size[1] + CAPTION_H + ROW_GAP

    out = RENDERS / "sheet.png"
    plate.save(out, optimize=True)
    return out


# ── the GIF ──────────────────────────────────────────────────────────────────
#
# 19 frames per screen at 10fps: 1.3 s hold + 0.6 s crossfade, the last
# crossfading back to the first so the loop is seamless.

FPS = 10
HOLD = 13
FADE = 6
SIZE = (390, 844)


def make_gif() -> Path:
    frames_src = [Image.open(SHOTS / f"{n}.png").convert("RGB").resize(SIZE, Image.LANCZOS)
                  for n in SCREENS]

    frames = []
    for index, screen in enumerate(frames_src):
        for _ in range(HOLD):
            frames.append(screen.copy())
        nxt = frames_src[(index + 1) % len(frames_src)]
        for step in range(1, FADE + 1):
            frames.append(Image.blend(screen, nxt, step / FADE))

    # The shared palette: median-cut over the frames themselves, then every
    # frame quantised onto it with floyd-steinberg dithering.
    sample_img = Image.new("RGB", (130, 281))
    pixels = []
    for frame in frames[::8]:
        pixels.extend(frame.resize((130, 281)).getdata())
    sample_img.putdata(pixels[:130 * 281])
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
    missing = [n for n in SCREENS if not (SHOTS / f"{n}.png").exists()]
    if missing:
        print(f"missing shots: {missing}", file=sys.stderr)
        return 1
    RENDERS.mkdir(exist_ok=True)
    sheet = make_sheet()
    gif = make_gif()
    print(f"{sheet} ({sheet.stat().st_size} bytes)")
    print(f"{gif} ({gif.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
