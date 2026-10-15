"""Sketcher tools (2D)."""
import math
from kit import *  # noqa: F401,F403
from kit import icon, arc_d, SCREEN, Iso, Aff, box, cyl

A = SCREEN
GL = 2.0  # sketch geometry stroke


def catmull(pts):
    """Catmull-Rom through pts -> cubic bezier path"""
    d = f"M{pt(pts[0])} "
    for i in range(len(pts) - 1):
        p0 = pts[i - 1] if i > 0 else pts[i]
        p1, p2 = pts[i], pts[i + 1]
        p3 = pts[i + 2] if i + 2 < len(pts) else pts[i + 1]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d += f"C{pt(c1)} {pt(c2)} {pt(p2)} "
    return d.strip()


def rot(p, ang, c=(0, 0)):
    a = math.radians(ang)
    x, y = p[0] - c[0], p[1] - c[1]
    return (c[0] + x * math.cos(a) - y * math.sin(a), c[1] + x * math.sin(a) + y * math.cos(a))


def pencil(c, tip, ang, L=11.5, w=4.4):
    """pencil whose tip is at `tip`, body extending at `ang` degrees (screen)"""
    t = c.t
    def m(x, y):
        a = math.radians(ang)
        return (tip[0] + x * math.cos(a) - y * math.sin(a), tip[1] + x * math.sin(a) + y * math.cos(a))
    cone = [m(0, 0), m(3.4, -w / 2), m(3.4, w / 2)]
    body = [m(3.4, -w / 2), m(L - 2.2, -w / 2), m(L - 2.2, w / 2), m(3.4, w / 2)]
    cap = [m(L - 2.2, -w / 2), m(L, -w / 2), m(L, w / 2), m(L - 2.2, w / 2)]
    c.add(poly(cone, fill=t["acc_soft"], stroke=t["ink"], sw=1.2))
    c.add(poly(body, fill=t["acc"], stroke=t["ink"], sw=1.2))
    c.add(poly(cap, fill=t["right"], stroke=t["ink"], sw=1.2))
    c.add(poly([m(0, 0), m(1.3, -0.7), m(1.3, 0.7)], fill=t["ink"], stroke=t["ink"], sw=0.4))


def ink_dot(c, p, r=1.6):
    c.add(circle(p[0], p[1], r, fill=c.t["ink"], stroke=c.t["bg"], sw=0.7))


# -------------------------------------------------------------------- sketch
@icon("sketch", "sketch", "Sketch")
def _(c, t):
    iso = Iso(k=1.0)
    pts = [(0, 0, 0), (15, 0, 0), (15, 15, 0), (0, 15, 0)]
    iso.fit(pts, 14.5, 20.5)
    pl = [iso.p(*q) for q in pts]
    c.add(poly(pl, fill=t["top"], stroke=t["ink"], sw=1.5))
    T = iso.top(0)
    # sketch curve on the plane: rounded profile
    d, e, tn = arc_d(T, 6, 8, 3.5, 270, 360)
    prof = f"M{pt(T(3,3))} L{pt(T(6,3))} " + arc_d(T, 6, 6.5, 3.5, 270, 360, move=False)[0] + f" L{pt(T(9.5,11))} L{pt(T(3,11))} Z"
    c.add(P(prof, stroke=t["acc"], sw=1.9, fill=t["acc_soft"], fop=0.55))
    pencil(c, iso.p(9.5, 5.2, 0), -50, L=13, w=4.6)


@icon("sketch", "line", "Line")
def _(c, t):
    c.add(line((6, 25), (25, 7), t["acc"], GL + 0.4))
    ink_dot(c, (6, 25), 2.0)
    ink_dot(c, (25, 7), 2.0)


@icon("sketch", "centerline", "Centerline")
def _(c, t):
    c.add(line((5, 24), (27, 8), t["acc"], GL, dash="7 2.2 1.4 2.2", cap="butt"))
    ink_dot(c, (5, 24), 2.0)
    ink_dot(c, (27, 8), 2.0)


@icon("sketch", "rectangle", "Rectangle")
def _(c, t):
    c.add(rect(6, 9, 20, 14, fill=t["acc_soft"], fop=0.5, stroke=t["acc"], sw=GL))
    for p in ((6, 9), (26, 9), (6, 23), (26, 23)):
        ink_dot(c, p, 1.9)


@icon("sketch", "slot", "Slot")
def _(c, t):
    c.add(f'<g transform="rotate(-30 16 16)">' + rect(4.5, 10.5, 23, 11, r=5.5, fill=t["acc_soft"], fop=0.5, stroke=t["acc"], sw=GL) + "</g>")
    a, b = rot((10, 16), -30, (16, 16)), rot((22, 16), -30, (16, 16))
    c.add(line(a, b, t["ink"], 1.2, dash=DASHDOT, cap="butt"))
    ink_dot(c, a, 1.8)
    ink_dot(c, b, 1.8)


@icon("sketch", "circle-center", "Circle (Center + Point)")
def _(c, t):
    c.add(circle(16, 16, 10, fill=t["acc_soft"], fop=0.5, stroke=t["acc"], sw=GL))
    p = (16 + 10 * math.cos(math.radians(-35)), 16 + 10 * math.sin(math.radians(-35)))
    c.add(line((16, 16), p, t["ink"], 1.2, dash=DASH))
    ink_dot(c, (16, 16), 1.9)
    ink_dot(c, p, 1.9)


@icon("sketch", "circle-3point", "Circle (3 Points)")
def _(c, t):
    c.add(circle(16, 16, 10, fill=t["acc_soft"], fop=0.5, stroke=t["acc"], sw=GL))
    for a in (200, 320, 85):
        ink_dot(c, (16 + 10 * math.cos(math.radians(a)), 16 + 10 * math.sin(math.radians(a))), 1.9)


@icon("sketch", "ellipse", "Ellipse")
def _(c, t):
    c.add(ellipse(16, 16, 12, 7, rot=-30, fill=t["acc_soft"], fop=0.5, stroke=t["acc"], sw=GL))
    a, b = rot((4, 16), -30, (16, 16)), rot((28, 16), -30, (16, 16))
    c.add(line(a, b, t["ink"], 1.2, dash=DASHDOT, cap="butt"))
    ink_dot(c, (16, 16), 1.8)
    ink_dot(c, rot((16, 9), -30, (16, 16)), 1.6)


@icon("sketch", "arc-3point", "Arc (3 Points)")
def _(c, t):
    d, e, tn = arc_d(A, 16, 21, 12, 205, 335)
    c.add(P(d, stroke=t["acc"], sw=GL + 0.3))
    for a in (205, 270, 335):
        ink_dot(c, (16 + 12 * math.cos(math.radians(a)), 21 + 12 * math.sin(math.radians(a))), 1.9)


@icon("sketch", "arc-center", "Arc (Center + Ends)")
def _(c, t):
    cx, cy, r = 16, 24, 17
    d, e, tn = arc_d(A, cx, cy, r, 232, 308)
    c.add(P(d, stroke=t["acc"], sw=GL + 0.3))
    for a in (232, 308):
        p = (cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a)))
        c.add(line((cx, cy), p, t["ink"], 1.2, dash=DASH))
        ink_dot(c, p, 1.9)
    ink_dot(c, (cx, cy), 1.9)


@icon("sketch", "arc-tangent", "Arc (Tangent End)")
def _(c, t):
    c.add(line((4, 25), (14, 25), t["ink"], 2.0))
    d, e, tn = arc_d(A, 14, 14, 11, 90, 0)
    c.add(P(d, stroke=t["acc"], sw=GL + 0.3))
    ink_dot(c, (14, 25), 1.9)
    ink_dot(c, (25, 14), 1.9)
    ink_dot(c, (4, 25), 1.9)


@icon("sketch", "spline", "Spline")
def _(c, t):
    pts = [(5, 22), (12, 8), (20, 24), (27, 9)]
    c.add(P(catmull(pts), stroke=t["acc"], sw=GL + 0.2))
    for p in pts:
        ink_dot(c, p, 1.9)


@icon("sketch", "polygon", "Polygon")
def _(c, t):
    pts = [(16 + 11 * math.cos(math.radians(60 * i - 90)), 16 + 11 * math.sin(math.radians(60 * i - 90))) for i in range(6)]
    c.add(poly(pts, fill=t["acc_soft"], stroke=t["acc"], sw=GL, op=1, fop=0.5))
    ink_dot(c, (16, 16), 1.9)
    ink_dot(c, pts[0], 1.7)


@icon("sketch", "point", "Point")
def _(c, t):
    c.add(line((7, 7), (25, 25), t["acc"], 2.6), line((25, 7), (7, 25), t["acc"], 2.6))
    c.add(circle(16, 16, 1.6, fill=t["ink"]))


@icon("sketch", "sketch-csys", "Sketch Coordinate System")
def _(c, t):
    o = (9, 23)
    c.add(arrow(o, (27, 23), t["ink"], 1.8))
    c.add(arrow(o, (9, 5), t["ink"], 1.8))
    c.add(circle(o[0], o[1], 2.6, fill=t["acc"], stroke=t["bg"], sw=0.8))
    # tick marks
    c.add(line((18, 21), (18, 25), t["ink"], 1.2), line((7, 14), (11, 14), t["ink"], 1.2))


@icon("sketch", "sketch-fillet", "Sketch Fillet")
def _(c, t):
    c.add(line((8, 5), (8, 18), t["ink"], 1.9))
    c.add(line((14, 24), (27, 24), t["ink"], 1.9))
    c.add(polyline([(8, 18), (8, 24), (14, 24)], t["ink"], 1.2, dash=DASH, op=0.6))
    d, e, tn = arc_d(A, 14, 18, 6, 180, 90)
    c.add(P(d, stroke=t["acc"], sw=GL + 0.4))
    ink_dot(c, (8, 18), 1.7)
    ink_dot(c, (14, 24), 1.7)


@icon("sketch", "sketch-chamfer", "Sketch Chamfer")
def _(c, t):
    c.add(line((8, 5), (8, 17), t["ink"], 1.9))
    c.add(line((15, 24), (27, 24), t["ink"], 1.9))
    c.add(polyline([(8, 17), (8, 24), (15, 24)], t["ink"], 1.2, dash=DASH, op=0.6))
    c.add(line((8, 17), (15, 24), t["acc"], GL + 0.4))
    ink_dot(c, (8, 17), 1.7)
    ink_dot(c, (15, 24), 1.7)


@icon("sketch", "trim", "Trim")
def _(c, t):
    c.add(line((16, 4), (16, 28), t["ink"], 1.9))
    c.add(line((4, 16), (16, 16), t["ink"], 1.9))
    c.add(line((16, 16), (28, 16), t["cut"], 1.9, dash="2.6 2.2"))
    c.add(line((23.5, 21), (28.5, 26), t["cut"], 2.0), line((28.5, 21), (23.5, 26), t["cut"], 2.0))
    ink_dot(c, (16, 16), 1.8)


@icon("sketch", "corner-trim", "Corner Trim")
def _(c, t):
    c.add(polyline([(6, 26), (6, 10), (22, 10)], t["acc"], 2.2, join="round"))
    c.add(line((6, 10), (6, 5), t["cut"], 1.8, dash="2.2 2"))
    c.add(line((22, 10), (28, 10), t["cut"], 1.8, dash="2.2 2"))
    ink_dot(c, (6, 10), 1.9)
    c.add(line((10, 22), (15, 27), t["cut"], 2.0), line((15, 22), (10, 27), t["cut"], 2.0))


@icon("sketch", "divide", "Divide")
def _(c, t):
    c.add(line((4, 20), (13, 20), t["ink"], 1.9))
    c.add(line((19, 20), (28, 20), t["ink"], 1.9))
    c.add(line((16, 7), (16, 26), t["acc"], 1.6, dash="2.4 2"))
    c.add(circle(16, 20, 2.4, fill=t["acc"], stroke=t["bg"], sw=0.8))
    c.add(line((13, 17), (13, 23), t["ink"], 1.4), line((19, 17), (19, 23), t["ink"], 1.4))


@icon("sketch", "extend-line", "Extend Line")
def _(c, t):
    c.add(line((24.5, 4), (24.5, 28), t["ink"], 2.6))
    c.add(line((3.5, 16), (12, 16), t["ink"], 1.9))
    c.add(arrow((12, 16), (22.5, 16), t["acc"], 2.0, size=4.6, half=2.7))
    ink_dot(c, (3.5, 16), 1.9)
    ink_dot(c, (12, 16), 1.9)


@icon("sketch", "offset-edge", "Offset Edge")
def _(c, t):
    # original (ink) rounded L, offset (accent) concentric inside
    d1 = f"M7 5 L7 18 " + arc_d(A, 15, 18, 8, 180, 90, move=False)[0] + " L27 26"
    d2 = f"M13 5 L13 18 " + arc_d(A, 15, 18, 2, 180, 90, move=False)[0] + " L27 20"
    c.add(P(d1, stroke=t["ink"], sw=1.9))
    c.add(P(d2, stroke=t["acc"], sw=GL + 0.2))
    c.add(arrow((20, 26), (20, 21.2), t["mute"], 1.2, size=3, half=1.6))


@icon("sketch", "thicken-edge", "Thicken Edge")
def _(c, t):
    d = ("M5 5 L5 18 " + arc_d(A, 15, 18, 10, 180, 90, move=False)[0] + " L27 28 L27 22 L15 22 "
         + arc_d(A, 15, 18, 4, 90, 180, move=False)[0] + " L11 5 Z")
    c.add(P(d, fill=t["acc_soft"], stroke=t["acc"], sw=GL - 0.2, fop=0.8))
    mid = "M8 5 L8 18 " + arc_d(A, 15, 18, 7, 180, 90, move=False)[0] + " L27 25"
    c.add(P(mid, stroke=t["ink"], sw=1.2, dash=DASH))


@icon("sketch", "mirror-sketch", "Mirror (Sketch)")
def _(c, t):
    c.add(line((16, 3), (16, 29), t["ink"], 1.3, dash=DASHDOT, cap="butt"))
    c.add(poly([(5, 9), (12.5, 9), (5, 24)], fill=t["top"], stroke=t["ink"], sw=1.6))
    c.add(poly([(27, 9), (19.5, 9), (27, 24)], fill=t["acc_soft"], stroke=t["acc"], sw=1.9))


@icon("sketch", "project-edge", "Project / Use Edge")
def _(c, t):
    # ground plane
    plane = [(16, 15), (29, 21.5), (16, 28), (3, 21.5)]
    c.add(poly(plane, fill=t["top"], stroke=t["ink"], sw=1.4))
    c.add(line((9, 8), (23, 8), t["ink"], 2.2))
    ink_dot(c, (9, 8), 1.8)
    ink_dot(c, (23, 8), 1.8)
    c.add(line((9, 10.5), (9, 19.5), t["acc"], 1.2, dash="2 2"), line((23, 10.5), (23, 19.5), t["acc"], 1.2, dash="2 2"))
    c.add(line((9, 21), (23, 21), t["acc"], 2.2))


@icon("sketch", "text", "Text")
def _(c, t):
    c.add(polyline([(6, 21), (14, 4.5), (22, 21)], t["ink"], 2.4))
    c.add(line((9.2, 15), (18.8, 15), t["ink"], 2.2))
    c.add(line((5, 26.5), (27, 26.5), t["acc"], 2.2))
    ink_dot(c, (5, 26.5), 1.6)
    ink_dot(c, (27, 26.5), 1.6)


@icon("sketch", "dimension", "Dimension")
def _(c, t):
    c.add(line((6, 6), (6, 19), t["ink"], 1.3), line((26, 6), (26, 19), t["ink"], 1.3))
    c.add(arrow((7, 12), (25, 12), t["acc"], 1.7, two=True, size=4.2, half=2.2))
    c.add(rect(13, 8, 6, 7, fill=t["bg"], stroke=None))
    c.add(line((14.2, 12), (17.8, 12), t["ink"], 1.4))
    c.add(rect(6, 21, 20, 6, fill=t["top"], stroke=t["ink"], sw=1.4))


@icon("sketch", "construction", "Construction Mode")
def _(c, t):
    c.add(line((5, 9), (27, 9), t["ink"], 2.2))
    c.add(line((5, 23), (27, 23), t["acc"], 2.0, dash="6 2 1.4 2", cap="butt"))
    c.add(arrow((16, 12.5), (16, 19.5), t["mute"], 1.4, two=True, size=3.4, half=1.9))
