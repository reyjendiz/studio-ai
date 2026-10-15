"""Builds the round app icon: the header mark (ring, centre dot, orbiting accent dot) on a flat charcoal disc.
usage: python make_icon.py            ->  ui/studio.png (512, round), ui/studio.ico (16..256), studio/assets/icon.png
       python make_icon.py BG.png     ->  the same mark over an AI-generated background instead (assets/icon_src/bg_*.png)"""
import sys, math, pathlib
from PIL import Image, ImageDraw, ImageFilter

root = pathlib.Path(__file__).resolve().parents[2]
S = 1024
c = S / 2
INK = (244, 246, 238, 255)
ACCENT = (212, 242, 74, 255)

if len(sys.argv) > 1:
    bg = Image.open(sys.argv[1]).convert("RGB")
    z = int(bg.width * 0.18)
    bg = bg.crop((z, z, bg.width - z, bg.height - z)).resize((S, S), Image.LANCZOS)
    img = bg.convert("RGBA")
else:
    # flat charcoal with a barely visible lift towards the top right, where the accent dot sits
    img = Image.new("RGBA", (S, S), (14, 14, 16, 255))
    lift = Image.new("L", (S, S), 0)
    ImageDraw.Draw(lift).ellipse((S * 0.45, -S * 0.15, S * 1.25, S * 0.65), fill=255)
    lift = lift.filter(ImageFilter.GaussianBlur(160)).point(lambda v: v * 38 // 255)
    img.paste(Image.new("RGBA", (S, S), (212, 242, 74, 255)), mask=lift)

# the mark, bold enough to stay legible at 16 px
ring_r, w = S * 0.27, int(S * 0.062)
mark = Image.new("RGBA", (S * 2, S * 2), (0, 0, 0, 0))   # drawn at 2x and scaled down for smooth edges
m = ImageDraw.Draw(mark)
k = 2
m.ellipse(((c - ring_r) * k, (c - ring_r) * k, (c + ring_r) * k, (c + ring_r) * k), outline=INK, width=w * k)
dot = S * 0.068
m.ellipse(((c - dot) * k, (c - dot) * k, (c + dot) * k, (c + dot) * k), fill=INK)
ang = math.radians(-45)
ox, oy = c + ring_r * math.cos(ang), c + ring_r * math.sin(ang)
od = S * 0.095
# a thin gap in the dark around the accent dot so it reads as sitting on the ring
m.ellipse(((ox - od * 1.3) * k, (oy - od * 1.3) * k, (ox + od * 1.3) * k, (oy + od * 1.3) * k), fill=(14, 14, 16, 255))
m.ellipse(((ox - od) * k, (oy - od) * k, (ox + od) * k, (oy + od) * k), fill=ACCENT)
img.alpha_composite(mark.resize((S, S), Image.LANCZOS))

# round: antialiased circle mask via 4x supersampling, plus a thin light rim
mask = Image.new("L", (S * 4, S * 4), 0)
ImageDraw.Draw(mask).ellipse((0, 0, S * 4 - 1, S * 4 - 1), fill=255)
mask = mask.resize((S, S), Image.LANCZOS)
rim = Image.new("RGBA", (S * 4, S * 4), (0, 0, 0, 0))
ImageDraw.Draw(rim).ellipse((6, 6, S * 4 - 7, S * 4 - 7), outline=(255, 255, 255, 40), width=14)
img.alpha_composite(rim.resize((S, S), Image.LANCZOS))
img.putalpha(mask)

out_png = root / "ui" / "studio.png"
img.resize((512, 512), Image.LANCZOS).save(out_png)
img.save(root / "ui" / "studio.ico", sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (24, 24), (16, 16)])
img.resize((512, 512), Image.LANCZOS).save(root / "studio" / "assets" / "icon.png")
print("ok", out_png)
