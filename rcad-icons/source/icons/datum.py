"""Datum features (teal)."""
import math
from kit import *  # noqa: F401,F403
from kit import icon, arc_d, SCREEN, Iso, mix, box, ellipse_d
from icons.features import fitted
from icons.sketch import ink_dot


def plane_poly(c, pts, strong=False):
    t = c.t
    c.add(poly(pts, fill=t["datum_soft"], fop=0.9, stroke=t["datum"], sw=1.8))


def corner_tag(c, p, d=1.0):
    t = c.t
    c.add(rect(p[0] - 1.9, p[1] - 1.9, 3.8, 3.8, fill=t["datum"], stroke=t["bg"], sw=0.6))


def letter(c, ch, x, y, col, s=1.0, w=1.3):
    """tiny stroked capital X / Y / Z, centred at x,y"""
    h = 2.3 * s
    if ch == "X":
        c.add(line((x - h, y - h), (x + h, y + h), col, w), line((x + h, y - h), (x - h, y + h), col, w))
    elif ch == "Y":
        c.add(polyline([(x - h, y - h), (x, y)], col, w), line((x + h, y - h), (x, y), col, w), line((x, y), (x, y + h), col, w))
    elif ch == "Z":
        c.add(polyline([(x - h, y - h), (x + h, y - h), (x - h, y + h), (x + h, y + h)], col, w))


@icon("datum", "datum-plane", "Datum Plane")
def _(c, t):
    iso = fitted(1.15, [(0, 0, 0), (14, 0, 0), (14, 14, 0), (0, 14, 0)], 16, 16.5)
    pts = [iso.p(0, 0, 0), iso.p(14, 0, 0), iso.p(14, 14, 0), iso.p(0, 14, 0)]
    plane_poly(c, pts)
    corner_tag(c, iso.p(14, 14, 0) if False else iso.p(14, 0, 0))
    c.add(line(iso.p(7, 0, 0), iso.p(7, 14, 0), t["datum"], 1.0, dash=DASH, op=0.7))
    c.add(line(iso.p(0, 7, 0), iso.p(14, 7, 0), t["datum"], 1.0, dash=DASH, op=0.7))


@icon("datum", "datum-plane-top", "Datum Plane TOP")
def _(c, t):
    iso = fitted(1.15, [(0, 0, 0), (14, 0, 0), (14, 14, 0), (0, 14, 0)], 16, 16.5)
    plane_poly(c, [iso.p(0, 0, 0), iso.p(14, 0, 0), iso.p(14, 14, 0), iso.p(0, 14, 0)])
    corner_tag(c, iso.p(14, 14, 0))


@icon("datum", "datum-plane-front", "Datum Plane FRONT")
def _(c, t):
    iso = fitted(1.15, [(0, 14, 0), (14, 14, 0), (14, 14, 14), (0, 14, 14)], 16, 16.5)
    plane_poly(c, [iso.p(0, 14, 0), iso.p(14, 14, 0), iso.p(14, 14, 14), iso.p(0, 14, 14)])
    corner_tag(c, iso.p(14, 14, 0))


@icon("datum", "datum-plane-right", "Datum Plane RIGHT")
def _(c, t):
    iso = fitted(1.15, [(14, 0, 0), (14, 14, 0), (14, 14, 14), (14, 0, 14)], 16, 16.5)
    plane_poly(c, [iso.p(14, 0, 0), iso.p(14, 14, 0), iso.p(14, 14, 14), iso.p(14, 0, 14)])
    corner_tag(c, iso.p(14, 14, 0))


@icon("datum", "datum-plane-offset", "Offset Datum Plane")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 0, 0), (13, 13, 0), (0, 13, 0), (0, 0, 10)], 16, 17.5)
    base = [iso.p(0, 0, 0), iso.p(13, 0, 0), iso.p(13, 13, 0), iso.p(0, 13, 0)]
    c.add(poly(base, fill=t["top"], stroke=t["ink"], sw=1.4))
    top = [iso.p(0, 0, 10), iso.p(13, 0, 10), iso.p(13, 13, 10), iso.p(0, 13, 10)]
    plane_poly(c, top)
    a, b = iso.p(6.5, 6.5, 0.2), iso.p(6.5, 6.5, 9.8)
    c.add(arrow(a, b, t["acc"], 1.9, two=True, size=3.8, half=2.2))


@icon("datum", "datum-axis", "Datum Axis")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (12, 12, 0), (12, 0, 0), (0, 12, 0)], 16, 22)
    x, y = iso.p(6, 6, 0)
    rx, ry = iso.ell_r(7.5)
    c.add(ellipse(x, y, rx, ry, fill=t["top"], stroke=t["ink"], sw=1.5))
    c.add(ellipse(x, y, rx * 0.45, ry * 0.45, fill=t["bg"], stroke=t["ink"], sw=1.3))
    c.add(line((x, 2.5), (x, 30), t["datum"], 2.0, dash=DASHDOT, cap="butt"))
    c.add(poly(head_pts((x, 1.4), (0, -1), 4.4, 2.6), fill=t["datum"], stroke=t["datum"], sw=0.5))


@icon("datum", "datum-point", "Datum Point")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0)], 16, 18)
    c.add(poly([iso.p(0, 0, 0), iso.p(14, 0, 0), iso.p(14, 14, 0), iso.p(0, 14, 0)], fill=t["top"], stroke=t["ink"], sw=1.4))
    x, y = iso.p(7, 7, 0)
    c.add(line((x - 5.2, y - 5.2), (x + 5.2, y + 5.2), t["datum"], 2.6), line((x + 5.2, y - 5.2), (x - 5.2, y + 5.2), t["datum"], 2.6))
    c.add(circle(x, y, 2.0, fill=t["bg"], stroke=t["datum"], sw=1.2))


@icon("datum", "datum-point-array", "Datum Points (Multiple)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0)], 16, 18)
    c.add(poly([iso.p(0, 0, 0), iso.p(14, 0, 0), iso.p(14, 14, 0), iso.p(0, 14, 0)], fill=t["top"], stroke=t["ink"], sw=1.4))
    for (u, v) in ((3.5, 3.5), (10.5, 3.5), (7, 7), (3.5, 10.5), (10.5, 10.5)):
        x, y = iso.p(u, v, 0)
        c.add(line((x - 2.4, y - 2.4), (x + 2.4, y + 2.4), t["datum"], 1.9), line((x + 2.4, y - 2.4), (x - 2.4, y + 2.4), t["datum"], 1.9))


@icon("datum", "datum-csys", "Coordinate System")
def _(c, t):
    o = (16, 19.5)
    c.add(arrow(o, (27, 26), t["datum"], 2.1, size=4.4, half=2.5))
    c.add(arrow(o, (5, 26), t["datum"], 2.1, size=4.4, half=2.5))
    c.add(arrow(o, (16, 4), t["datum"], 2.1, size=4.4, half=2.5))
    c.add(circle(o[0], o[1], 2.2, fill=t["ink"], stroke=t["bg"], sw=0.8))
    letter(c, "X", 28.2, 22.4, t["ink"], 0.7, 1.15)
    letter(c, "Y", 3.8, 22.4, t["ink"], 0.7, 1.15)
    letter(c, "Z", 21.2, 4.6, t["ink"], 0.7, 1.15)


@icon("datum", "datum-csys-offset", "Offset Coordinate System")
def _(c, t):
    c.add(arrow((6, 26), (6, 11), t["mute"], 1.4, size=3.4, half=1.9, dash="2 2"))
    c.add(arrow((6, 26), (17, 31), t["mute"], 1.4, size=3.4, half=1.9, dash="2 2"))
    o = (17, 14)
    c.add(arrow(o, (27, 19.5), t["datum"], 1.9, size=3.8, half=2.3))
    c.add(arrow(o, (7, 19.5), t["datum"], 1.9, size=3.8, half=2.3))
    c.add(arrow(o, (17, 2.5), t["datum"], 1.9, size=3.8, half=2.3))
    c.add(circle(o[0], o[1], 2.0, fill=t["ink"], stroke=t["bg"], sw=0.8))
