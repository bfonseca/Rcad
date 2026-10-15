"""Build + review tooling for the rcad icon set.

  python3 build.py sheet <category|all> <light|dark> [out.png]   # review contact sheet
  python3 build.py dist                                          # write the full deliverable tree
"""
import importlib
import io
import json
import os
import pkgutil
import sys

import cairosvg
from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import kit  # noqa: E402

CATEGORIES = ["sketch", "constraints", "features", "surfaces", "curves", "datum",
              "assembly", "edit", "view", "general"]


def load_icons():
    import icons
    for m in pkgutil.iter_modules(icons.__path__):
        importlib.import_module(f"icons.{m.name}")


def content_bbox(svg, scale=8, pad=8):
    """bounding box (in icon units) of everything painted, measured on an enlarged canvas"""
    big = svg.replace('viewBox="0 0 32 32"', f'viewBox="{-pad} {-pad} {32 + 2 * pad} {32 + 2 * pad}"', 1)
    big = big.replace('width="32" height="32"', f'width="{(32 + 2 * pad) * scale}" height="{(32 + 2 * pad) * scale}"', 1)
    im = Image.open(io.BytesIO(cairosvg.svg2png(bytestring=big.encode()))).convert("RGBA")
    a = im.split()[3].point(lambda v: 255 if v > 24 else 0)
    bb = a.getbbox()
    if not bb:
        return None
    return (bb[0] / scale - pad, bb[1] / scale - pad, bb[2] / scale - pad, bb[3] / scale - pad)


LIMIT = (0.7, 31.3)


def fit_to_grid(svg):
    """If anything leaves the 32px grid (minus a hairline margin), scale + centre the whole icon."""
    bb = content_bbox(svg)
    if bb is None:
        return svg
    x0, y0, x1, y1 = bb
    if x0 >= LIMIT[0] and y0 >= LIMIT[0] and x1 <= LIMIT[1] and y1 <= LIMIT[1]:
        return svg
    w, h = x1 - x0, y1 - y0
    s = min(1.0, (LIMIT[1] - LIMIT[0]) / max(w, h))
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    # keep the original centre when only scaling is needed along an axis that already fits
    tx, ty = 16 - cx * s, 16 - cy * s
    head, rest = svg.split(">", 1)
    inner = rest[: rest.rindex("</svg>")]
    return f'{head}><g transform="translate({kit.fmt(tx)} {kit.fmt(ty)}) scale({kit.fmt(s) if s < 1 else 1})">{inner}</g></svg>'


def render_svg(cat, name, fn, theme):
    c = kit.Canvas(theme, f"{cat}-{name}")
    fn(c, c.t)
    return fit_to_grid(c.svg())


def png_bytes(svg, px):
    return cairosvg.svg2png(bytestring=svg.encode(), output_width=px, output_height=px)


def hex2rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def sheet(cat, theme, out, cols=8):
    load_icons()
    items = [r for r in kit.REGISTRY if cat == "all" or r[0] in cat.split(",")]
    bg = hex2rgb(kit.THEMES[theme]["bg"])
    fg = hex2rgb(kit.THEMES[theme]["ink"])
    cw, ch = 170, 128
    rows = (len(items) + cols - 1) // cols
    im = Image.new("RGB", (cols * cw, rows * ch), bg)
    dr = ImageDraw.Draw(im)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 11)
    except Exception:
        font = ImageFont.load_default()
    for i, (cat_, name, label, fn) in enumerate(items):
        svg = render_svg(cat_, name, fn, theme)
        x0, y0 = (i % cols) * cw, (i // cols) * ch
        big = Image.open(io.BytesIO(png_bytes(svg, 96))).convert("RGBA")
        im.paste(big, (x0 + 8, y0 + 6), big)
        for j, px in enumerate((32, 16)):
            s = Image.open(io.BytesIO(png_bytes(svg, px))).convert("RGBA")
            im.paste(s, (x0 + 116, y0 + 10 + j * 44), s)
        dr.text((x0 + 8, y0 + 106), name, fill=fg, font=font)
        dr.rectangle([x0, y0, x0 + cw - 1, y0 + ch - 1], outline=tuple(int(v * 0.5 + b * 0.5) for v, b in zip(fg, bg)))
    im.save(out)
    print(out, im.size, len(items), "icons")


def zoom(theme, names, out, px=160):
    load_icons()
    items = [r for r in kit.REGISTRY if r[1] in names or f"{r[0]}/{r[1]}" in names]
    bg = hex2rgb(kit.THEMES[theme]["bg"])
    cols = min(4, len(items))
    rows = (len(items) + cols - 1) // cols
    im = Image.new("RGB", (cols * (px + 10), rows * (px + 10)), bg)
    for i, (cat_, name, label, fn) in enumerate(items):
        svg = render_svg(cat_, name, fn, theme)
        big = Image.open(io.BytesIO(png_bytes(svg, px))).convert("RGBA")
        im.paste(big, ((i % cols) * (px + 10) + 5, (i // cols) * (px + 10) + 5), big)
    im.save(out)
    print(out, im.size)


def dist(root):
    load_icons()
    manifest = []
    for theme in kit.THEMES:
        for cat, name, label, fn in kit.REGISTRY:
            svg = render_svg(cat, name, fn, theme)
            d = os.path.join(root, "svg", theme, cat)
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, f"{name}.svg"), "w") as f:
                f.write(svg)
            for px in (32, 64):
                d = os.path.join(root, "png", f"{px}", theme, cat)
                os.makedirs(d, exist_ok=True)
                with open(os.path.join(d, f"{name}.png"), "wb") as f:
                    f.write(png_bytes(svg, px))
    for cat, name, label, fn in kit.REGISTRY:
        manifest.append({"id": f"{cat}/{name}", "category": cat, "name": name, "label": label})
    with open(os.path.join(root, "icons.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print("wrote", len(kit.REGISTRY), "icons x", len(kit.THEMES), "themes")


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "sheet":
        sheet(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else f"/tmp/sheet-{sys.argv[2]}-{sys.argv[3]}.png")
    elif cmd == "zoom":
        zoom(sys.argv[2], sys.argv[4:], sys.argv[3])
    elif cmd == "dist":
        dist(sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, "..", "dist"))
