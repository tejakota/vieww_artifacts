"""Build a Windows .ico from the app's own 256 px icon render.

    python packaging/make-ico.py icon-256.png wright.ico

A .ico is a small container of PNG/BMP images; Pillow writes every size
Explorer asks for (16 px in lists, 32 in the Start menu, 256 in tiles).
Used only on the Windows release runner, never by the app.
"""

import sys

from PIL import Image

src, dst = sys.argv[1], sys.argv[2]
Image.open(src).save(dst, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print(f"wrote {dst}")
