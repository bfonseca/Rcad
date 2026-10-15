"""Surface features (violet) and curves."""
import math
from kit import *  # noqa: F401,F403
from kit import (icon, arc_d, SCREEN, Iso, mix, bez, bez_tan, ribbon, solid_plan, sheet_strip,
                 gopen, gclose, box, ellipse_d)
from icons.features import fitted, up_arrow, arc_plan, curved_arrow
from icons.sketch import catmull, ink_dot


def hfun(x, y):
    return 1.15 * math.sin(x * 0.34 + 0.5) + 0.8 * math.cos(y * 0.4 + 0.2)


def hgrad(h, x, y, e=0.05):
    return (h(x + e, y) - h(x - e, y)) / (2 * e), (h(x, y + e) - h(x, y - e)) / (2 * e)


def shade(c, h, x, y, dark, light):
    """brightness from the surface normal vs a light from the top-left"""
    gx, gy = hgrad(h, x, y)
    nz = 1 / math.sqrt(1 + gx * gx + gy * gy)
    nx, ny = -gx * nz, -gy * nz
    L = (-0.45, -0.45, 0.77)
    b = max(0.0, nx * L[0] + ny * L[1] + nz * L[2])
    b = max(0.0, min(1.0, (b - 0.35) / 0.55))
    return mix(dark, light, b)


def mesh(c, iso, h, x0, x1, y0, y1, n=7, dark=None, light=None, keep=None, skip=None,
         outline=True, sw=1.5, zoff=0.0, stroke=None, tint=None):
    """Height-field surface patch drawn back-to-front. keep(x,y)->colour override or None."""
    t = c.t
    dark = dark or t["sf_right"]
    light = light or t["sf_top"]
    stroke = stroke or t["ink"]
    xs = [x0 + (x1 - x0) * i / n for i in range(n + 1)]
    ys = [y0 + (y1 - y0) * j / n for j in range(n + 1)]
    cells = []
    for i in range(n):
        for j in range(n):
            cx, cy = (xs[i] + xs[i + 1]) / 2, (ys[j] + ys[j + 1]) / 2
            if skip and skip(cx, cy):
                continue
            cells.append((cx + cy, i, j))
    for _, i, j in sorted(cells):
        cx, cy = (xs[i] + xs[i + 1]) / 2, (ys[j] + ys[j + 1]) / 2
        col = shade(c, h, cx, cy, dark, light)
        if keep:
            ov = keep(cx, cy)
            if ov:
                col = ov
        q = [iso.p(xs[i], ys[j], h(xs[i], ys[j]) + zoff), iso.p(xs[i + 1], ys[j], h(xs[i + 1], ys[j]) + zoff),
             iso.p(xs[i + 1], ys[j + 1], h(xs[i + 1], ys[j + 1]) + zoff), iso.p(xs[i], ys[j + 1], h(xs[i], ys[j + 1]) + zoff)]
        c.add(poly(q, fill=col, stroke=col, sw=0.7))
    if outline:
        edge = ([(xs[i], ys[0]) for i in range(n + 1)] + [(xs[n], ys[j]) for j in range(1, n + 1)]
                + [(xs[i], ys[n]) for i in range(n - 1, -1, -1)] + [(xs[0], ys[j]) for j in range(n - 1, 0, -1)])
        c.add(poly([iso.p(x, y, h(x, y) + zoff) for x, y in edge], stroke=stroke, sw=sw))


def edge_line(iso, h, pts2, zoff=0.0):
    return [iso.p(x, y, h(x, y) + zoff) for x, y in pts2]


def flat(z0=0.0):
    return lambda x, y: z0


def tilt(a=0.0, bx=0.0, by=0.0):
    return lambda x, y: a + bx * x + by * y


# ------------------------------------------------------------------ surfaces
@icon("surfaces", "surface-extend", "Extend Surface")
def _(c, t):
    h = lambda x, y: 0.7 * math.sin(x * 0.22 + 0.6) + 0.5 * math.cos(y * 0.3)
    iso = fitted(1.0, [(0, 0, 0), (20, 12, 0), (20, 0, 0), (0, 12, 0), (0, 0, 4)], 14.5, 15.5)
    mesh(c, iso, h, 0, 11, 0, 12, 5)
    # new geometry: the extension strip past the +x edge
    ext = lambda x, y: t["hl_top"]
    mesh(c, iso, h, 11, 19, 0, 12, 3, keep=ext, outline=False)
    c.add(poly(edge_line(iso, h, [(11, 0), (19, 0), (19, 12), (11, 12)]), stroke=t["acc"], sw=1.8, dash="3 2"))
    c.add(line(iso.p(11, 0, h(11, 0)), iso.p(11, 12, h(11, 12)), t["ink"], 1.6))
    p0, p1 = iso.p(6.5, 6, h(6.5, 6) - 0.2), iso.p(19.5, 6, h(19, 6) - 0.2)
    c.add(arrow((p0[0] + 2.2, p0[1] + 1.2), (p1[0] + 2.6, p1[1] + 1.5), t["acc"], 2.4, size=5, half=3.0))


@icon("surfaces", "surface-trim", "Trim Surface")
def _(c, t):
    h = hfun
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 6)], 16, 17)
    curve = lambda y: 8.5 + 2.2 * math.sin(y * 0.55)
    cut = lambda x, y: (t["cut_top"] if x > curve(y) else None)
    mesh(c, iso, h, 0, 14, 0, 14, 8, keep=cut)
    ys = [i * 14 / 14 for i in range(15)]
    c.add(polyline(edge_line(iso, h, [(curve(y), y) for y in ys]), t["acc"], 2.4))
    cx, cy = iso.p(11.5, 7.5, h(11.5, 7.5))
    c.add(line((cx - 2.6, cy - 2.6), (cx + 2.6, cy + 2.6), t["cut"], 2.2), line((cx - 2.6, cy + 2.6), (cx + 2.6, cy - 2.6), t["cut"], 2.2))


@icon("surfaces", "surface-merge", "Merge Surfaces")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 9)], 16, 18)
    h1 = lambda x, y: 0.0
    h2 = lambda x, y: 0.0 + max(0.0, (x - 13)) * 0.0
    # patch A flat (front), patch B rising from the shared edge (back)
    mesh(c, iso, lambda x, y: 0.0, 0, 13, 6.5, 13, 4, dark=t["sf_left"], light=t["sf_top"])
    mesh(c, iso, lambda x, y: (6.5 - y) * 1.3, 0, 13, 0, 6.5, 4, dark=t["sf_right"], light=t["sf_top"])
    c.add(line(iso.p(0, 6.5, 0), iso.p(13, 6.5, 0), t["acc"], 2.4))
    for xx in (3, 6.5, 10):
        pass


@icon("surfaces", "surface-offset", "Offset Surface")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 11)], 16, 17.5)
    h = lambda x, y: 1.2 * math.sin(x * 0.45) + 0.8 * math.cos(y * 0.5)
    mesh(c, iso, h, 0, 13, 0, 13, 6, dark=t["surf"], light=t["surf_soft"], outline=False)
    c.add(poly([iso.p(0, 0, h(0, 0)), iso.p(13, 0, h(13, 0)), iso.p(13, 13, h(13, 13)), iso.p(0, 13, h(0, 13))],
               stroke=t["ink"], sw=1.2, dash=DASH, op=0.7))
    mesh(c, iso, h, 0, 13, 0, 13, 6, zoff=6.5)
    p0 = iso.p(6.5, 6.5, h(6.5, 6.5) + 0.2)
    p1 = iso.p(6.5, 6.5, h(6.5, 6.5) + 6.5)
    c.add(arrow(p0, p1, t["acc"], 2.0, size=4.2, half=2.4, two=True))


@icon("surfaces", "surface-fill", "Fill Surface")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0)], 16, 16)
    loop = [(1, 3), (9, 0.5), (14, 5), (12.5, 12.5), (4, 14), (0, 8)]
    pl = [iso.p(x, y, 0) for x, y in loop]
    c.add(poly(pl, fill=t["sf_top"], stroke=t["acc"], sw=2.2))
    c.add(poly(pl, fill=t["sf_left"], fop=0.35))
    for p in pl:
        ink_dot(c, p, 1.7)


@icon("surfaces", "surface-boundary", "Boundary Blend")
def _(c, t):
    A = [(4, 24), (4, 14), (9, 6), (15, 4)]
    B = [(17, 28), (19, 20), (25, 13), (29, 11)]
    n = 14
    ca = [bez(*A, i / n) for i in range(n + 1)]
    cb = [bez(*B, i / n) for i in range(n + 1)]
    for i in range(n):
        f = i / n
        col = mix(t["sf_top"], t["sf_right"], f * 0.9)
        c.add(poly([ca[i], ca[i + 1], cb[i + 1], cb[i]], fill=col, stroke=col, sw=0.7))
    c.add(poly(ca + cb[::-1], stroke=t["ink"], sw=1.4))
    c.add(P(f"M{pt(A[0])} C{pt(A[1])} {pt(A[2])} {pt(A[3])}", stroke=t["acc"], sw=2.3))
    c.add(P(f"M{pt(B[0])} C{pt(B[1])} {pt(B[2])} {pt(B[3])}", stroke=t["acc"], sw=2.3))


@icon("surfaces", "surface-copy", "Copy Surface")
def _(c, t):
    h = lambda x, y: 0.8 * math.sin(x * 0.32 + 0.5) + 0.6 * math.cos(y * 0.4)
    pts = [(0, 0, 0), (12, 12, 0), (12, 0, 0), (0, 12, 0)]
    back = Iso(1.0).fit(pts, 12, 12)
    front = Iso(1.0).fit(pts, 19.5, 20)
    mesh(c, back, h, 0, 12, 0, 12, 5, dark=t["surf_soft"], light=t["surf_soft"], stroke=t["mute"], sw=1.4)
    mesh(c, front, h, 0, 12, 0, 12, 5)


@icon("surfaces", "surface-intersect", "Intersect Surfaces")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (15, 15, 0), (15, 0, 0), (0, 15, 0), (7.5, 7.5, 10)], 16, 17)
    mesh(c, iso, lambda x, y: 0.0, 0, 15, 0, 15, 5, dark=t["sf_right"], light=t["sf_top"])
    # vertical sheet crossing the floor along y = 7.5
    q = [iso.p(0, 7.5, 0), iso.p(15, 7.5, 0), iso.p(15, 7.5, 10), iso.p(0, 7.5, 10)]
    c.add(poly(q, fill=t["sf_left"], stroke=t["ink"], sw=1.5, op=0.92))
    c.add(line(iso.p(0, 7.5, 0), iso.p(15, 7.5, 0), t["acc"], 2.6))


@icon("surfaces", "surface-solidify", "Solidify")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (12, 12, 0), (12, 0, 0), (0, 12, 0), (0, 0, 12)], 16, 17)
    box(c, iso, 0, 0, 0, 12, 12, 12, fills=(t["sf_top"], t["sf_left"], t["sf_right"]))
    # hidden edges show it is a closed set of surfaces
    p = iso.p
    for a, b in (((0, 0, 0), (12, 0, 0)), ((0, 0, 0), (0, 12, 0)), ((0, 0, 0), (0, 0, 12))):
        c.add(line(p(*a), p(*b), t["ink"], 1.1, dash=DASH, op=0.0))
    cx, cy = 25.5, 25.5
    c.add(circle(cx, cy, 4.6, fill=t["acc"], stroke=t["bg"], sw=1.0))
    c.add(polyline([(cx - 2.2, cy + 0.1), (cx - 0.5, cy + 1.9), (cx + 2.4, cy - 1.8)], t["white"], 1.7))


@icon("surfaces", "surface-thicken", "Thicken")
def _(c, t):
    outer = arc_plan(0, 0, 13, 0, 90, 6)
    inner = arc_plan(0, 0, 9.4, 90, 0, 6)
    plan = outer + inner
    iso = fitted(1.05, [(x, y, 0) for x, y in plan] + [(x, y, 12) for x, y in plan], 16, 16.5)
    solid_plan(c, iso, plan, 0, 12)
    # the source surface sits on the outer face (violet edge)
    c.add(polyline([iso.p(x, y, 12) for x, y in outer], t["surf"], 2.4))
    c.add(polyline([iso.p(x, y, 0) for x, y in outer], t["surf"], 2.4))


@icon("surfaces", "surface-style", "Style (Freeform) Surface")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 7)], 16, 17)
    h = lambda x, y: 3.2 * math.sin(x * 0.38) * math.cos(y * 0.38) + 0.9
    mesh(c, iso, h, 0, 14, 0, 14, 8)
    for (x, y) in [(2, 2), (12, 2), (12, 12), (2, 12)]:
        px, py = iso.p(x, y, h(x, y) + 2.2)
        sx, sy = iso.p(x, y, h(x, y))
        c.add(line((sx, sy), (px, py), t["acc"], 1.2))
        c.add(circle(px, py, 1.9, fill=t["acc"], stroke=t["bg"], sw=0.8))


@icon("surfaces", "surface-remove", "Delete Surface")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 7)], 16, 17)
    h = hfun
    hole = lambda x, y: (x - 7) ** 2 + (y - 7) ** 2 < 3.3 ** 2
    mesh(c, iso, h, 0, 14, 0, 14, 10, skip=hole)
    ring = [(7 + 3.4 * math.cos(math.radians(a)), 7 + 3.4 * math.sin(math.radians(a))) for a in range(0, 360, 20)]
    c.add(poly(edge_line(iso, h, ring), stroke=t["cut"], sw=1.6, dash="2.2 1.8"))


# ---------------------------------------------------------------------- curves
@icon("curves", "curve-points", "Curve Through Points")
def _(c, t):
    pts = [(5, 22), (11, 11), (19, 21), (27, 8)]
    c.add(P(catmull(pts), stroke=t["acc"], sw=2.3))
    for i, p in enumerate(pts):
        c.add(circle(p[0], p[1], 2.5, fill=t["datum"], stroke=t["bg"], sw=0.9))


@icon("curves", "curve-equation", "Curve From Equation")
def _(c, t):
    c.add(arrow((4, 24), (28, 24), t["ink"], 1.5, size=3.6, half=2.0))
    c.add(arrow((6, 27), (6, 4), t["ink"], 1.5, size=3.6, half=2.0))
    pts = [(6 + i * 20 / 20, 15.5 - 7 * math.sin(i / 20 * 2 * math.pi)) for i in range(21)]
    c.add(polyline(pts, t["acc"], 2.3))
    c.add(polyline([(23, 7.5), (26, 7.5)], t["mute"], 0))


@icon("curves", "curve-project", "Project Curve")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 14)], 16, 17)
    mesh(c, iso, lambda x, y: 0.0, 0, 13, 0, 13, 5, dark=t["sf_right"], light=t["sf_top"])
    n = 12
    cur = [(2.5 + 8 * i / n, 6.5 + 2.2 * math.sin(i / n * math.pi * 1.5)) for i in range(n + 1)]
    up = [iso.p(x, y, 13) for x, y in cur]
    dn = [iso.p(x, y, 0) for x, y in cur]
    # projection rays at both ends, drawn behind the curves
    for i in (0, n):
        c.add(line(up[i], dn[i], t["ink"], 1.2, dash="1.8 1.8"))
    c.add(polyline(dn, t["acc"], 2.4))
    c.add(polyline(up, t["ink"], 2.0))
@icon("curves", "curve-intersect", "Intersection Curve")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (15, 15, 0), (15, 0, 0), (0, 15, 0), (7.5, 7.5, 10)], 16, 17)
    mesh(c, iso, lambda x, y: 0.0, 0, 15, 0, 15, 5, dark=t["sf_right"], light=t["sf_top"])
    # tilted second surface cutting the floor
    q = [iso.p(0, 4, -1), iso.p(15, 11, -1), iso.p(15, 11, 10), iso.p(0, 4, 10)]
    c.add(poly(q, fill=t["sf_left"], stroke=t["ink"], sw=1.5, op=0.92))
    c.add(line(iso.p(0, 4, 0), iso.p(15, 11, 0), t["acc"], 2.8))


@icon("curves", "curve-offset", "Offset Curve")
def _(c, t):
    d1 = "M4 26 C4 10 12 6 28 6"
    d2 = "M10 26 C10 16 15 12 28 12"
    c.add(P(d1, stroke=t["ink"], sw=2.0))
    c.add(P(d2, stroke=t["acc"], sw=2.4))
    c.add(arrow((19, 6.8), (19, 11.6), t["mute"], 1.2, size=3, half=1.6))


@icon("curves", "curve-composite", "Composite Curve")
def _(c, t):
    c.add(P("M3 24 C6 12 10 10 14 14", stroke=t["acc"], sw=2.4))
    c.add(P("M14 14 C17 17 19 24 22 22", stroke=t["datum"], sw=2.4))
    c.add(P("M22 22 C24 20 26 12 29 7", stroke=t["surf"], sw=2.4))
    for p in ((14, 14), (22, 22)):
        c.add(circle(p[0], p[1], 2.1, fill=t["ink"], stroke=t["bg"], sw=0.8))


@icon("curves", "curve-extend", "Extend Curve")
def _(c, t):
    c.add(P("M3 25 C5 14 10 11 16 12", stroke=t["ink"], sw=2.3))
    c.add(P("M16 12 C20 12.5 24 10 27 5", stroke=t["acc"], sw=2.3, dash="3.4 2.4"))
    c.add(poly(head_pts((27.6, 4.2), (0.55, -0.83), 4.4, 2.5), fill=t["acc"], stroke=t["acc"], sw=0.5))
    ink_dot(c, (16, 12), 1.9)
