"""Our own app icon (no game art): a sand-stone tower with a crimson pennant on dark leather. Writes AppIcon.png
(1024 px) next to this script; viewer/bundle.sh turns it into AppIcon.icns.
Usage: uv run --with Pillow python -I viewer/icon/draw_icon.py"""
import os
from PIL import Image, ImageDraw
S = 1024
im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
# macOS icon grid: 824 px rounded square, centred
m, r = 100, 185
bg = Image.new("RGBA", (S, S))
for y in range(S):  # warm vertical gradient
    t = y / S
    ImageDraw.Draw(bg).line([(0, y), (S, y)], fill=(int(70 - 40 * t), int(46 - 28 * t), int(26 - 16 * t), 255))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([m, m, S - m, S - m], r, fill=255)
im.paste(bg, (0, 0), mask)
d = ImageDraw.Draw(im)
d.rounded_rectangle([m + 10, m + 10, S - m - 10, S - m - 10], r - 10, outline=(201, 160, 82, 255), width=10)
stone, shade, dark = (226, 196, 140, 255), (186, 152, 98, 255), (60, 36, 20, 255)
# tower body with a slight taper, crenellations, arrow slit, door
x0, x1, top, base = 360, 664, 360, 800
d.polygon([(x0 - 20, base), (x0, top), (x1, top), (x1 + 20, base)], fill=stone)
d.polygon([(512, top), (x1, top), (x1 + 20, base), (512, base)], fill=shade)
lx0, lx1 = x0 - 34, x1 + 34  # the parapet ledge
for i in range(5):  # merlons, evenly over the ledge
    cx = lx0 + i * (lx1 - lx0 - 56) // 4
    d.rectangle([cx, top - 70, cx + 56, top + 4], fill=stone if cx + 28 < 512 else shade)
d.rectangle([lx0, top - 6, lx1, top + 30], fill=stone)
for y in range(408, 790, 48):  # masonry courses
    d.line([(x0 - 6 + (y - top) * 20 // (base - top), y), (x1 + 6 - 0 + (y - top) * 20 // (base - top), y)], fill=(150, 118, 72, 255), width=4)
d.rounded_rectangle([497, 460, 527, 560], 12, fill=dark)  # arrow slit
d.pieslice([452, 640, 572, 760], 180, 360, fill=dark)  # arched door
d.rectangle([452, 700, 572, 800], fill=dark)
# pole and crimson pennant
d.rectangle([506, 150, 518, 300], fill=(90, 62, 40, 255))
d.polygon([(518, 160), (690, 196), (518, 232)], fill=(158, 30, 24, 255))
d.polygon([(518, 160), (690, 196), (600, 196)], fill=(190, 46, 36, 255))
# ground
d.rounded_rectangle([250, 790, 774, 830], 18, fill=(98, 70, 40, 255))
im.save(os.path.join(os.path.dirname(os.path.abspath(__file__)), "AppIcon.png"))
