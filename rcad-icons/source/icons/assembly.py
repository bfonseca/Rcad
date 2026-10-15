"""Assembly operations and constraints."""
import math
from kit import *  # noqa: F401,F403
from kit import (icon, arc_d, SCREEN, Iso, mix, box, cyl, box_ghost, solid_plan, pocket, hole_at, ellipse_d,
                 gopen, gclose)
from icons.features import fitted, arc_plan, curved_arrow, hole_colors, dn_arrow
from icons.datum import letter


def lock_badge(c, cx, cy, s=1.0):
    t = c.t
    d, e, tn = arc_d(SCREEN, cx, cy - 1.6 * s, 2.6 * s, 180, 360)
    c.add(P(d + f" L{fmt(cx + 2.6 * s)} {fmt(cy)} M{fmt(cx - 2.6 * s)} {fmt(cy - 1.6 * s)} L{fmt(cx - 2.6 * s)} {fmt(cy)}", stroke=t["ink"], sw=1.6))
    c.add(rect(cx - 4.4 * s, cy - 0.6 * s, 8.8 * s, 6.6 * s, r=1.2, fill=t["acc"], stroke=t["bg"], sw=0.9))


def plus_badge(c, cx, cy, r=5.2):
    t = c.t
    c.add(circle(cx, cy, r, fill=t["acc"], stroke=t["bg"], sw=1.0))
    c.add(line((cx - r * 0.5, cy), (cx + r * 0.5, cy), t["white"], 1.9), line((cx, cy - r * 0.5), (cx, cy + r * 0.5), t["white"], 1.9))


def tint3(t):
    return (t["hl_top"], t["hl_left"], t["hl_right"])


@icon("assembly", "asm-assemble", "Assemble Component")
def _(c, t):
    iso = fitted(0.92, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (7, 7, 20)], 16, 17.5)
    box(c, iso, 0, 0, 0, 14, 14, 6)
    wall, floor = hole_colors(t)
    hole_at(c, iso, 7, 7, 6, 3.6, 4, wall, floor, sw=1.4)
    cyl(c, iso, 7, 7, 12, 19, 2.8, fills=tint3(t))
    p0, p1 = iso.p(7, 7, 11.2), iso.p(7, 7, 8.2)
    c.add(arrow((p0[0], p0[1]), (p1[0], p1[1]), t["acc"], 1.9, size=3.6, half=2.1))


@icon("assembly", "asm-insert", "Insert")
def _(c, t):
    iso = fitted(0.92, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (7, 7, 20)], 16, 17.5)
    box(c, iso, 0, 0, 0, 14, 14, 6)
    wall, floor = hole_colors(t)
    hole_at(c, iso, 7, 7, 6, 3.8, 4, wall, floor, sw=1.4)
    cyl(c, iso, 7, 7, 7.5, 15, 3.0, fills=tint3(t))
    a, b = iso.p(7, 7, 3.5), iso.p(7, 7, 18.5)
    c.add(line(a, b, t["acc"], 1.4, dash=DASHDOT, cap="butt"))


@icon("assembly", "asm-new-component", "Create Component")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 11)], 14.5, 16.5)
    box(c, iso, 0, 0, 0, 13, 13, 11)
    plus_badge(c, 24.5, 24.5, 5.6)


@icon("assembly", "asm-mate", "Coincident (Mate)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 14)], 16, 17.5)
    box(c, iso, 0, 0, 0, 14, 14, 6)
    box(c, iso, 2, 2, 6, 12, 12, 13)
    p = iso.p
    c.add(poly([p(2, 2, 6), p(12, 2, 6), p(12, 12, 6), p(2, 12, 6)], stroke=t["acc"], sw=2.0))
    c.add(poly([p(2, 12, 6), p(12, 12, 6), p(12, 12, 6), p(2, 12, 6)], stroke=t["acc"], sw=2.0))


@icon("assembly", "asm-distance", "Offset (Distance)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 19)], 15, 16.5)
    box(c, iso, 0, 0, 0, 13, 13, 5)
    box(c, iso, 0, 0, 12, 13, 13, 17)
    a, b = iso.p(13, 13, 5.2), iso.p(13, 13, 11.8)
    c.add(arrow((a[0] + 3.2, a[1] + 1), (b[0] + 3.2, b[1] + 1), t["acc"], 1.8, two=True, size=3.8, half=2.2))
    c.add(line((a[0] + 0.5, a[1]), (a[0] + 4.8, a[1] + 1.2), t["acc"], 1.1, op=0.9), line((b[0] + 0.5, b[1]), (b[0] + 4.8, b[1] + 1.2), t["acc"], 1.1, op=0.9))


@icon("assembly", "asm-angle", "Angle Offset")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 12, 0), (14, 0, 0), (0, 12, 0), (0, 4, 11)], 16, 18)
    box(c, iso, 0, 0, 0, 14, 12, 2)
    th = math.radians(42)
    L = 10.5
    p = iso.p
    plate = [p(0, 0, 2), p(14, 0, 2), p(14, L * math.cos(th), 2 + L * math.sin(th)), p(0, L * math.cos(th), 2 + L * math.sin(th))]
    c.add(poly(plate, fill=t["hl_left"], stroke=t["ink"], sw=SW))
    A = iso.sidep(14)
    d, e, tn = arc_d(A, 0, 2, 7.5, 0, 42)
    c.add(P(d, stroke=t["acc"], sw=1.8))


@icon("assembly", "asm-parallel", "Parallel")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 10, 0), (14, 0, 0), (0, 10, 0), (0, 0, 11)], 16, 17)
    box(c, iso, 0, 0, 0, 14, 10, 2, fills=tint3(t))
    box(c, iso, 0, 0, 7.5, 14, 10, 9.5, fills=tint3(t))
    a, b = iso.p(14, 10, 2.4), iso.p(14, 10, 7.1)


@icon("assembly", "asm-normal", "Normal (Perpendicular)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 12, 0), (14, 0, 0), (0, 12, 0), (0, 0, 12)], 16, 17.5)
    box(c, iso, 0, 0, 0, 14, 12, 2, fills=tint3(t))
    box(c, iso, 0, 0, 2, 2, 12, 12, fills=tint3(t))
    p = iso.p
    c.add(polyline([p(6, 6, 2.1), p(6, 6, 6), p(2.1, 6, 6)], t["ink"], 1.6, join="miter"))


@icon("assembly", "asm-fix", "Fix Component")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 11)], 14.5, 16)
    box(c, iso, 0, 0, 0, 13, 13, 11)
    lock_badge(c, 24.5, 23.5, 1.1)


@icon("assembly", "asm-default", "Default (Align to Default CSYS)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 11)], 16, 17.5)
    box(c, iso, 0, 0, 0, 13, 13, 11)
    o = iso.p(0, 0, 0)
    for end in ((16, 0, 0), (0, 16, 0), (0, 0, 14)):
        pass
    c.add(arrow(o, iso.p(0, 0, 0)[0:2] and (o[0], o[1] - 11.5) if False else (o[0], max(1.5, o[1] - 11.5)), t["datum"], 2.0, size=4, half=2.3))
    c.add(arrow(o, iso.p(17, 0, 0), t["datum"], 2.0, size=4, half=2.3))
    c.add(arrow(o, iso.p(0, 17, 0), t["datum"], 2.0, size=4, half=2.3))
    c.add(circle(o[0], o[1], 2.0, fill=t["acc"], stroke=t["bg"], sw=0.8))


@icon("assembly", "asm-tangent", "Tangent")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (16, 16, 0), (16, 0, 0), (0, 16, 0)], 16, 24)
    c.add(poly([iso.p(0, 0, 0), iso.p(16, 0, 0), iso.p(16, 16, 0), iso.p(0, 16, 0)], fill=t["top"], stroke=t["ink"], sw=1.5))
    x, y = iso.p(8, 8, 0)
    c.add(circle(16, 12.5, 8.5, fill=t["left"], stroke=t["ink"], sw=1.6))
    c.add(ellipse(12.5, 9, 3, 2, rot=-40, fill=t["top"], op=0.9))
    c.add(circle(16, 21, 1.9, fill=t["acc"], stroke=t["bg"], sw=0.8))


@icon("assembly", "asm-explode", "Explode View")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 23)], 16, 17)
    c.add(line(iso.p(6.5, 6.5, -1), iso.p(6.5, 6.5, 25), t["acc"], 1.3, dash=DASHDOT, cap="butt"))
    box(c, iso, 0, 0, 0, 13, 13, 3)
    box(c, iso, 0, 0, 8, 13, 13, 11)
    box(c, iso, 0, 0, 16, 13, 13, 19)


@icon("assembly", "asm-interference", "Interference Check")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 11)], 16, 17)
    box_ghost(c, iso, 0, 0, 0, 9, 9, 9, color=t["mute"], sw=1.3)
    box_ghost(c, iso, 5, 5, 2, 14, 14, 11, color=t["mute"], sw=1.3)
    box(c, iso, 5, 5, 2, 9, 9, 9, fills=(t["cut_top"], t["cut_left"], t["cut_right"]), stroke=t["cut"])


@icon("assembly", "asm-bom", "Bill of Materials")
def _(c, t):
    c.add(rect(5, 4, 22, 24, r=2, fill=t["top"], stroke=t["ink"], sw=1.7))
    c.add(rect(5, 4, 22, 5.5, r=2, fill=t["left"], stroke=t["ink"], sw=1.7))
    for y in (13.5, 18.5, 23.5):
        c.add(line((8.5, y), (12, y), t["ink"], 1.6), line((15, y), (23.5, y), t["ink"], 1.6))
    c.add(line((13.4, 9.5), (13.4, 28), t["ink"], 1.0, op=0.6))


@icon("assembly", "asm-replace", "Replace Component")
def _(c, t):
    iso = fitted(0.8, [(0, 0, 0), (9, 9, 0), (9, 0, 0), (0, 9, 0), (0, 0, 9)], 9.5, 23)
    box(c, iso, 0, 0, 0, 9, 9, 9, sw=1.4)
    iso2 = Iso(0.9, 22.5, 11.5)
    cyl(c, iso2, 0, 0, -3, 3.5, 5, fills=tint3(t), sw=1.4)
    d, e, tn = arc_d(SCREEN, 16, 16, 9.5, 200, 270)
    c.add(path_arrow(d, e, tn, t["acc"], 1.9, 4, 2.3))
    d, e, tn = arc_d(SCREEN, 16, 16, 9.5, 20, 90)
    c.add(path_arrow(d, e, tn, t["acc"], 1.9, 4, 2.3))
