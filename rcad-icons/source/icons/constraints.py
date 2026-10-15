"""Sketch constraints."""
import math
from kit import *  # noqa: F401,F403
from kit import icon, arc_d, SCREEN
from icons.sketch import ink_dot, rot

A = SCREEN
G = 2.1


def H(c, x, y, col, s=1.0, w=1.7):
    """letter H, top-left at x,y"""
    c.add(line((x, y), (x, y + 8 * s), col, w), line((x + 6 * s, y), (x + 6 * s, y + 8 * s), col, w),
          line((x, y + 4 * s), (x + 6 * s, y + 4 * s), col, w))


def V(c, x, y, col, s=1.0, w=1.7):
    c.add(polyline([(x, y), (x + 3.5 * s, y + 8 * s), (x + 7 * s, y)], col, w))


@icon("constraints", "horizontal", "Horizontal")
def _(c, t):
    H(c, 13, 5, t["ink"])
    c.add(line((5, 21), (27, 21), t["acc"], G + 0.3))
    ink_dot(c, (5, 21), 1.9)
    ink_dot(c, (27, 21), 1.9)
    c.add(line((5, 26.5), (27, 26.5), t["mute"], 1.1, dash=DASH))


@icon("constraints", "vertical", "Vertical")
def _(c, t):
    V(c, 21, 5, t["ink"])
    c.add(line((10, 5), (10, 27), t["acc"], G + 0.3))
    ink_dot(c, (10, 5), 1.9)
    ink_dot(c, (10, 27), 1.9)
    c.add(line((5.5, 5), (5.5, 27), t["mute"], 1.1, dash=DASH))


@icon("constraints", "perpendicular", "Perpendicular")
def _(c, t):
    c.add(line((4, 25), (28, 25), t["acc"], G + 0.2))
    c.add(line((16, 25), (16, 5), t["ink"], G))
    c.add(polyline([(16, 20), (21, 20), (21, 25)], t["ink"], 1.4))
    ink_dot(c, (16, 5), 1.8)


@icon("constraints", "parallel", "Parallel")
def _(c, t):
    c.add(line((4, 20), (20, 4), t["acc"], G + 0.2))
    c.add(line((12, 28), (28, 12), t["ink"], G + 0.2))
    c.add(arrow((9, 12.5), (13.4, 8.1), t["mute"], 1.2, size=3, half=1.6))
    c.add(arrow((17, 20.5), (21.4, 16.1), t["mute"], 1.2, size=3, half=1.6))


@icon("constraints", "tangent", "Tangent")
def _(c, t):
    c.add(circle(16, 19.5, 8, fill=t["top"], stroke=t["ink"], sw=1.9))
    c.add(line((3.5, 11.5), (28.5, 11.5), t["acc"], G + 0.2))
    c.add(circle(16, 11.5, 2.3, fill=t["acc"], stroke=t["bg"], sw=0.8))


@icon("constraints", "midpoint", "Midpoint")
def _(c, t):
    c.add(line((4, 23), (28, 23), t["ink"], G))
    c.add(line((10, 20), (10, 26), t["mute"], 1.3), line((22, 20), (22, 26), t["mute"], 1.3))
    c.add(line((16, 9), (16, 20), t["acc"], 1.5, dash="2.2 2"))
    c.add(circle(16, 23, 2.8, fill=t["acc"], stroke=t["bg"], sw=0.9))
    ink_dot(c, (16, 7.5), 2.0)
    ink_dot(c, (4, 23), 1.8)
    ink_dot(c, (28, 23), 1.8)


@icon("constraints", "coincident", "Coincident")
def _(c, t):
    c.add(line((16, 16), (4, 8), t["ink"], G))
    c.add(line((16, 16), (28, 8), t["ink"], G))
    c.add(line((16, 16), (16, 28), t["ink"], G))
    c.add(circle(16, 16, 4.3, fill=t["bg"], stroke=t["acc"], sw=2.0))
    c.add(circle(16, 16, 1.6, fill=t["acc"]))


@icon("constraints", "equal", "Equal")
def _(c, t):
    c.add(line((4, 24), (15, 24), t["ink"], G + 0.2))
    c.add(line((26, 4), (26, 15), t["ink"], G + 0.2))
    c.add(line((14.5, 9), (21.5, 9), t["acc"], 2.4), line((14.5, 14), (21.5, 14), t["acc"], 2.4))
    ink_dot(c, (4, 24), 1.7)
    ink_dot(c, (15, 24), 1.7)
    ink_dot(c, (26, 4), 1.7)
    ink_dot(c, (26, 15), 1.7)


@icon("constraints", "symmetric", "Symmetric")
def _(c, t):
    c.add(line((16, 3), (16, 29), t["ink"], 1.4, dash=DASHDOT, cap="butt"))
    for y in (10, 22):
        c.add(arrow((16, y), (6.2, y), t["acc"], 1.4, two=False, size=3.2, half=1.8))
        c.add(arrow((16, y), (25.8, y), t["acc"], 1.4, two=False, size=3.2, half=1.8))
        ink_dot(c, (5, y), 2.0)
        ink_dot(c, (27, y), 2.0)


@icon("constraints", "concentric", "Concentric")
def _(c, t):
    c.add(circle(16, 16, 11, fill=t["top"], stroke=t["ink"], sw=1.9))
    c.add(circle(16, 16, 5.3, fill=t["bg"], stroke=t["acc"], sw=2.1))
    c.add(circle(16, 16, 1.5, fill=t["acc"]))


@icon("constraints", "fix", "Fix / Lock")
def _(c, t):
    d, e, tn = arc_d(A, 16, 14, 5.5, 180, 360)
    c.add(P(d + " L21.5 16 M10.5 14 L10.5 16", stroke=t["ink"], sw=2.2, cap="round"))
    c.add(rect(7, 14.5, 18, 13, r=2.5, fill=t["left"], stroke=t["ink"], sw=1.8))
    c.add(circle(16, 20, 2.0, fill=t["ink"]))
    c.add(line((16, 21), (16, 24), t["ink"], 1.8))


@icon("constraints", "collinear", "Collinear")
def _(c, t):
    c.add(line((4, 28), (14, 18), t["ink"], G))
    c.add(line((18, 14), (28, 4), t["ink"], G))
    c.add(line((14, 18), (18, 14), t["acc"], 1.8, dash="1.6 2", cap="butt"))
    for p in ((4, 28), (14, 18), (18, 14), (28, 4)):
        ink_dot(c, p, 1.7)
