"""Solid features: extrude / revolve / sweep families (+ cut variants)."""
import math
from kit import *  # noqa: F401,F403
from kit import (icon, arc_d, SCREEN, Iso, Aff, box, cyl, mix, bez, bez_tan, ellipse_d,
                 hole_at, prism, ribbon, solid_plan, pocket, evenodd, gopen, gclose, trench, sheet_strip)


def fitted(k, pts3, cx=16.0, cy=16.0):
    return Iso(k).fit(pts3, cx, cy)


def up_arrow(c, iso, x, y, z0, z1, col=None, sw=2.0):
    col = col or c.t["acc"]
    c.add(arrow(iso.p(x, y, z0), iso.p(x, y, z1), col, sw, size=4.6, half=2.7))


def dn_arrow(c, iso, x, y, z0, z1, col=None, sw=2.0):
    col = col or c.t["cut"]
    c.add(arrow(iso.p(x, y, z0), iso.p(x, y, z1), col, sw, size=4.6, half=2.7))


def profile_on_top(c, iso, inset, W, D, H, col=None, fill=None):
    t = c.t
    col = col or t["acc"]
    q = [iso.p(inset, inset, H), iso.p(W - inset, inset, H), iso.p(W - inset, D - inset, H), iso.p(inset, D - inset, H)]
    c.add(poly(q, fill=fill or t["acc_soft"], fop=0.75, stroke=col, sw=1.7))


def curved_arrow(c, cx, cy, rx, ry, a0, a1, col, sw=1.9, size=4.4, half=2.6):
    d, e, tn = arc_d(SCREEN, cx, cy, ry, a0, a1, ru=rx)
    c.add(path_arrow(d, e, tn, col, sw, size, half))


# ------------------------------------------------------------------ extrude
@icon("features", "extrude", "Extrude")
def _(c, t):
    W = D = 12
    H = 8
    iso = fitted(1.05, [(0, 0, 0), (W, 0, 0), (W, D, 0), (0, D, 0), (W / 2, D / 2, H + 9)], 16, 16.5)
    box(c, iso, 0, 0, 0, W, D, H)
    profile_on_top(c, iso, 2, W, D, H)
    up_arrow(c, iso, W / 2, D / 2, H, H + 9)


@icon("features", "extrude-cut", "Extrude Cut")
def _(c, t):
    W, D, H = 14, 14, 10
    iso = fitted(1.05, [(0, 0, 0), (W, D, 0), (W, 0, 0), (0, D, 0), (0, 0, H)], 16, 17)
    pocket(c, iso, W, D, H, (3.5, 3.5, 10.5, 10.5), 4.5, t["cut_top"], wall_cols=(t["cut_left"], t["cut_right"]),
           rim_stroke=t["ink"])
    dn_arrow(c, iso, 7, 7, H + 6.5, H - 1.5)


@icon("features", "extrude-thin", "Extrude (Thin)")
def _(c, t):
    W, D, H = 14, 14, 9
    iso = fitted(1.05, [(0, 0, 0), (W, D, 0), (W, 0, 0), (0, D, 0), (W / 2, D / 2, H + 8)], 16, 17)
    p = iso.p
    c.add(poly([p(0, D, 0), p(W, D, 0), p(W, D, H), p(0, D, H)], fill=t["left"], stroke=t["ink"], sw=SW))
    c.add(poly([p(W, 0, 0), p(W, D, 0), p(W, D, H), p(W, 0, H)], fill=t["right"], stroke=t["ink"], sw=SW))
    w = 2.6
    opening = [p(w, w, H), p(W - w, w, H), p(W - w, D - w, H), p(w, D - w, H)]
    cid = c.clip(pts_d(opening, True))
    gopen(c, cid)
    c.add(poly([p(w, w, 0), p(W - w, w, 0), p(W - w, w, H), p(w, w, H)], fill=mix(t["left"], t["hole"], 0.3)))
    c.add(poly([p(w, w, 0), p(w, D - w, 0), p(w, D - w, H), p(w, w, H)], fill=mix(t["right"], t["hole"], 0.3)))
    c.add(polyline([p(W - w, w, 0), p(w, w, 0), p(w, D - w, 0)], t["ink"], 1.1))
    gclose(c)
    outer = [p(0, 0, H), p(W, 0, H), p(W, D, H), p(0, D, H)]
    c.add(evenodd(P(pts_d(outer, True) + " " + pts_d(opening, True), fill=t["acc_soft"], stroke=t["ink"], sw=SW)))
    up_arrow(c, iso, W / 2, D / 2, H - 1, H + 8)


@icon("features", "extrude-surface", "Extrude Surface")
def _(c, t):
    H = 11
    curve = [(13 * math.cos(math.radians(a)), 13 * math.sin(math.radians(a))) for a in range(0, 91, 6)]
    pts3 = [(x, y, 0) for x, y in curve] + [(x, y, H + 4) for x, y in curve]
    iso = fitted(1.12, pts3, 16, 16.5)
    bot, top = sheet_strip(c, iso, curve, 0, H, (t["sf_left"], t["sf_right"], t["sf_top"]))
    c.add(polyline(bot, t["acc"], 2.2))
    mx, my = curve[len(curve) // 2]
    up_arrow(c, iso, mx, my, H, H + 7)


# ------------------------------------------------------------------ revolve
VASE = [(5.6, 0), (6.6, 2.2), (6.4, 4.6), (5.0, 7.4), (3.9, 10.2), (4.0, 12.6), (4.8, 14.2)]


def smooth_profile(prof, n=5):
    out = []
    for i in range(len(prof) - 1):
        p0 = prof[i - 1] if i > 0 else prof[i]
        p1, p2 = prof[i], prof[i + 1]
        p3 = prof[i + 2] if i + 2 < len(prof) else prof[i + 1]
        for s in range(n):
            u = s / n
            u2, u3 = u * u, u ** 3
            r = 0.5 * ((2 * p1[0]) + (-p0[0] + p2[0]) * u + (2 * p0[0] - 5 * p1[0] + 4 * p2[0] - p3[0]) * u2 + (-p0[0] + 3 * p1[0] - 3 * p2[0] + p3[0]) * u3)
            z = 0.5 * ((2 * p1[1]) + (-p0[1] + p2[1]) * u + (2 * p0[1] - 5 * p1[1] + 4 * p2[1] - p3[1]) * u2 + (-p0[1] + 3 * p1[1] - 3 * p2[1] + p3[1]) * u3)
            out.append((r, z))
    out.append(prof[-1])
    return out


def vase(c, iso, prof, body, band, top_fill, open_top=False, stroke=None, sw=SW, inner=None):
    t = c.t
    stroke = stroke or t["ink"]
    S = iso.p
    rows = []
    for r, z in smooth_profile(prof):
        rx, ry = iso.ell_r(r)
        x, y = S(0, 0, z)
        rows.append((x, y, rx, ry))
    left = [(x - rx, y) for x, y, rx, ry in rows]
    right = [(x + rx, y) for x, y, rx, ry in rows]
    x0, y0, rx0, ry0 = rows[0]
    x1, y1, rx1, ry1 = rows[-1]
    body_d = (f"M{pt(left[0])} " + " ".join(f"L{pt(p)}" for p in left[1:]) + f" L{pt(right[-1])} "
              + " ".join(f"L{pt(p)}" for p in right[::-1][1:])
              + f" A{fmt(rx0)} {fmt(ry0)} 0 0 1 {pt(left[0])} Z")
    c.add(P(body_d, fill=body))
    band_d = (f"M{pt((x0 + rx0 * 0.28, y0 + ry0 * 0.96))} "
              + " ".join(f"L{pt((x + rx * 0.28, y + ry * 0.96))}" for x, y, rx, ry in rows[1:])
              + " " + " ".join(f"L{pt(p)}" for p in right[::-1]) + " Z")
    c.add(P(band_d, fill=band))
    c.add(P(body_d, stroke=stroke, sw=sw))
    if open_top:
        c.add(ellipse(x1, y1, rx1, ry1, fill=inner or mix(body, t["hole"], 0.35), stroke=stroke, sw=sw))
    else:
        c.add(ellipse(x1, y1, rx1, ry1, fill=top_fill, stroke=stroke, sw=sw))
    return rows


def rev_arrow(c, rows, frac, col, extra=2.4):
    x, y, rx, ry = rows[int(len(rows) * frac)]
    curved_arrow(c, x, y, rx + extra, ry + extra * 0.5, 172, 12, col, 2.2)


@icon("features", "revolve", "Revolve")
def _(c, t):
    iso = fitted(1.0, [(7, 7, 0), (-7, -7, 0), (7, -7, 0), (-7, 7, 0), (0, 0, 17)], 16, 16.5)
    rows = vase(c, iso, VASE, t["left"], t["right"], t["top"])
    c.add(line(iso.p(0, 0, -2.4), iso.p(0, 0, 17.4), t["acc"], 1.5, dash=DASHDOT, cap="butt"))
    rev_arrow(c, rows, 0.40, t["acc"], 2.6)


@icon("features", "revolve-surface", "Revolve Surface")
def _(c, t):
    iso = fitted(1.0, [(7, 7, 0), (-7, -7, 0), (7, -7, 0), (-7, 7, 0), (0, 0, 17)], 16, 16.5)
    rows = vase(c, iso, VASE, t["sf_left"], t["sf_right"], t["sf_top"], open_top=True, inner=t["sf_right"])
    c.add(line(iso.p(0, 0, -2.4), iso.p(0, 0, 17.4), t["acc"], 1.5, dash=DASHDOT, cap="butt"))
    x0, y0, rx0, ry0 = rows[0]
    c.add(P(f"M{fmt(x0 - rx0)} {fmt(y0)} A{fmt(rx0)} {fmt(ry0)} 0 0 0 {fmt(x0 + rx0)} {fmt(y0)}", stroke=t["acc"], sw=2.2))
    rev_arrow(c, rows, 0.45, t["acc"], 2.6)


@icon("features", "revolve-cut", "Revolve Cut")
def _(c, t):
    iso = fitted(1.0, [(7.5, 7.5, 0), (-7.5, -7.5, 0), (7.5, -7.5, 0), (-7.5, 7.5, 0), (0, 0, 14)], 16, 17)
    H = 11
    cyl(c, iso, 0, 0, 0, H, 7.5)
    x, y = iso.p(0, 0, H)
    rx, ry = iso.ell_r(5.6)
    rim = ellipse_d(x, y, rx, ry)
    cid = c.clip(rim)
    D = 3.6
    c.add(ellipse(x, y, rx, ry, fill=t["cut_right"]))
    gopen(c, cid)
    c.add(ellipse(x, y + D, rx, ry, fill=t["cut_top"]))
    # central island stands proud of the groove floor
    cyl(c, iso, 0, 0, H - D, H, 3.0)
    gclose(c)
    c.add(ellipse(x, y, rx, ry, stroke=t["ink"], sw=1.4))
    x2, y2 = iso.p(0, 0, H)
    c.add(ellipse(x2, y2, iso.ell_r(3.0)[0], iso.ell_r(3.0)[1], stroke=t["ink"], sw=1.4))
    c.add(line(iso.p(0, 0, -2.4), iso.p(0, 0, 14.4), t["acc"], 1.4, dash=DASHDOT, cap="butt"))


# -------------------------------------------------------------------- sweep
def sweep_path():
    return (5.5, 24.5), (5.5, 7), (27, 4), (26.5, 21.5)


@icon("features", "sweep", "Sweep")
def _(c, t):
    p0, p1, p2, p3 = sweep_path()
    d = f"M{pt(p0)} C{pt(p1)} {pt(p2)} {pt(p3)}"
    tube(c, d, 6.2, t["left"], t["ink"], sw=1.5, hi=t["top"])
    c.add(P(d, stroke=t["acc"], sw=1.5, dash="1.8 2.2", cap="butt"))
    c.add(ellipse(p0[0], p0[1], 4.8, 2.2, fill=t["acc_soft"], stroke=t["acc"], sw=1.9))
    c.add(ellipse(p3[0], p3[1], 4.8, 2.2, fill=t["right"], stroke=t["ink"], sw=1.4))


@icon("features", "sweep-cut", "Sweep Cut")
def _(c, t):
    W, D, H = 15, 15, 8
    iso = fitted(1.0, [(0, 0, 0), (W, D, 0), (W, 0, 0), (0, D, 0), (0, 0, H)], 16, 17.5)
    box(c, iso, 0, 0, 0, W, D, H)
    cen = [(2.2 + 10.6 * i / 18, 7.5 + 3.6 * math.sin(i / 18 * math.pi * 1.5)) for i in range(19)]
    poly_pts = ribbon([iso.p(x, y, H) for x, y in cen], [4.4] * 19)
    trench(c, poly_pts, 1.8, t["cut_right"], t["cut_top"], stroke=t["ink"], sw=1.3)
    x0, y0 = iso.p(*cen[0], H)
    c.add(circle(x0, y0, 1.7, fill=t["acc"], stroke=t["bg"], sw=0.7))


@icon("features", "sweep-surface", "Sweep Surface")
def _(c, t):
    p0, p1, p2, p3 = (5, 24), (4, 8), (24, 4), (26.5, 20)
    n = 20
    cen = [bez(p0, p1, p2, p3, i / n) for i in range(n + 1)]
    wid = [8.5] * (n + 1)
    poly_pts = ribbon(cen, wid)
    c.add(poly(poly_pts, fill=t["sf_left"], stroke=t["ink"], sw=1.5))
    for i in range(0, n, 3):
        a = ribbon(cen[i:i + 2], [8.5, 8.5])
        c.add(poly(a, fill=t["sf_top"], stroke=t["sf_top"], sw=0.5, op=0.65))
    c.add(poly(poly_pts, stroke=t["ink"], sw=1.5))
    d = f"M{pt(p0)} C{pt(p1)} {pt(p2)} {pt(p3)}"
    c.add(P(d, stroke=t["acc"], sw=1.8, dash="2 2.2", cap="butt"))


@icon("features", "swept-blend", "Swept Blend")
def _(c, t):
    p0, p1, p2, p3 = (7.5, 25), (5, 10), (14, 5.5), (27, 7)
    n = 26
    cen = [bez(p0, p1, p2, p3, i / n) for i in range(n + 1)]
    wid = [11 - 7.4 * (i / n) for i in range(n + 1)]
    outl = ribbon(cen, wid)
    c.add(poly(outl, fill=t["left"], stroke=t["ink"], sw=1.5, join="round"))
    c.add(poly(ribbon(cen[:n // 2 + 3], [w * 0.28 for w in wid[:n // 2 + 3]]), fill=t["top"], op=0.7))
    d = f"M{pt(p0)} C{pt(p1)} {pt(p2)} {pt(p3)}"
    c.add(P(d, stroke=t["acc"], sw=1.4, dash="1.8 2.2", cap="butt"))
    c.add(ellipse(p0[0], p0[1], 5.6, 2.5, fill=t["acc_soft"], stroke=t["acc"], sw=1.9))
    c.add(rect(p3[0] - 1.7, p3[1] - 2.8, 3.4, 5.6, r=0.8, fill=t["acc_soft"], stroke=t["acc"], sw=1.7))


@icon("features", "blend", "Blend")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (6.5, 6.5, 14)], 16, 17)
    S = iso.p
    # loft from a square (z=0) to a smaller circle (z=13)
    r = 3.6
    cx, cy, zt = 6.5, 6.5, 13
    ex, ey = S(cx, cy, zt)
    rx, ry = iso.ell_r(r)
    bl, bf, br = S(0, 13, 0), S(13, 13, 0), S(13, 0, 0)
    fl_ = [bl, bf, (ex, ey + ry * 0.98), (ex - rx, ey)]
    fr_ = [bf, br, (ex + rx, ey), (ex, ey + ry * 0.98)]
    c.add(poly(fl_, fill=t["left"], stroke=t["ink"], sw=1.5))
    c.add(poly(fr_, fill=t["right"], stroke=t["ink"], sw=1.5))
    c.add(P(f"M{pt(bl)} L{pt(bf)} L{pt(br)}", stroke=t["acc"], sw=2.0))
    c.add(ellipse(ex, ey, rx, ry, fill=t["top"], stroke=t["acc"], sw=2.0))
    # far side silhouette of the loft
    c.add(P(f"M{pt(S(13, 0, 0))} L{pt((ex + rx, ey))}", stroke=t["ink"], sw=1.5))


@icon("features", "blend-cut", "Blend Cut")
def _(c, t):
    W, D, H = 15, 15, 9
    iso = fitted(1.0, [(0, 0, 0), (W, D, 0), (W, 0, 0), (0, D, 0), (0, 0, H)], 16, 17.5)
    box(c, iso, 0, 0, 0, W, D, H)
    cid = c.clip(pts_d([iso.p(0, 0, H), iso.p(W, 0, H), iso.p(W, D, H), iso.p(0, D, H)], True))
    # tapered pocket: wide square rim, narrower circular floor
    x, y = iso.p(7.5, 7.5, H)
    rx, ry = iso.ell_r(6)
    c.add(ellipse(x, y, rx, ry, fill=t["cut_left"], stroke=t["ink"], sw=1.5))
    fx, fy = iso.p(7.5, 7.5, H - 4)
    frx, fry = iso.ell_r(3)
    c.add(ellipse(fx, fy, frx, fry, fill=t["cut_top"], stroke=t["cut"], sw=1.5))
    c.add(ellipse(x, y, rx, ry, stroke=t["ink"], sw=1.5))


@icon("features", "helical-sweep", "Helical Sweep")
def _(c, t):
    cx, rx, ry = 16, 9.0, 3.6
    turns, pitch, top_y, npt = 2, 9.4, 6.6, 24
    pts = []
    for i in range(int(turns * npt) + 1):
        a = math.pi + 2 * math.pi * i / npt
        pts.append((cx + rx * math.cos(a), top_y + (i / npt) * pitch + ry * math.sin(a), math.sin(a)))
    # split where the half changes (back = upper half on screen, sin < 0)
    bounds = [0]
    for i in range(1, len(pts)):
        if (pts[i][2] >= 0) != (pts[i - 1][2] >= 0):
            bounds.append(i)
    bounds.append(len(pts) - 1)
    c.add(line((cx, 1.5), (cx, 30.5), t["acc"], 1.4, dash=DASHDOT, cap="butt"))
    for front in (False, True):
        for k in range(len(bounds) - 1):
            lo, hi = bounds[k], bounds[k + 1]
            mid = pts[(lo + hi) // 2][2]
            if (mid >= 0) != front:
                continue
            lo2, hi2 = (max(0, lo - 2), min(len(pts) - 1, hi + 2)) if front else (lo, hi)
            xy = [(x, y) for x, y, _ in pts[lo2:hi2 + 1]]
            tube(c, xy, 2.4, t["left"] if front else t["right"], t["ink"], sw=1.15)


# ------------------------------------------------------------- engineering
def block(W=15, D=15, H=9, k=1.0, cy=17.5, extra=None):
    pts = [(0, 0, 0), (W, D, 0), (W, 0, 0), (0, D, 0), (0, 0, H)] + (extra or [])
    return fitted(k, pts, 16, cy)


def hole_colors(t):
    return mix(t["right"], t["hole"], 0.45), mix(t["left"], t["hole"], 0.62)


def thread_ring(c, iso, cx, cy, z, r, col):
    x, y = iso.p(cx, cy, z)
    rx, ry = iso.ell_r(r)
    d, e, tn = arc_d(SCREEN, x, y, ry, 35, 305, ru=rx)
    c.add(P(d, stroke=col, sw=1.7))


@icon("features", "hole", "Hole")
def _(c, t):
    W = D = 15
    H = 9
    iso = block(W, D, H, 1.0, 19, [(7.5, 7.5, H + 9)])
    box(c, iso, 0, 0, 0, W, D, H)
    wall, floor = hole_colors(t)
    hole_at(c, iso, 7.5, 7.5, H, 4.2, 6, wall, floor, sw=1.5)
    dn_arrow(c, iso, 7.5, 7.5, H + 9, H + 2.2, col=t["acc"])


@icon("features", "hole-counterbore", "Counterbore Hole")
def _(c, t):
    W = D = 15
    H = 9
    iso = block(W, D, H, 1.0, 19, [(7.5, 7.5, H + 9)])
    box(c, iso, 0, 0, 0, W, D, H)
    wall, floor = hole_colors(t)
    R, r2, d1, d2 = 5.8, 3.0, 2.4, 6
    x, y = iso.p(7.5, 7.5, H)
    rx, ry = iso.ell_r(R)
    cid1 = c.clip(ellipse_d(x, y, rx, ry))
    c.add(ellipse(x, y, rx, ry, fill=wall))
    gopen(c, cid1)
    c.add(ellipse(x, y + d1, rx, ry, fill=t["left"]))
    x2, y2 = iso.p(7.5, 7.5, H - d1)
    rx2, ry2 = iso.ell_r(r2)
    cid2 = c.clip(ellipse_d(x2, y2, rx2, ry2))
    c.add(ellipse(x2, y2, rx2, ry2, fill=wall))
    gopen(c, cid2)
    c.add(ellipse(x2, y2 + d2, rx2, ry2, fill=floor))
    gclose(c)
    c.add(ellipse(x2, y2, rx2, ry2, stroke=t["ink"], sw=1.3))
    gclose(c)
    c.add(ellipse(x, y, rx, ry, stroke=t["ink"], sw=1.5))
    dn_arrow(c, iso, 7.5, 7.5, H + 9, H + 2.6, col=t["acc"])


@icon("features", "hole-countersink", "Countersink Hole")
def _(c, t):
    W = D = 15
    H = 9
    iso = block(W, D, H, 1.0, 19, [(7.5, 7.5, H + 9)])
    box(c, iso, 0, 0, 0, W, D, H)
    wall, floor = hole_colors(t)
    R, r2, dz = 6.0, 2.4, 3.4
    x, y = iso.p(7.5, 7.5, H)
    rx, ry = iso.ell_r(R)
    cid = c.clip(ellipse_d(x, y, rx, ry))
    c.add(ellipse(x, y, rx, ry, fill=mix(t["left"], t["hole"], 0.18)))
    gopen(c, cid)
    x2, y2 = iso.p(7.5, 7.5, H - dz)
    rx2, ry2 = iso.ell_r(r2)
    # cone surface lit from the right, darker toward the bore
    c.add(ellipse(x + 0.8, y + 0.6, rx * 0.8, ry * 0.8, fill=mix(t["right"], t["left"], 0.4), op=0.55))
    c.add(ellipse(x2, y2, rx2, ry2, fill=floor, stroke=t["ink"], sw=1.2))
    gclose(c)
    c.add(ellipse(x, y, rx, ry, stroke=t["ink"], sw=1.5))
    dn_arrow(c, iso, 7.5, 7.5, H + 9, H + 2.6, col=t["acc"])


@icon("features", "hole-threaded", "Standard (Threaded) Hole")
def _(c, t):
    W = D = 15
    H = 9
    iso = block(W, D, H, 1.0, 19, [(7.5, 7.5, H + 9)])
    box(c, iso, 0, 0, 0, W, D, H)
    wall, floor = hole_colors(t)
    hole_at(c, iso, 7.5, 7.5, H, 3.4, 6, wall, floor, sw=1.5)
    thread_ring(c, iso, 7.5, 7.5, H, 5.7, t["acc"])
    dn_arrow(c, iso, 7.5, 7.5, H + 9, H + 2.6, col=t["acc"])


def arc_plan(cx, cy, r, a0, a1, step=10):
    pts = []
    a = a0
    while (a <= a1 + 1e-6) if a1 >= a0 else (a >= a1 - 1e-6):
        pts.append((cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a))))
        a += step if a1 >= a0 else -step
    return pts


@icon("features", "round", "Round (Fillet)")
def _(c, t):
    arc = arc_plan(8, 8, 5, 0, 90)
    plan = [(0, 0), (13, 0)] + arc + [(0, 13)]
    iso = fitted(1.05, [(x, y, 0) for x, y in plan] + [(x, y, 11) for x, y in plan], 16, 16.5)
    tint = set(range(2, 2 + len(arc) - 1))
    solid_plan(c, iso, plan, 0, 11, tint=tint, tint_cols=(t["hl_left"], t["hl_right"]))


@icon("features", "round-auto", "Auto Round")
def _(c, t):
    a1 = arc_plan(9, 4, 4, -90, 0)
    a2 = arc_plan(9, 9, 4, 0, 90)
    a3 = arc_plan(4, 9, 4, 90, 180)
    plan = [(0, 0), (9, 0)] + a1 + a2 + a3 + [(0, 0)][:0]
    plan = [(4, 0)] + [(9, 0)] + a1[1:] + a2[1:] + a3[1:]
    plan = [(0, 0), (9, 0)] + a1[1:] + a2[1:] + a3[1:] + [(0, 4)]
    iso = fitted(1.05, [(x, y, 0) for x, y in plan] + [(x, y, 11) for x, y in plan], 16, 16.5)
    tint = set(range(2, len(plan) - 2))
    solid_plan(c, iso, plan, 0, 11, tint=tint, tint_cols=(t["hl_left"], t["hl_right"]))
    # sparkle marks "automatic"
    sx, sy = 25.5, 6
    c.add(P(f"M{sx} {sy - 4.2} Q{sx} {sy} {sx + 4.2} {sy} Q{sx} {sy} {sx} {sy + 4.2} Q{sx} {sy} {sx - 4.2} {sy} Q{sx} {sy} {sx} {sy - 4.2} Z",
            fill=t["acc"], stroke=t["acc"], sw=0.8))


@icon("features", "round-full", "Full Round")
def _(c, t):
    arc = arc_plan(10, 4.5, 4.5, -90, 90)
    plan = [(0, 0)] + arc + [(0, 9)]
    iso = fitted(1.15, [(x, y, 0) for x, y in plan] + [(x, y, 8) for x, y in plan], 16, 17)
    tint = set(range(1, len(arc)))
    solid_plan(c, iso, plan, 0, 8, tint=tint, tint_cols=(t["hl_left"], t["hl_right"]))


@icon("features", "chamfer", "Chamfer (Edge)")
def _(c, t):
    plan = [(0, 0), (13, 0), (13, 7.5), (7.5, 13), (0, 13)]
    iso = fitted(1.05, [(x, y, 0) for x, y in plan] + [(x, y, 11) for x, y in plan], 16, 16.5)
    solid_plan(c, iso, plan, 0, 11, tint={2}, tint_cols=(t["hl_left"], t["hl_right"]))


@icon("features", "chamfer-corner", "Corner Chamfer")
def _(c, t):
    W = D = H = 12
    d = 5.5
    iso = fitted(1.05, [(0, 0, 0), (W, D, 0), (W, 0, 0), (0, D, 0), (0, 0, H)], 16, 17)
    p = iso.p
    c.add(poly([p(0, D, 0), p(W, D, 0), p(W, D, H - d), p(W - d, D, H), p(0, D, H)], fill=t["left"], stroke=t["ink"], sw=SW))
    c.add(poly([p(W, 0, 0), p(W, D, 0), p(W, D, H - d), p(W, D - d, H), p(W, 0, H)], fill=t["right"], stroke=t["ink"], sw=SW))
    c.add(poly([p(0, 0, H), p(W, 0, H), p(W, D - d, H), p(W - d, D, H), p(0, D, H)], fill=t["top"], stroke=t["ink"], sw=SW))
    c.add(poly([p(W - d, D, H), p(W, D - d, H), p(W, D, H - d)], fill=t["hl_top"], stroke=t["acc"], sw=1.7))


@icon("features", "draft", "Draft")
def _(c, t):
    B, T, H = 14, 10.5, 12
    iso = fitted(1.05, [(0, 0, 0), (B, B, 0), (B, 0, 0), (0, B, 0), (0, 0, H)], 16, 17)
    p = iso.p
    c.add(poly([p(0, B, 0), p(B, B, 0), p(T, T, H), p(0, T, H)], fill=t["hl_left"], stroke=t["ink"], sw=SW))
    c.add(poly([p(B, 0, 0), p(B, B, 0), p(T, T, H), p(T, 0, H)], fill=t["hl_right"], stroke=t["ink"], sw=SW))
    c.add(poly([p(0, 0, H), p(T, 0, H), p(T, T, H), p(0, T, H)], fill=t["top"], stroke=t["ink"], sw=SW))
    # pull direction + draft angle reference
    a = p(B, B, 0)
    c.add(line(a, p(B, B, H + 2), t["ink"], 1.3, dash=DASH))


@icon("features", "shell", "Shell")
def _(c, t):
    W = D = 15
    H = 11
    iso = block(W, D, H, 1.0, 16.5)
    w = 2.8
    pocket(c, iso, W, D, H, (w, w, W - w, D - w), 2.8, mix(t["top"], t["hole"], 0.18),
           wall_cols=(mix(t["left"], t["hole"], 0.28), mix(t["right"], t["hole"], 0.28)), top_fill=t["top"])


@icon("features", "rib", "Rib")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 13)], 16, 17)
    box(c, iso, 0, 0, 0, 14, 14, 3)
    box(c, iso, 0, 0, 3, 3.2, 14, 13)
    p = iso.p
    y0, y1 = 5.8, 8.6
    tri_l = [p(3.2, y1, 3), p(13, y1, 3), p(3.2, y1, 11)]
    tri_top = [p(13, y0, 3), p(13, y1, 3), p(3.2, y1, 11), p(3.2, y0, 11)]
    c.add(poly(tri_top, fill=t["hl_top"], stroke=t["ink"], sw=SW))
    c.add(poly(tri_l, fill=t["hl_left"], stroke=t["ink"], sw=SW))
    # lift the rib's visible left face over the hypotenuse
    c.add(poly(tri_l, fill=t["hl_left"], stroke=t["ink"], sw=SW))


@icon("features", "rib-trajectory", "Trajectory Rib")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (15, 15, 0), (15, 0, 0), (0, 15, 0), (0, 0, 11)], 16, 17.5)
    box(c, iso, 0, 0, 0, 15, 15, 3)
    outer = arc_plan(0, 0, 12.5, 0, 90, 6)
    inner = arc_plan(0, 0, 9.8, 90, 0, 6)
    plan = outer + inner
    solid_plan(c, iso, plan, 3, 9.5, tint=set(range(len(plan))), tint_cols=(t["hl_left"], t["hl_right"]),
               top_fill=t["hl_top"])
