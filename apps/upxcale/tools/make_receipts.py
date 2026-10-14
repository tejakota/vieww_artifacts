#!/usr/bin/env python3
"""Regenerate upxcale's render receipts (sheet.png, anim.gif) from shots/.

Run after `cargo run --example shots --features native -- shots/`:

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

# ── the sheet ────────────────────────────────────────────────────────────────

SHEET_W, SHEET_H = 1790, 884
TITLE = "upxcale — Lanczos-3 x4, on this device"
CAPTIONS = [
    "01 the grid, masthead above it",
    "02 the picker, frosted over the grid",
    "03 upscaling — ring and honest bar",
    "04 rendered — fresh ring and tick",
    "05 before / after, measured — the strip under the stage",
]
MARGIN = 18
GUTTER = 12
TITLE_H = 46
CAPTION_H = 34


def load_font(bold: bool, size: int) -> ImageFont.FreeTypeFont:
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    return ImageFont.truetype(f"/usr/share/fonts/truetype/dejavu/{name}", size)


def make_sheet() -> Path:
    screens = [Image.open(SHOTS / f"{n}.png").convert("RGB") for n in
               ("1-landing", "2-picker", "3-progress", "4-rendered", "5-compare")]

    # Fit five screens to the plate's height budget.
    budget_h = SHEET_H - TITLE_H - CAPTION_H - MARGIN * 2
    budget_w = SHEET_W - MARGIN * 2 - GUTTER * (len(screens) - 1)
    scale = min(budget_h / screens[0].height, budget_w / (screens[0].width * len(screens)))
    size = (round(screens[0].width * scale), round(screens[0].height * scale))
    screens = [s.resize(size, Image.LANCZOS) for s in screens]

    plate = Image.new("RGB", (SHEET_W, SHEET_H), (8, 10, 13))
    draw = ImageDraw.Draw(plate)
    title_font = load_font(bold=True, size=17)
    caption_font = load_font(bold=False, size=13)

    draw.text((MARGIN, 14), TITLE, font=title_font, fill=(232, 236, 240))

    x = MARGIN
    y = TITLE_H
    for index, screen in enumerate(screens):
        # A hairline frame so a dark screen edge reads as an edge.
        draw.rectangle(
            [x - 1, y - 1, x + size[0], y + size[1]],
            outline=(48, 52, 58),
            width=1,
        )
        plate.paste(screen, (x, y))
        caption = CAPTIONS[index]
        draw.text((x, y + size[1] + 8), caption, font=caption_font, fill=(150, 156, 164))
        x += size[0] + GUTTER

    out = RENDERS / "sheet.png"
    plate.save(out, optimize=True)
    return out


# ── the GIF ──────────────────────────────────────────────────────────────────
#
# 100 frames at 10fps: per screen, 1.4 s hold + 0.6 s crossfade to the next,
# the fifth crossfading back to the first so the loop is seamless.

FPS = 10
HOLD = 14
FADE = 6
SIZE = (414, 896)


def crossfade(a: Image.Image, b: Image.Image, t: float) -> Image.Image:
    return Image.blend(a, b, t)


def make_gif() -> Path:
    names = ("1-landing", "2-picker", "3-progress", "4-rendered", "5-compare")
    screens = [Image.open(SHOTS / f"{n}.png").convert("RGB").resize(SIZE, Image.LANCZOS)
               for n in names]

    frames = []
    for index, screen in enumerate(screens):
        for _ in range(HOLD):
            frames.append(screen.copy())
        nxt = screens[(index + 1) % len(screens)]
        for step in range(1, FADE + 1):
            frames.append(crossfade(screen, nxt, step / FADE))

    # The shared palette: median-cut over the frames themselves, then every
    # frame quantised onto it with floyd-steinberg dithering — a two-pass
    # recipe so no frame is quantised against its own local colours.
    sampler = Image.new("RGB", SIZE)
    sampler.paste(frames[0], (0, 0))
    all_pixels = []
    for frame in frames[::4]:
        all_pixels.extend(frame.resize((104, 224)).getdata())
    sample_img = Image.new("RGB", (104, 224))
    sample_img.putdata(all_pixels[:104 * 224])
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
    missing = [n for n in ("1-landing", "2-picker", "3-progress", "4-rendered", "5-compare")
               if not (SHOTS / f"{n}.png").exists()]
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
