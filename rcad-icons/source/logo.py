"""rcad logo: extruded isometric 'r' on a datum plane + wordmark (outlined from Inter Display, SIL OFL)."""
import math
import os
import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from kit import Iso, fmt  # noqa: E402

FONT = "/usr/share/fonts/opentype/inter/InterDisplay-Bold.otf"
C30 = math.sqrt(3) / 2

BRAND = dict(
    blue_hi="#A9CBFF", blue="#5B93F5", blue_lo="#2F63D6", side="#1D3F9E", side_hi="#2A56BF",
    orange="#FF7A22", orange_deep="#E85A0C", navy="#16233A", navy_hi="#22365A", ink="#16233A", paper="#F4F8FF",
)

# letter plan (u right, v up), chunky 'r' = stem + shoulder
STROKE = 4.7
STEM = "M0 0 L0 15.8"
ARM = "M0 8.2 C0 13.2 3.6 15.8 9.6 15.8"


def text_path(text, size, tracking=0.0):
    f = TTFont(FONT)
    gs = f.getGlyphSet()
    cmap = f.getBestCmap()
    s = size / f["head"].unitsPerEm
    x, out = 0.0, []
    for ch in text:
        g = gs[cmap[ord(ch)]]
        pen = SVGPathPen(gs, ntos=lambda v: f"{v:.2f}".rstrip("0").rstrip("."))
        g.draw(TransformPen(pen, (s, 0, 0, -s, x, 0)))
        out.append(pen.getCommands())
        x += g.width * s + tracking
    return " ".join(out), x - tracking


def r_mark(k=7.2, depth=7.0, cx=128, cy=128, uid="m", plane=True, dot=True, palette=BRAND, plane_op=0.46):
    """returns (defs, body) of the isometric extruded r, centred on cx,cy"""
    P_ = palette
    iso = Iso(k, 0, 0)
    Yp = 0.0
    A = iso.frontp(Yp)  # u = x, v = z (up)
    mat = f"matrix({fmt(A.a)} {fmt(A.b)} {fmt(A.c)} {fmt(A.d)} {fmt(A.e)} {fmt(A.f)})"
    sx, sy = C30 * k, -0.5 * k  # screen shift per unit of depth (towards the back)

    # rough bounding box for centring: letter + extrusion + floor plane
    pts = []
    for u, v in ((-STROKE / 2, -STROKE / 2), (9.6 + STROKE / 2, 15.8 + STROKE / 2), (-STROKE / 2, 15.8 + STROKE / 2), (9.6 + STROKE / 2, -STROKE / 2)):
        x, y = A(u, v)
        pts += [(x, y), (x + sx * depth, y + sy * depth)]
    fl = [iso.p(-3.5, 3.2, 0), iso.p(15.5, 3.2, 0), iso.p(15.5, -8.8, 0), iso.p(-3.5, -8.8, 0)] if plane else []
    pts += fl
    minx, maxx = min(p[0] for p in pts), max(p[0] for p in pts)
    miny, maxy = min(p[1] for p in pts), max(p[1] for p in pts)
    tx, ty = cx - (minx + maxx) / 2, cy - (miny + maxy) / 2

    gid = f"{uid}-g"
    defs = (f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="20">'
            f'<stop offset="0" stop-color="{P_["blue_lo"]}"/><stop offset="1" stop-color="{P_["blue_hi"]}"/></linearGradient>'
            f'<linearGradient id="{uid}-s" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="20">'
            f'<stop offset="0" stop-color="{P_["side"]}"/><stop offset="1" stop-color="{P_["side_hi"]}"/></linearGradient>')
    body = [f'<g transform="translate({fmt(tx)} {fmt(ty)})">']
    if plane:
        poly = " ".join(f"{fmt(x)},{fmt(y)}" for x, y in fl)
        body.append(f'<polygon points="{poly}" fill="{P_["orange"]}" fill-opacity="{plane_op}" stroke="{P_["orange"]}" '
                    f'stroke-width="{fmt(k * 0.36)}" stroke-linejoin="round"/>')
        # centre ticks (datum plane feel)
        a, b = iso.p(6, 3.2, 0), iso.p(6, -8.8, 0)
        body.append(f'<line x1="{fmt(a[0])}" y1="{fmt(a[1])}" x2="{fmt(b[0])}" y2="{fmt(b[1])}" stroke="{P_["orange"]}" '
                    f'stroke-width="{fmt(k * 0.16)}" stroke-dasharray="{fmt(k * .5)} {fmt(k * .5)}" opacity=".7"/>')
    steps = 28
    for i in range(steps, 0, -1):
        s = depth * i / steps
        body.append(f'<g transform="translate({fmt(sx * s)} {fmt(sy * s)})"><g transform="{mat}" fill="none" stroke="url(#{uid}-s)" '
                    f'stroke-width="{STROKE}" stroke-linecap="round" stroke-linejoin="round"><path d="{STEM}"/><path d="{ARM}"/></g></g>')
    body.append(f'<g transform="{mat}" fill="none" stroke="url(#{gid})" stroke-width="{STROKE}" stroke-linecap="round" '
                f'stroke-linejoin="round"><path d="{STEM}"/><path d="{ARM}"/></g>')
    # soft highlight along the front edge
    body.append(f'<g transform="{mat}" fill="none" stroke="#FFFFFF" stroke-opacity=".38" stroke-width="{STROKE * 0.16}" '
                f'stroke-linecap="round"><path d="M-1.2 0.8 L-1.2 14.6"/><path d="M-1.0 9.4 C-1.0 12.6 1.4 14.6 5.4 14.6"/></g>')
    if dot:
        ex, ey = A(9.6, 15.8)
        body.append(f'<circle cx="{fmt(ex + 1.6 * k)}" cy="{fmt(ey + 0.1 * k)}" r="{fmt(k * 0.95)}" fill="{P_["orange"]}" '
                    f'stroke="#FFFFFF" stroke-width="{fmt(k * 0.22)}"/>')
    body.append("</g>")
    return defs, "".join(body)


def tile(size=256, r=0.225, palette=BRAND, uid="t"):
    P_ = palette
    defs = (f'<linearGradient id="{uid}-bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{P_["navy_hi"]}"/>'
            f'<stop offset="1" stop-color="{P_["navy"]}"/></linearGradient>')
    body = f'<rect width="{size}" height="{size}" rx="{fmt(size * r)}" fill="url(#{uid}-bg)"/>'
    return defs, body


def svg_wrap(w, h, defs, body, title="rcad"):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {fmt(w)} {fmt(h)}" width="{fmt(w)}" height="{fmt(h)}" role="img" '
            f'aria-label="{title}"><title>{title}</title><defs>{defs}</defs>{body}</svg>')


def app_icon(size=256):
    d1, b1 = tile(256)
    d2, b2 = r_mark(k=6.7, depth=5.6, cx=128, cy=129, uid="ai")
    return svg_wrap(256, 256, d1 + d2, b1 + b2, "rcad app icon")


def mark_only():
    d, b = r_mark(k=7.6, depth=6.0, cx=128, cy=128, uid="mk")
    return svg_wrap(256, 256, d, b, "rcad mark")


def glyph_top(ch):
    from fontTools.pens.boundsPen import BoundsPen
    f = TTFont(FONT)
    gs = f.getGlyphSet()
    bp = BoundsPen(gs)
    gs[f.getBestCmap()[ord(ch)]].draw(bp)
    return bp.bounds[3] / f["head"].unitsPerEm


def lockup(theme="light"):
    ink = "#16233A" if theme == "light" else "#F2F6FD"
    size = 104
    d_text, w = text_path("rcad", size, tracking=-size * 0.014)
    asc = glyph_top("d") * size
    dm, bm = r_mark(k=4.15, depth=3.9, cx=64, cy=64, uid=f"lk{theme}", plane_op=0.40 if theme == "light" else 0.62)
    baseline = 64 + asc / 2
    x0 = 128 + 14
    text = f'<g transform="translate({fmt(x0)} {fmt(baseline)})"><path d="{d_text}" fill="{ink}"/></g>'
    return svg_wrap(x0 + w + 6, 128, dm, bm + text, "rcad")


def stacked(theme="light"):
    ink = "#16233A" if theme == "light" else "#F2F6FD"
    size = 70
    d_text, w = text_path("rcad", size, tracking=-size * 0.014)
    dm, bm = r_mark(k=4.9, depth=4.6, cx=100, cy=78, uid=f"st{theme}", plane_op=0.40 if theme == "light" else 0.62)
    text = f'<g transform="translate({fmt(100 - w / 2)} 208)"><path d="{d_text}" fill="{ink}"/></g>'
    return svg_wrap(200, 224, dm, bm + text, "rcad")


def wordmark_svg(theme="light"):
    ink = "#16233A" if theme == "light" else "#F2F6FD"
    size = 104
    d_text, w = text_path("rcad", size, tracking=-size * 0.014)
    asc = glyph_top("d") * size
    return svg_wrap(w + 8, asc + 24, "", f'<g transform="translate(4 {fmt(asc + 8)})"><path d="{d_text}" fill="{ink}"/></g>', "rcad")


def export_all(root):
    import cairosvg
    from PIL import Image
    os.makedirs(os.path.join(root, "png"), exist_ok=True)
    files = {
        "rcad-app-icon.svg": app_icon(), "rcad-mark.svg": mark_only(),
        "rcad-logo-horizontal-light.svg": lockup("light"), "rcad-logo-horizontal-dark.svg": lockup("dark"),
        "rcad-logo-stacked-light.svg": stacked("light"), "rcad-logo-stacked-dark.svg": stacked("dark"),
        "rcad-wordmark-light.svg": wordmark_svg("light"), "rcad-wordmark-dark.svg": wordmark_svg("dark"),
    }
    for name, svg in files.items():
        open(os.path.join(root, name), "w").write(svg)
    for px in (16, 32, 48, 64, 128, 180, 256, 512, 1024):
        cairosvg.svg2png(bytestring=files["rcad-app-icon.svg"].encode(), write_to=os.path.join(root, "png", f"rcad-app-icon-{px}.png"),
                         output_width=px, output_height=px)
    cairosvg.svg2png(bytestring=files["rcad-mark.svg"].encode(), write_to=os.path.join(root, "png", "rcad-mark-512.png"),
                     output_width=512, output_height=512)
    for n in files:
        if n.startswith("rcad-logo") or n.startswith("rcad-wordmark"):
            cairosvg.svg2png(bytestring=files[n].encode(), write_to=os.path.join(root, "png", n.replace(".svg", "-h256.png")), output_height=256)
    # multi-size Windows icon + favicon
    sizes = [16, 32, 48, 64, 128, 256]
    imgs = [Image.open(os.path.join(root, "png", f"rcad-app-icon-{p}.png")).convert("RGBA") for p in sizes]
    imgs[-1].save(os.path.join(root, "rcad.ico"), format="ICO", sizes=[(p, p) for p in sizes], append_images=imgs[:-1])
    imgs[1].save(os.path.join(root, "favicon.ico"), format="ICO", sizes=[(16, 16), (32, 32), (48, 48)])
    print("logo files:", len(files), "svg +", len(os.listdir(os.path.join(root, "png"))), "png")


if __name__ == "__main__" and len(sys.argv) > 1 and sys.argv[1] == "export":
    export_all(sys.argv[2])
    raise SystemExit


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "."
    open(os.path.join(out, "app-icon.svg"), "w").write(app_icon())
    open(os.path.join(out, "mark.svg"), "w").write(mark_only())
    open(os.path.join(out, "lockup-light.svg"), "w").write(lockup("light"))
    open(os.path.join(out, "lockup-dark.svg"), "w").write(lockup("dark"))
    open(os.path.join(out, "stacked-light.svg"), "w").write(stacked("light"))
    open(os.path.join(out, "stacked-dark.svg"), "w").write(stacked("dark"))
    open(os.path.join(out, "wordmark-light.svg"), "w").write(wordmark_svg("light"))
    open(os.path.join(out, "wordmark-dark.svg"), "w").write(wordmark_svg("dark"))
