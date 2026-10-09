"""Regenerates the installer's icon and wizard images (needs Pillow).
   python make_assets.py"""
from PIL import Image, ImageDraw, ImageFilter

ACC = (37, 99, 235)
BG1, BG2 = (15, 23, 42), (30, 41, 82)


def plate(size):
    """The app mark: a bumper plate seen from the front."""
    S = size * 4
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = S / 2
    for r, fill in [(0.47, ACC + (255,)), (0.30, (17, 24, 39, 255)), (0.095, (255, 255, 255, 255))]:
        d.ellipse((c - S * r, c - S * r, c + S * r, c + S * r), fill=fill)
    r = S * 0.39
    d.ellipse((c - r, c - r, c + r, c + r), outline=(255, 255, 255, 80), width=max(1, S // 64))
    return im.resize((size, size), Image.LANCZOS)


def layer(w, h):
    return Image.new("RGBA", (w, h), (0, 0, 0, 0))


def wizard(w, h):
    im = Image.linear_gradient("L").resize((w, h))
    base = Image.composite(Image.new("RGB", (w, h), BG2), Image.new("RGB", (w, h), BG1), im).convert("RGBA")
    cx, cy = w * 0.5, h * 0.42
    glow = layer(w, h)
    gr = w * 0.55
    ImageDraw.Draw(glow).ellipse((cx - gr, cy - gr, cx + gr, cy + gr), fill=ACC + (110,))
    base = Image.alpha_composite(base, glow.filter(ImageFilter.GaussianBlur(w * 0.22)))

    ov = layer(w, h)
    d = ImageDraw.Draw(ov)
    p = int(w * 0.62)
    x0, y0 = int(cx - p / 2), int(cy - p / 2)
    L, t = int(p * 0.22), max(2, w // 80)
    for x, y, dx, dy in [(x0, y0, 1, 1), (x0 + p, y0, -1, 1), (x0, y0 + p, 1, -1), (x0 + p, y0 + p, -1, -1)]:
        d.line((x, y, x + dx * L, y), fill=(147, 197, 253, 190), width=t)
        d.line((x, y, x, y + dy * L), fill=(147, 197, 253, 190), width=t)
    by = int(h * 0.80)
    for i, a in enumerate([70, 40, 22]):
        yy = by + i * int(h * 0.035)
        d.rounded_rectangle((int(w * 0.2), yy, int(w * 0.8), yy + max(2, w // 70)), radius=max(1, w // 100), fill=(255, 255, 255, a))
    base = Image.alpha_composite(base, ov)

    pl = plate(int(p * 0.72))
    base.alpha_composite(pl, (int(cx - pl.width / 2), int(cy - pl.height / 2)))
    scan = layer(w, h)
    sy = int(cy + p * 0.08)
    ImageDraw.Draw(scan).line((x0 + t * 3, sy, x0 + p - t * 3, sy), fill=(96, 165, 250, 210), width=max(1, t // 2))
    return Image.alpha_composite(base, scan).convert("RGB")


plate(256).save("gym-app.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
for scale, (w, h) in [(100, (164, 314)), (150, (246, 471)), (200, (328, 628))]:
    wizard(w, h).save(f"wizard-{scale}.bmp")
for scale, s in [(100, 55), (150, 83), (200, 110)]:
    im = Image.new("RGBA", (s, s), (255, 255, 255, 255))
    pl = plate(int(s * 0.9))
    im.alpha_composite(pl, ((s - pl.width) // 2, (s - pl.height) // 2))
    im.convert("RGB").save(f"wizard-small-{scale}.bmp")
