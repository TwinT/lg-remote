"""Genera los íconos desde el dibujo base (TV + ondas + control), sin textos ni logos de terceros.
Uso: python3 tools/make_icon.py "icon/LG Doom Remote Icon.png"
Salidas en icon/: icon.png (1024, esquinas redondeadas), play-store-512.png, adaptive-fg-432.png, adaptive-bg.txt
"""
import sys
from PIL import Image, ImageDraw, ImageFilter

src = Image.open(sys.argv[1]).convert("RGB")
BG = (13, 12, 14)
S = 1024
pink = src.getpixel((630, 495))            # rosa de las ondas

# 1) borrar logo+texto de la pantalla (relleno con el promedio del borde, fundido) y dibujar un play
R = (395, 238, 870, 472)
ring = [src.getpixel(p) for p in [(R[0] - 12, R[1]), (R[0] - 12, R[3]), (R[2] + 12, R[1]), (R[2] + 12, R[3])]]
screen = tuple(sum(c[i] for c in ring) // len(ring) for i in range(3))
patch_mask = Image.new("L", src.size, 0)
ImageDraw.Draw(patch_mask).rectangle(R, fill=255)
patch_mask = patch_mask.filter(ImageFilter.GaussianBlur(6))
src.paste(Image.new("RGB", src.size, screen), (0, 0), patch_mask)
K = 4
layer = Image.new("RGBA", (src.width * K, src.height * K), (0, 0, 0, 0))
cx, cy, r = 640, 355, 82
tri = [(cx - r * 0.62, cy - r), (cx - r * 0.62, cy + r), (cx + r, cy)]
ImageDraw.Draw(layer).polygon([(x * K, y * K) for x, y in tri], fill=pink + (255,))
layer = layer.filter(ImageFilter.GaussianBlur(K * 1.5)).resize(src.size, Image.LANCZOS)
src = Image.alpha_composite(src.convert("RGBA"), layer).convert("RGB")

# 2) recorte del dibujo (sin el texto de abajo) fundido sobre fondo plano
box = (218, 157, 1034, 897)
art = src.crop(box)
w, h = art.size
mask = Image.new("L", (w, h), 0)
ImageDraw.Draw(mask).rectangle((26, 26, w - 27, h - 27), fill=255)
mask = mask.filter(ImageFilter.GaussianBlur(14))
centre = ((248 + 1004) // 2 - box[0], (187 + 867) // 2 - box[1])

def compose(size, scale, transparent=False):
    c = Image.new("RGBA" if transparent else "RGB", (size, size), (0, 0, 0, 0) if transparent else BG)
    a = art.resize((round(w * scale * size / S), round(h * scale * size / S)), Image.LANCZOS)
    m = mask.resize(a.size, Image.LANCZOS)
    pos = (size // 2 - round(centre[0] * scale * size / S), size // 2 - round(centre[1] * scale * size / S))
    c.paste(a, pos, m)
    return c

full = compose(S, 1.0)
full.resize((512, 512), Image.LANCZOS).save("icon/play-store-512.png")
rounded = full.convert("RGBA")
rm = Image.new("L", (S * 4, S * 4), 0)
ImageDraw.Draw(rm).rounded_rectangle((0, 0, S * 4 - 1, S * 4 - 1), radius=int(S * 4 * 0.225), fill=255)
rounded.putalpha(rm.resize((S, S), Image.LANCZOS))
rounded.save("icon/icon.png")
# ícono adaptativo: el dibujo debe caber en la zona segura (66% del lienzo de 108dp)
compose(432, 0.78, transparent=True).save("icon/adaptive-fg-432.png")
open("icon/adaptive-bg.txt", "w").write("#%02x%02x%02x\n" % BG)
print("ok")
