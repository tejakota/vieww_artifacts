#!/usr/bin/env python3
"""Regenerate vavlt's render receipts (sheet.png, anim.gif) from the
screenshot suite's output.

Run the suite first (`cargo test -p vavlt-app --test screenshots`, which
writes `target/screenshots/`), then:

    python3 tools/make_receipts.py

The receipts follow the film_lab/renders convention: a dark plate carrying
every screen, numbered and captioned, and a seamless-loop GIF of the flow.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent.parent
SHOTS = HERE / "target" / "screenshots"
RENDERS = HERE / "renders"

# Phone-dark variants — the sheet is the phone story; the suite's full set
# (phone+desktop, dark+light) stays in target/screenshots/.
SCREENS = [
    "01-vavlt-empty-phone-dark",
    "02-vavlt-after-run-phone-dark",
    "03-plan-move-phone-dark",
    "04-plan-deep-phone-dark",
    "05-working-phone-dark",
    "06-outcome-phone-dark",
    "09-activity-phone-dark",
    "10-settings-phone-dark",
    "11-activity-composition-phone-dark",
]

CAPTIONS = [
    "01 the vavlt tab, before a pick",
    "02 after a run — proven savings",
    "03 plan — move, the safe tier",
    "04 plan — deep move, warned",
    "05 working — the live grid",
    "06 what changed",
    "07 the audit log, with what was handed over",
    "08 settings",
    "09 a mixed vault — the composition card",
]

TITLE = "vavlt — consent-first, proven savings"
SHEET_W = 1290
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

    # Two rows: five then four. Screens are 412x915; fit the width.
    budget_w = SHEET_W - MARGIN * 2 - GUTTER * (5 - 1)
    scale = budget_w / (412.0 * 5)
    size = (round(412 * scale), round(915 * scale))
    screens = [s.resize(size, Image.LANCZOS) for s in screens]
    rows = [screens[:5], screens[5:]]

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
# 15 frames per screen at 10fps: 1.1 s hold + 0.4 s crossfade, the last
# crossfading back to the first so the loop is seamless.

FPS = 10
HOLD = 11
FADE = 4
SIZE = (412, 915)


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
    sample_w, sample_h = 137, 305
    sample_img = Image.new("RGB", (sample_w, sample_h))
    pixels = []
    for frame in frames[::8]:
        pixels.extend(frame.resize((sample_w, sample_h)).getdata())
    sample_img.putdata(pixels[:sample_w * sample_h])
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
