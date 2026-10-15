"""rcad icon kit: themes, SVG primitives, isometric helpers.

Every icon lives on a 32x32 grid. Convention:
  ink    = neutral outlines / context
  top/left/right = the three shades of solid material (light from top-left)
  acc    = sketch geometry, paths, directions, "the thing being created" (orange)
  datum  = datum features (teal)
  surf   = surface features (violet)
  cut    = material removal (red)
"""
import math

SW = 1.6          # outline stroke width for solids
GW = 2.0          # sketch-geometry stroke width
W = 32
C30 = math.sqrt(3) / 2
DASH = "2.4 2"
DASHDOT = "5 1.7 1 1.7"

THEMES = {
    "light": dict(
        ink="#243044", top="#DCEAFF", left="#A9C8F8", right="#7CA5E6",
        acc="#F26B1D", acc_soft="#FFD9BF",
        datum="#12A58C", datum_soft="#C6EFE6",
        surf="#8B6CF0", surf_soft="#DDD2FB", surf_mid="#B9A5F5",
        cut="#E5484D", cut_soft="#FCDADB",
        hole="#2E3B52", white="#FFFFFF", mute="#8A96A8", bg="#FFFFFF",
        hl_top="#FFCDA8", hl_left="#FFA66B", hl_right="#F2843A",
        cut_top="#FBC4C6", cut_left="#F59A9E", cut_right="#E5686E",
        sf_top="#D3C8FB", sf_left="#AE99F3", sf_right="#8A70E3",
    ),
    "dark": dict(
        ink="#D6DEEB", top="#7BA6F2", left="#5483D8", right="#3C63AE",
        acc="#FF8A3D", acc_soft="#6B4426",
        datum="#2DD4BF", datum_soft="#1B4F47",
        surf="#A48BFF", surf_soft="#4A3E7E", surf_mid="#7561C9",
        cut="#FF6B70", cut_soft="#6A2A2E",
        hole="#10151C", white="#FFFFFF", mute="#7C889B", bg="#1E232B",
        hl_top="#FFB27A", hl_left="#F7893A", hl_right="#C9651F",
        cut_top="#FF9EA2", cut_left="#E5575D", cut_right="#B53A40",
        sf_top="#BBA9FF", sf_left="#8E74F0", sf_right="#6B52C9",
    ),
}


# ----------------------------------------------------------------- numbers
def fmt(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("", "-0") else s


def pt(p):
    return f"{fmt(p[0])} {fmt(p[1])}"


def pts_d(pts, close=False):
    d = "M" + " L".join(pt(p) for p in pts)
    return d + ("Z" if close else "")


# --------------------------------------------------------------- primitives
def P(d, fill="none", stroke=None, sw=SW, dash=None, op=None, fop=None,
      cap="round", join="round", clip=None):
    a = [f'd="{d}"', f'fill="{fill}"']
    if fop is not None:
        a.append(f'fill-opacity="{fop}"')
    if stroke:
        a += [f'stroke="{stroke}"', f'stroke-width="{fmt(sw)}"',
              f'stroke-linecap="{cap}"', f'stroke-linejoin="{join}"']
    if dash:
        a.append(f'stroke-dasharray="{dash}"')
    if op is not None:
        a.append(f'opacity="{op}"')
    if clip:
        a.append(f'clip-path="url(#{clip})"')
    return "<path " + " ".join(a) + "/>"


def poly(pts, fill="none", stroke=None, sw=SW, close=True, **kw):
    return P(pts_d(pts, close), fill=fill, stroke=stroke, sw=sw, **kw)


def line(p0, p1, stroke, sw=SW, **kw):
    return P(f"M{pt(p0)} L{pt(p1)}", stroke=stroke, sw=sw, **kw)


def polyline(pts, stroke, sw=SW, **kw):
    return P(pts_d(pts, False), stroke=stroke, sw=sw, **kw)


def circle(cx, cy, r, fill="none", stroke=None, sw=SW, dash=None, op=None, fop=None, clip=None):
    a = [f'cx="{fmt(cx)}"', f'cy="{fmt(cy)}"', f'r="{fmt(r)}"', f'fill="{fill}"']
    if fop is not None:
        a.append(f'fill-opacity="{fop}"')
    if stroke:
        a += [f'stroke="{stroke}"', f'stroke-width="{fmt(sw)}"']
    if dash:
        a.append(f'stroke-dasharray="{dash}"')
    if op is not None:
        a.append(f'opacity="{op}"')
    if clip:
        a.append(f'clip-path="url(#{clip})"')
    return "<circle " + " ".join(a) + "/>"


def ellipse(cx, cy, rx, ry, rot=0, fill="none", stroke=None, sw=SW, dash=None, op=None, fop=None, clip=None):
    a = [f'cx="{fmt(cx)}"', f'cy="{fmt(cy)}"', f'rx="{fmt(rx)}"', f'ry="{fmt(ry)}"', f'fill="{fill}"']
    if fop is not None:
        a.append(f'fill-opacity="{fop}"')
    if stroke:
        a += [f'stroke="{stroke}"', f'stroke-width="{fmt(sw)}"']
    if dash:
        a.append(f'stroke-dasharray="{dash}"')
    if op is not None:
        a.append(f'opacity="{op}"')
    if clip:
        a.append(f'clip-path="url(#{clip})"')
    if rot:
        a.append(f'transform="rotate({fmt(rot)} {fmt(cx)} {fmt(cy)})"')
    return "<ellipse " + " ".join(a) + "/>"


def rect(x, y, w, h, r=0, fill="none", stroke=None, sw=SW, dash=None, op=None, fop=None):
    a = [f'x="{fmt(x)}"', f'y="{fmt(y)}"', f'width="{fmt(w)}"', f'height="{fmt(h)}"', f'fill="{fill}"']
    if r:
        a.append(f'rx="{fmt(r)}"')
    if fop is not None:
        a.append(f'fill-opacity="{fop}"')
    if stroke:
        a += [f'stroke="{stroke}"', f'stroke-width="{fmt(sw)}"', 'stroke-linejoin="round"']
    if dash:
        a.append(f'stroke-dasharray="{dash}"')
    if op is not None:
        a.append(f'opacity="{op}"')
    return "<rect " + " ".join(a) + "/>"


def dot(x, y, color, r=1.5):
    return circle(x, y, r, fill=color)


def pdot(x, y, color, r=1.7, ring="#FFFFFF"):
    """point marker with a thin ring so it separates from lines"""
    return circle(x, y, r, fill=color, stroke=ring, sw=0.6)


def vec(p0, p1):
    dx, dy = p1[0] - p0[0], p1[1] - p0[1]
    L = math.hypot(dx, dy) or 1
    return dx / L, dy / L


def head_pts(tip, d, size=4.2, half=2.3):
    bx, by = tip[0] - d[0] * size, tip[1] - d[1] * size
    nx, ny = -d[1], d[0]
    return [tip, (bx + nx * half, by + ny * half), (bx - nx * half, by - ny * half)]


def arrow(p0, p1, color, sw=1.7, size=4.2, half=2.3, dash=None, two=False):
    """straight arrow, filled head; two=True puts heads on both ends"""
    d = vec(p0, p1)
    base = (p1[0] - d[0] * size * 0.75, p1[1] - d[1] * size * 0.75)
    start = p0
    out = []
    if two:
        start = (p0[0] + d[0] * size * 0.75, p0[1] + d[1] * size * 0.75)
        out.append(poly(head_pts(p0, (-d[0], -d[1]), size, half), fill=color, stroke=color, sw=0.5))
    out.append(line(start, base, color, sw, dash=dash))
    out.append(poly(head_pts(p1, d, size, half), fill=color, stroke=color, sw=0.5))
    return out


def path_arrow(d, end, tan, color, sw=1.7, size=4.2, half=2.3):
    """arrowhead at `end` of an existing stroked path `d`; tan = tangent direction"""
    L = math.hypot(*tan) or 1
    u = (tan[0] / L, tan[1] / L)
    return [P(d, stroke=color, sw=sw), poly(head_pts(end, u, size, half), fill=color, stroke=color, sw=0.5)]


# ----------------------------------------------------- affine plane + arcs
class Aff:
    """x' = a*u + c*v + e ; y' = b*u + d*v + f"""

    def __init__(self, a=1, b=0, c=0, d=1, e=0, f=0):
        self.a, self.b, self.c, self.d, self.e, self.f = a, b, c, d, e, f

    def __call__(self, u, v):
        return (self.a * u + self.c * v + self.e, self.b * u + self.d * v + self.f)

    def vec(self, u, v):
        return (self.a * u + self.c * v, self.b * u + self.d * v)


SCREEN = Aff()


def arc_d(A, cu, cv, r, a0, a1, move=True, ru=None):
    """Bezier arc of a circle (radius r, or ellipse ru x r) in the local plane A.
    Angles in degrees. Returns (d, end_point, end_tangent)."""
    ru = r if ru is None else ru
    total = math.radians(a1 - a0)
    n = max(1, math.ceil(abs(total) / (math.pi / 2) - 1e-9))
    da = total / n
    k = 4.0 / 3.0 * math.tan(da / 4.0)
    a = math.radians(a0)

    def lp(ang):
        return (cu + ru * math.cos(ang), cv + r * math.sin(ang))

    def ld(ang):  # derivative direction
        return (-ru * math.sin(ang), r * math.cos(ang))

    p0 = lp(a)
    d = (f"M{pt(A(*p0))} " if move else "")
    sgn = 1 if da > 0 else -1
    for i in range(n):
        a1_ = a + da
        q0, q3 = lp(a), lp(a1_)
        t0, t3 = ld(a), ld(a1_)
        # k is signed (negative for da < 0), so handles already point along travel
        q1 = (q0[0] + k * t0[0], q0[1] + k * t0[1])
        q2 = (q3[0] - k * t3[0], q3[1] - k * t3[1])
        d += f"C{pt(A(*q1))} {pt(A(*q2))} {pt(A(*q3))} "
        a = a1_
    t3 = ld(a)
    et = A.vec(t3[0] * sgn, t3[1] * sgn)
    return d.strip(), A(*lp(a)), et


def ellipse_pts(A, cu, cv, r, a0, a1, n=24, ru=None):
    ru = r if ru is None else ru
    out = []
    for i in range(n + 1):
        a = math.radians(a0 + (a1 - a0) * i / n)
        out.append(A(cu + ru * math.cos(a), cv + r * math.sin(a)))
    return out


# ------------------------------------------------------------------- isometric
class Iso:
    """k = pixels per model unit. x runs lower-right, y lower-left, z up.
    Visible faces: top (+z), 'front' = +y (left on screen), 'right' = +x."""

    def __init__(self, k=1.0, ox=16.0, oy=16.0):
        self.k, self.ox, self.oy = k, ox, oy

    def v(self, x, y, z):
        return ((x - y) * C30 * self.k, (x + y) * 0.5 * self.k - z * self.k)

    def p(self, x, y, z=0):
        vx, vy = self.v(x, y, z)
        return (self.ox + vx, self.oy + vy)

    def fit(self, pts3, cx=16.0, cy=16.0):
        xs, ys = [], []
        for q in pts3:
            vx, vy = self.v(*q)
            xs.append(vx)
            ys.append(vy)
        self.ox = cx - (min(xs) + max(xs)) / 2
        self.oy = cy - (min(ys) + max(ys)) / 2
        return self

    def plane(self, origin, U, V):
        o = self.p(*origin)
        bu = self.v(*U)
        bv = self.v(*V)
        return Aff(bu[0], bu[1], bv[0], bv[1], o[0], o[1])

    def top(self, z=0):
        return self.plane((0, 0, z), (1, 0, 0), (0, 1, 0))

    def frontp(self, y=0):  # plane y = const (U = x, V = z)
        return self.plane((0, y, 0), (1, 0, 0), (0, 0, 1))

    def sidep(self, x=0):  # plane x = const (U = y, V = z)
        return self.plane((x, 0, 0), (0, 1, 0), (0, 0, 1))

    def ell_r(self, r):
        return (r * math.sqrt(2) * C30 * self.k, r * math.sqrt(2) * 0.5 * self.k)


def box(c, iso, x0, y0, z0, x1, y1, z1, fills=None, stroke=None, sw=SW, op=None):
    t = c.t
    ft, fl, fr = fills or (t["top"], t["left"], t["right"])
    stroke = stroke or t["ink"]
    p = iso.p
    top = [p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)]
    lf = [p(x0, y1, z0), p(x1, y1, z0), p(x1, y1, z1), p(x0, y1, z1)]
    rt = [p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)]
    c.add(poly(lf, fill=fl, stroke=stroke, sw=sw, op=op))
    c.add(poly(rt, fill=fr, stroke=stroke, sw=sw, op=op))
    c.add(poly(top, fill=ft, stroke=stroke, sw=sw, op=op))


def box_ghost(c, iso, x0, y0, z0, x1, y1, z1, color=None, sw=1.3, dash=DASH, op=0.9, hidden=False):
    color = color or c.t["ink"]
    p = iso.p
    hexa = [p(x0, y0, z1), p(x1, y0, z1), p(x1, y0, z0), p(x1, y1, z0), p(x0, y1, z0), p(x0, y1, z1)]
    c.add(poly(hexa, stroke=color, sw=sw, dash=dash, op=op))
    n = p(x1, y1, z1)
    for q in (p(x0, y1, z1), p(x1, y0, z1), p(x1, y1, z0)):
        c.add(line(n, q, color, sw, dash=dash, op=op))
    if hidden:
        b = p(x0, y0, z0)
        for q in (p(x0, y1, z0), p(x1, y0, z0), p(x0, y0, z1)):
            c.add(line(b, q, color, sw * 0.8, dash=dash, op=op * 0.5))


def cyl(c, iso, cx, cy, z0, z1, r, fills=None, stroke=None, sw=SW, top_fill=None, op=None):
    t = c.t
    ft, fl, fr = fills or (t["top"], t["left"], t["right"])
    stroke = stroke or t["ink"]
    rx, ry = iso.ell_r(r)
    xt, yt = iso.p(cx, cy, z1)
    xb, yb = iso.p(cx, cy, z0)
    body = (f"M{fmt(xt - rx)} {fmt(yt)} L{fmt(xb - rx)} {fmt(yb)} "
            f"A{fmt(rx)} {fmt(ry)} 0 0 0 {fmt(xb + rx)} {fmt(yb)} L{fmt(xt + rx)} {fmt(yt)} Z")
    c.add(P(body, fill=fl, stroke=stroke, sw=sw, op=op))
    # right-hand shading band
    band = (f"M{fmt(xt + rx * 0.25)} {fmt(yt + ry * 0.97)} L{fmt(xb + rx * 0.25)} {fmt(yb + ry * 0.97)} "
            f"A{fmt(rx)} {fmt(ry)} 0 0 0 {fmt(xb + rx)} {fmt(yb)} L{fmt(xt + rx)} {fmt(yt)} "
            f"A{fmt(rx)} {fmt(ry)} 0 0 1 {fmt(xt + rx * 0.25)} {fmt(yt + ry * 0.97)} Z")
    c.add(P(band, fill=fr, op=op))
    c.add(ellipse(xt, yt, rx, ry, fill=top_fill or ft, stroke=stroke, sw=sw, op=op))


def tube(c, pts, width, fill, stroke, sw=SW, hi=None):
    """thick stroked path with outline (two-pass stroke)"""
    d = pts if isinstance(pts, str) else pts_d(pts)
    c.add(P(d, stroke=stroke, sw=width + 2 * sw, cap="butt"))
    c.add(P(d, stroke=fill, sw=width, cap="butt"))
    if hi:
        c.add(P(d, stroke=hi, sw=max(1.0, width * 0.28), cap="butt", op=0.9))


# ------------------------------------------------------------------- canvas
class Canvas:
    def __init__(self, theme_name, name):
        self.tn = theme_name
        self.t = THEMES[theme_name]
        self.name = name
        self.items = []
        self.defs = []
        self._n = 0

    def add(self, *items):
        for it in items:
            if isinstance(it, (list, tuple)):
                self.add(*it)
            else:
                self.items.append(it)

    def clip(self, d):
        self._n += 1
        cid = f"{self.name}-c{self._n}"
        self.defs.append(f'<clipPath id="{cid}"><path d="{d}"/></clipPath>')
        return cid

    def svg(self, size=32):
        body = "".join(self.items)
        defs = f"<defs>{''.join(self.defs)}</defs>" if self.defs else ""
        return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
                f'viewBox="0 0 32 32" fill="none">{defs}{body}</svg>')


# ------------------------------------------------------------------ registry
REGISTRY = []   # (category, name, label, fn)


def icon(cat, name, label=None):
    def deco(fn):
        REGISTRY.append((cat, name, label or name.replace("-", " ").title(), fn))
        return fn
    return deco


# ------------------------------------------------------------------- helpers
def mix(c1, c2, f):
    """blend two #rrggbb colours; f = share of c2"""
    a = [int(c1[i:i + 2], 16) for i in (1, 3, 5)]
    b = [int(c2[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * f):02X}" for x, y in zip(a, b))


def bez(p0, p1, p2, p3, t):
    u = 1 - t
    return (u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0],
            u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1])


def bez_tan(p0, p1, p2, p3, t):
    u = 1 - t
    dx = 3 * u * u * (p1[0] - p0[0]) + 6 * u * t * (p2[0] - p1[0]) + 3 * t * t * (p3[0] - p2[0])
    dy = 3 * u * u * (p1[1] - p0[1]) + 6 * u * t * (p2[1] - p1[1]) + 3 * t * t * (p3[1] - p2[1])
    L = math.hypot(dx, dy) or 1
    return (dx / L, dy / L)


def ellipse_d(x, y, rx, ry):
    return (f"M{fmt(x - rx)} {fmt(y)} A{fmt(rx)} {fmt(ry)} 0 1 0 {fmt(x + rx)} {fmt(y)} "
            f"A{fmt(rx)} {fmt(ry)} 0 1 0 {fmt(x - rx)} {fmt(y)} Z")


def hole_at(c, iso, cx, cy, z, r, depth, wall, floor, stroke=None, sw=SW):
    """circular recess seen from above: rim ellipse (wall colour) + floor ellipse shifted down, clipped to rim"""
    stroke = stroke or c.t["ink"]
    rx, ry = iso.ell_r(r)
    x, y = iso.p(cx, cy, z)
    cid = c.clip(ellipse_d(x, y, rx, ry))
    c.add(ellipse(x, y, rx, ry, fill=wall))
    c.add(ellipse(x, y + depth * iso.k, rx, ry, fill=floor, clip=cid))
    c.add(ellipse(x, y, rx, ry, stroke=stroke, sw=sw))


def prism(c, iso, plan, z0, z1, fills=None, stroke=None, sw=SW, op=None, top_extra=None):
    """vertical prism over an axis-aligned plan polygon (list of (x, y))"""
    t = c.t
    ft, fl, fr = fills or (t["top"], t["left"], t["right"])
    stroke = stroke or t["ink"]
    n = len(plan)
    area = sum(plan[i][0] * plan[(i + 1) % n][1] - plan[(i + 1) % n][0] * plan[i][1] for i in range(n))
    sgn = 1 if area > 0 else -1
    sides = []
    for i in range(n):
        a, b = plan[i], plan[(i + 1) % n]
        dx, dy = b[0] - a[0], b[1] - a[1]
        nx, ny = (dy * sgn, -dx * sgn)
        if nx > 0 and abs(nx) >= abs(ny):
            kind = "r"
        elif ny > 0 and abs(ny) > abs(nx):
            kind = "l"
        else:
            continue
        sides.append(((a[0] + b[0] + a[1] + b[1]) / 2, a, b, kind))
    for _, a, b, kind in sorted(sides, key=lambda s: s[0]):
        quad = [iso.p(a[0], a[1], z0), iso.p(b[0], b[1], z0), iso.p(b[0], b[1], z1), iso.p(a[0], a[1], z1)]
        c.add(poly(quad, fill=fr if kind == "r" else fl, stroke=stroke, sw=sw, op=op))
    c.add(poly([iso.p(x, y, z1) for x, y in plan], fill=ft, stroke=stroke, sw=sw, op=op))


def ribbon(pts, widths):
    """polygon around a centre polyline with per-point widths -> (polygon points)"""
    left, right = [], []
    n = len(pts)
    for i, p in enumerate(pts):
        a = pts[max(0, i - 1)]
        b = pts[min(n - 1, i + 1)]
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dy) or 1
        nx, ny = -dy / L, dx / L
        w = widths[i] / 2
        left.append((p[0] + nx * w, p[1] + ny * w))
        right.append((p[0] - nx * w, p[1] - ny * w))
    return left + right[::-1]


def evenodd(path_str):
    return path_str.replace("<path ", '<path fill-rule="evenodd" ', 1)


def solid_plan(c, iso, plan, z0, z1, tint=(), tint_cols=None, top_fill=None, stroke=None,
               sw=SW, sharp_deg=25, top_stroke=None):
    """Vertical extrusion of any plan polygon (can be concave / rounded, given as many points).
    Visible side strips are shaded between the left colour (normal +y) and right colour (normal +x).
    `tint` = edge indices drawn with tint_cols=(left, right) instead of the normal shades."""
    t = c.t
    stroke = stroke or t["ink"]
    n = len(plan)
    area = sum(plan[i][0] * plan[(i + 1) % n][1] - plan[(i + 1) % n][0] * plan[i][1] for i in range(n))
    sgn = 1 if area > 0 else -1
    edges = []
    for i in range(n):
        a, b = plan[i], plan[(i + 1) % n]
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dy)
        if L < 1e-9:
            continue
        nx, ny = dy * sgn / L, -dx * sgn / L
        vis = (nx + ny) > 0.02
        ang = math.degrees(math.atan2(ny, nx))
        edges.append(dict(i=i, a=a, b=b, nx=nx, ny=ny, vis=vis, ang=ang, mid=(a[0] + b[0] + a[1] + b[1]) / 2))
    cols = {}
    for e in edges:
        f = max(0.0, min(1.0, (90 - e["ang"]) / 90))
        lc, rc = (tint_cols if (tint_cols and e["i"] in tint) else (t["left"], t["right"]))
        cols[e["i"]] = mix(lc, rc, f)
    for e in sorted([e for e in edges if e["vis"]], key=lambda e: e["mid"]):
        a, b = e["a"], e["b"]
        quad = [iso.p(a[0], a[1], z0), iso.p(b[0], b[1], z0), iso.p(b[0], b[1], z1), iso.p(a[0], a[1], z1)]
        col = cols[e["i"]]
        c.add(poly(quad, fill=col, stroke=col, sw=0.6))
    # outlines: bottom edges + sharp / silhouette vertical edges
    for e in edges:
        if e["vis"]:
            c.add(line(iso.p(e["a"][0], e["a"][1], z0), iso.p(e["b"][0], e["b"][1], z0), stroke, sw))
    byi = {e["i"]: e for e in edges}
    for j in range(n):
        prev = byi.get((j - 1) % n)
        cur = byi.get(j)
        if prev is None or cur is None:
            continue
        v = plan[j]
        if prev["vis"] != cur["vis"]:
            draw = True
        elif prev["vis"] and cur["vis"]:
            d = abs(prev["ang"] - cur["ang"])
            d = min(d, 360 - d)
            draw = d > sharp_deg
        else:
            draw = False
        if draw:
            c.add(line(iso.p(v[0], v[1], z0), iso.p(v[0], v[1], z1), stroke, sw))
    c.add(poly([iso.p(x, y, z1) for x, y in plan], fill=top_fill or t["top"], stroke=top_stroke or stroke, sw=sw))


def pocket(c, iso, W, D, H, cav, fl, floor_fill, wall_cols=None, stroke=None, sw=SW,
           top_fill=None, rim_stroke=None):
    """Box (0..W, 0..D, 0..H) with a rectangular cavity cav=(xa,ya,xb,yb) opening in the top face,
    floor at height fl."""
    t = c.t
    stroke = stroke or t["ink"]
    xa, ya, xb, yb = cav
    p = iso.p
    c.add(poly([p(0, D, 0), p(W, D, 0), p(W, D, H), p(0, D, H)], fill=t["left"], stroke=stroke, sw=sw))
    c.add(poly([p(W, 0, 0), p(W, D, 0), p(W, D, H), p(W, 0, H)], fill=t["right"], stroke=stroke, sw=sw))
    opening = [p(xa, ya, H), p(xb, ya, H), p(xb, yb, H), p(xa, yb, H)]
    cid = c.clip(pts_d(opening, True))
    lc, rc = wall_cols or (mix(t["left"], t["hole"], 0.35), mix(t["right"], t["hole"], 0.35))
    c.add(poly([p(xa, ya, fl), p(xb, ya, fl), p(xb, yb, fl), p(xa, yb, fl)], fill=floor_fill, clip=cid))
    c.add(poly([p(xa, ya, fl), p(xb, ya, fl), p(xb, ya, H), p(xa, ya, H)], fill=lc, clip=cid))
    c.add(poly([p(xa, ya, fl), p(xa, yb, fl), p(xa, yb, H), p(xa, ya, H)], fill=rc, clip=cid))
    c.add(polyline([p(xb, ya, fl), p(xa, ya, fl), p(xa, yb, fl)], stroke, sw * 0.75, clip=cid))
    outer = [p(0, 0, H), p(W, 0, H), p(W, D, H), p(0, D, H)]
    rim = pts_d(outer, True) + " " + pts_d(opening, True)
    c.add(evenodd(P(rim, fill=top_fill or t["top"], stroke=rim_stroke or stroke, sw=sw)))


def gopen(c, clip_id):
    c.add(f'<g clip-path="url(#{clip_id})">')


def gclose(c):
    c.add("</g>")


def trench(c, outline_pts, depth, wall, floor, stroke=None, sw=1.4, floor_stroke=None):
    """Recessed region: polygon outline_pts on a face; floor = same shape shifted down by `depth` px,
    clipped to the outline (so the far wall shows as a crescent)."""
    stroke = stroke or c.t["ink"]
    d = pts_d(outline_pts, True)
    cid = c.clip(d)
    c.add(P(d, fill=wall))
    c.add(f'<g clip-path="url(#{cid})">' + P(pts_d([(x, y + depth) for x, y in outline_pts], True), fill=floor) + "</g>")
    c.add(P(d, stroke=stroke, sw=sw))


def sheet_strip(c, iso, curve, z0, z1, cols, stroke=None, sw=1.5, edge_col=None):
    """Open vertical sheet over a plan curve (list of (x, y)), shaded per segment between cols=(left, right, top)."""
    t = c.t
    stroke = stroke or t["ink"]
    lc, rc, tc = cols
    quads = []
    for i in range(len(curve) - 1):
        a, b = curve[i], curve[i + 1]
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dy) or 1
        nx, ny = dy / L, -dx / L
        if nx + ny < 0:
            nx, ny = -nx, -ny
        ang = math.degrees(math.atan2(ny, nx))
        f = max(0.0, min(1.0, (90 - ang) / 90))
        col = mix(lc, rc, f)
        q = [iso.p(a[0], a[1], z0), iso.p(b[0], b[1], z0), iso.p(b[0], b[1], z1), iso.p(a[0], a[1], z1)]
        quads.append((col, q))
    for col, q in quads:
        c.add(poly(q, fill=col, stroke=col, sw=0.6))
    bot = [iso.p(x, y, z0) for x, y in curve]
    top = [iso.p(x, y, z1) for x, y in curve]
    c.add(poly(bot + top[::-1], stroke=stroke, sw=sw))
    return bot, top
