#!/usr/bin/env python3
"""Generate the sample photographs `upxcale` ships in `assets/photos/`.

These are synthetic on purpose. The app needs a set of *low-resolution* source
images so that upscaling them is a visible operation rather than a no-op, and it
needs to ship them under a licence nobody has to check. Twelve procedural
landscapes at ~320px on the long edge do both.

Run from the crate root:

    python3 tools/make_photos.py

Regenerating is deterministic — every image is seeded by name — so re-running
this never produces a diff unless the code here changed.
"""

import math
import os

import numpy as np
from PIL import Image, ImageFilter

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets", "photos")

# name, (width, height), palette family. Sizes are deliberately small and
# deliberately varied — the landing grid is a heterogeneous masonry, so a set of
# identically-shaped images would not exercise it.
SCENES = [
    ("mountain-ridge",  (240, 320), "alpine"),
    ("still-lake",      (300, 300), "alpine"),
    ("pine-forest",     (320, 240), "forest"),
    ("dune-field",      (200, 340), "desert"),
    ("city-dusk",       (300, 300), "city"),
    ("aurora-night",    (240, 320), "aurora"),
    ("north-coast",     (320, 240), "coast"),
    ("summer-meadow",   (300, 300), "meadow"),
    ("red-canyon",      (240, 320), "desert"),
    ("morning-fog",     (340, 200), "forest"),
    ("high-peak",       (300, 300), "alpine"),
    ("slow-river",      (320, 240), "coast"),
]

PALETTES = {
    #            sky top          sky bottom       land near        land far
    "alpine": ((38, 62, 104),   (150, 178, 205), (46, 54, 66),    (96, 112, 130)),
    "forest": ((58, 78, 62),    (168, 190, 152), (24, 42, 30),    (62, 92, 60)),
    "desert": ((132, 96, 68),   (232, 186, 132), (120, 68, 44),   (186, 122, 78)),
    "city":   ((28, 34, 58),    (196, 132, 112), (18, 20, 32),    (52, 56, 78)),
    "aurora": ((10, 14, 32),    (28, 46, 70),    (12, 18, 28),    (30, 44, 56)),
    "coast":  ((60, 96, 128),   (186, 206, 216), (38, 62, 78),    (86, 122, 140)),
    "meadow": ((92, 128, 168),  (206, 220, 226), (74, 106, 52),   (140, 164, 88)),
}


def value_noise(rng, w, h, octaves=5):
    """Fractal value noise in 0..1 — the texture under everything else."""
    total = np.zeros((h, w), dtype=np.float32)
    amplitude = 1.0
    norm = 0.0
    size = 2
    for _ in range(octaves):
        grid = rng.random((size + 1, size + 1)).astype(np.float32)
        # Bilinear upsample to full size via PIL, which is faster and smoother
        # than doing it by hand and is only ever used for texture here.
        layer = np.asarray(
            Image.fromarray((grid * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC),
            dtype=np.float32,
        ) / 255.0
        total += layer * amplitude
        norm += amplitude
        amplitude *= 0.5
        size *= 2
    return total / norm


def ridge(rng, w, base, roughness, octaves=6):
    """A 1-D horizon line: midpoint displacement, returned as height per column."""
    n = 1
    while n < w:
        n *= 2
    pts = np.zeros(n + 1, dtype=np.float32)
    pts[0] = rng.random()
    pts[-1] = rng.random()
    step = n
    scale = roughness
    while step > 1:
        half = step // 2
        for i in range(half, n, step):
            pts[i] = (pts[i - half] + pts[i + half]) / 2.0 + (rng.random() - 0.5) * scale
        step = half
        scale *= 0.55
    pts = pts[:w]
    lo, hi = pts.min(), pts.max()
    if hi - lo > 1e-6:
        pts = (pts - lo) / (hi - lo)
    return base + pts * (1.0 - base) * 0.45


def render(name, size, family):
    w, h = size
    # Seed from the name so the set is reproducible and each scene differs.
    rng = np.random.default_rng(abs(hash(name)) % (2**32))
    sky_top, sky_bottom, land_near, land_far = (np.array(c, dtype=np.float32) for c in PALETTES[family])

    yy = np.linspace(0.0, 1.0, h, dtype=np.float32)[:, None]
    xx = np.linspace(0.0, 1.0, w, dtype=np.float32)[None, :]

    # ── sky: a vertical gradient, gamma-bent so the horizon glow is not linear
    t = yy ** 1.35
    img = sky_top[None, None, :] * (1 - t)[..., None] + sky_bottom[None, None, :] * t[..., None]

    # ── sun / moon: a soft radial bloom sitting just above the horizon
    sun_x = 0.2 + rng.random() * 0.6
    sun_y = 0.30 + rng.random() * 0.22
    d = np.sqrt(((xx - sun_x) * (w / h)) ** 2 + (yy - sun_y) ** 2)
    glow = np.exp(-(d ** 2) / 0.035)[..., None]
    warm = np.array([255, 226, 178], dtype=np.float32) if family != "aurora" else np.array([120, 255, 200], dtype=np.float32)
    img = img * (1 - glow * 0.75) + warm[None, None, :] * glow * 0.75

    if family == "aurora":
        # Two vertical curtains of light, sinusoidally displaced.
        for band in range(2):
            phase = rng.random() * math.tau
            centre = 0.3 + band * 0.3
            offset = np.sin(xx * 9.0 + phase) * 0.05
            band_d = np.abs(yy - (centre + offset))
            curtain = np.exp(-(band_d ** 2) / 0.006)[..., None]
            tint = np.array([80, 240, 170], dtype=np.float32) if band == 0 else np.array([140, 170, 255], dtype=np.float32)
            img = img + tint[None, None, :] * curtain * 0.55

        stars = rng.random((h, w)) > 0.9975
        img[stars] = np.array([235, 240, 255], dtype=np.float32)

    # ── layered terrain, far to near, each a displaced ridge line
    layers = 3 if family in ("alpine", "desert", "city") else 2
    for i in range(layers):
        depth = i / max(layers - 1, 1)
        base = 0.42 + i * 0.14
        line = ridge(rng, w, base, roughness=0.55 - depth * 0.25)
        mask = (yy >= line[None, :]).astype(np.float32)
        # Soften the silhouette by a fraction of a pixel so it is not stair-stepped.
        mask = np.asarray(
            Image.fromarray((mask * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.6)),
            dtype=np.float32,
        ) / 255.0
        colour = land_far * (1 - depth) + land_near * depth
        shade = value_noise(rng, w, h, octaves=4)[..., None] * 0.35 + 0.82
        img = img * (1 - mask[..., None]) + (colour[None, None, :] * shade) * mask[..., None]

    if family == "city":
        # A skyline of lit windows along the nearest band.
        ground = int(h * 0.72)
        x = 0
        while x < w:
            bw = int(6 + rng.random() * 14)
            bh = int((0.10 + rng.random() * 0.26) * h)
            top = max(ground - bh, 0)
            img[top:ground, x:min(x + bw, w)] = np.array([16, 18, 28], dtype=np.float32)
            for wy in range(top + 3, ground - 2, 5):
                for wx in range(x + 2, min(x + bw, w) - 2, 4):
                    if rng.random() > 0.45:
                        img[wy:wy + 2, wx:wx + 2] = np.array([248, 214, 140], dtype=np.float32)
            x += bw + int(1 + rng.random() * 3)

    if family in ("coast", "alpine") and rng.random() > 0.35:
        # Water: mirror the lower third and streak it horizontally.
        split = int(h * 0.70)
        reflection = img[:split][::-1][: h - split]
        if reflection.shape[0] == h - split:
            blend = np.linspace(0.55, 0.15, h - split, dtype=np.float32)[:, None, None]
            img[split:] = img[split:] * (1 - blend) + reflection * blend
            streak = np.sin(np.linspace(0, 40, h - split, dtype=np.float32))[:, None, None] * 3.0
            img[split:] += streak

    # ── grain, then a light blur: this is what makes it read as a photograph
    # rather than as a gradient, and it also gives the sharpener something real
    # to recover.
    grain = (value_noise(rng, w, h, octaves=6) - 0.5)[..., None] * 14.0
    img = img + grain

    out = Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), mode="RGB")
    out = out.filter(ImageFilter.GaussianBlur(0.35))
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, size, family in SCENES:
        path = os.path.join(OUT, f"{name}.png")
        render(name, size, family).save(path, optimize=True)
        print(f"wrote {path} ({size[0]}x{size[1]})")


if __name__ == "__main__":
    main()
