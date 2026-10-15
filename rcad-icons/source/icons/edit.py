"""Editing operations: mirror, pattern, boolean, move..."""
import math
from kit import *  # noqa: F401,F403
from kit import (icon, arc_d, SCREEN, Iso, mix, box, cyl, box_ghost, solid_plan, pocket, hole_at, ellipse_d,
                 gopen, gclose)
from icons.features import fitted, arc_plan, curved_arrow, hole_colors
from icons.datum import plane_poly


@icon("edit", "mirror", "Mirror")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 9, 0), (0, 9, 0), (14, 0, 0), (0, 0, 9)], 16, 17.5)
    box(c, iso, 0, 0, 0, 4.5, 9, 7)
    # mirror plane between the two bodies (x = 7)
    p = iso.p
    plane_poly(c, [p(7, -1.5, -1.5), p(7, 10.5, -1.5), p(7, 10.5, 9.5), p(7, -1.5, 9.5)])
    ghost = (mix(t["top"], t["bg"], 0.55), mix(t["left"], t["bg"], 0.5), mix(t["right"], t["bg"], 0.5))
    box(c, iso, 9.5, 0, 0, 14, 9, 7, fills=ghost, stroke=t["mute"])


@icon("edit", "pattern-direction", "Pattern (Direction)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (21, 5, 0), (0, 5, 0), (21, 0, 0), (0, 0, 5)], 16, 18)
    for i in range(3):
        x0 = i * 8.2
        box(c, iso, x0, 0, 0, x0 + 5, 5, 5)
    a, b = iso.p(0, 7.4, 0), iso.p(21.4, 7.4, 0)
    c.add(arrow((a[0] - 1, a[1] + 1), (b[0] - 0.4, b[1] + 1), t["acc"], 2.0, size=4.4, half=2.5))


@icon("edit", "pattern-axis", "Pattern (Axis)")
def _(c, t):
    iso = Iso(1.0, 16, 17)
    cx, cy = 0, 0
    R = 8.2
    items = []
    for i in range(6):
        a = math.radians(60 * i + 20)
        items.append((cx + R * math.cos(a) + cy * 0, cy + R * math.sin(a)))
    iso = Iso(0.95, 16, 17)
    s = 3.8
    for (x, y) in sorted(items, key=lambda p: p[0] + p[1]):
        box(c, iso, x - s / 2, y - s / 2, 0, x + s / 2, y + s / 2, s, sw=1.3)
    ox, oy = iso.p(0, 0, 0)
    c.add(line((ox, oy - 11), (ox, oy + 11), t["acc"], 1.4, dash=DASHDOT, cap="butt", op=0.9))
    c.add(circle(ox, oy, 1.8, fill=t["acc"], stroke=t["bg"], sw=0.7))


@icon("edit", "pattern-fill", "Pattern (Fill)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (16, 16, 0), (16, 0, 0), (0, 16, 0)], 16, 16)
    c.add(poly([iso.p(0, 0, 0), iso.p(16, 0, 0), iso.p(16, 16, 0), iso.p(0, 16, 0)], fill=t["top"], stroke=t["ink"], sw=1.5))
    c.add(poly([iso.p(1.5, 1.5, 0), iso.p(14.5, 1.5, 0), iso.p(14.5, 14.5, 0), iso.p(1.5, 14.5, 0)], stroke=t["acc"], sw=1.3, dash="2.4 2"))
    for i in range(3):
        for j in range(3):
            x, y = iso.p(3.5 + i * 4.5, 3.5 + j * 4.5, 0)
            c.add(ellipse(x, y, 2.0, 1.2, fill=mix(t["hole"], t["top"], 0.15), stroke=t["ink"], sw=0.9))


@icon("edit", "pattern-curve", "Pattern (Along Curve)")
def _(c, t):
    d = "M4 24 C8 6 22 28 28 8"
    c.add(P(d, stroke=t["acc"], sw=1.5, dash="2 2.2"))
    iso = Iso(0.8, 0, 0)
    for u in (0.04, 0.3, 0.55, 0.78, 0.97):
        p = bez((4, 24), (8, 6), (22, 28), (28, 8), u)
        s = 3.4
        iso.ox, iso.oy = p[0], p[1] + 2.2
        box(c, iso, -s / 2, -s / 2, 0, s / 2, s / 2, s, sw=1.2)


@icon("edit", "move", "Move / Transform")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (8, 8, 0), (8, 0, 0), (0, 8, 0), (0, 0, 8)], 16, 16)
    box(c, iso, 0, 0, 0, 8, 8, 8)
    for (a, b) in (((16, 3.8), (16, 0.6)), ((16, 28.2), (16, 31.4)), ((3.8, 16), (0.6, 16)), ((28.2, 16), (31.4, 16))):
        pass
    c.add(arrow((16, 8.2), (16, 1.2), t["acc"], 1.8, size=3.8, half=2.2))
    c.add(arrow((16, 23.8), (16, 30.8), t["acc"], 1.8, size=3.8, half=2.2))
    c.add(arrow((8.2, 16), (1.2, 16), t["acc"], 1.8, size=3.8, half=2.2))
    c.add(arrow((23.8, 16), (30.8, 16), t["acc"], 1.8, size=3.8, half=2.2))


@icon("edit", "rotate-part", "Rotate")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (8, 8, 0), (8, 0, 0), (0, 8, 0), (0, 0, 8)], 16, 16.5)
    box(c, iso, 0, 0, 0, 8, 8, 8)
    curved_arrow(c, 16, 16, 13, 6.2, 200, 340, t["acc"], 2.0)
    curved_arrow(c, 16, 17.5, 13, 6.2, 20, 160, t["acc"], 2.0)


@icon("edit", "boolean-union", "Merge (Union)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 11)], 16, 17)
    box(c, iso, 0, 0, 0, 9, 9, 9)
    box(c, iso, 5, 5, 2, 14, 14, 11)


@icon("edit", "boolean-subtract", "Cut (Subtract)")
def _(c, t):
    plan = [(0, 0), (13, 0), (13, 6), (7, 6), (7, 13), (0, 13)]
    iso = fitted(1.05, [(x, y, 0) for x, y in plan] + [(x, y, 10) for x, y in plan], 16, 16.5)
    solid_plan(c, iso, plan, 0, 10, tint={2, 3}, tint_cols=(t["cut_left"], t["cut_right"]))
    box_ghost(c, iso, 7, 6, 0, 13, 13, 10, color=t["cut"], sw=1.2, dash="2.2 1.8", op=0.95)


@icon("edit", "boolean-intersect", "Intersect (Common)")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (14, 14, 0), (14, 0, 0), (0, 14, 0), (0, 0, 11)], 16, 17)
    box_ghost(c, iso, 0, 0, 0, 9, 9, 9, color=t["mute"], sw=1.3, hidden=False)
    box_ghost(c, iso, 5, 5, 2, 14, 14, 11, color=t["mute"], sw=1.3, hidden=False)
    box(c, iso, 5, 5, 2, 9, 9, 9, fills=(t["hl_top"], t["hl_left"], t["hl_right"]))


@icon("edit", "split", "Split Body")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (15, 9, 0), (0, 9, 0), (15, 0, 0), (0, 0, 9)], 16, 17.5)
    box(c, iso, 0, 0, 0, 5.5, 9, 8)
    p = iso.p
    plane_poly(c, [p(7.5, -1.5, -1.5), p(7.5, 10.5, -1.5), p(7.5, 10.5, 9.5), p(7.5, -1.5, 9.5)])
    box(c, iso, 9.5, 0, 0, 15, 9, 8)


@icon("edit", "scale-model", "Scale")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (7, 7, 0), (7, 0, 0), (0, 7, 0), (0, 0, 7)], 12, 20)
    box(c, iso, 0, 0, 0, 7, 7, 7)
    c.add(rect(15, 4, 13, 13, r=1.5, stroke=t["ink"], sw=1.5, dash=DASH))
    c.add(arrow((11.5, 18), (26.5, 5.5), t["acc"], 2.2, size=4.8, half=2.8))


@icon("edit", "flip-direction", "Flip Direction")
def _(c, t):
    c.add(arrow((16, 16), (16, 3), t["acc"], 2.2, size=4.8, half=2.8))
    c.add(arrow((16, 16), (16, 29), t["mute"], 2.2, size=4.8, half=2.8))
    c.add(circle(16, 16, 2.2, fill=t["ink"], stroke=t["bg"], sw=0.8))
    d, e, tn = arc_d(SCREEN, 16, 16, 8.5, 300, 400)
    c.add(P(d, stroke=t["ink"], sw=1.4, dash="2 2"))


@icon("edit", "offset-solid", "Offset Faces")
def _(c, t):
    plan = [(0, 0), (13, 0), (13, 13), (0, 13)]
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 14)], 16, 17.5)
    box(c, iso, 0, 0, 0, 13, 13, 7)
    box(c, iso, 3, 3, 7, 10, 10, 11, fills=(t["hl_top"], t["hl_left"], t["hl_right"]))
    up = iso.p(6.5, 6.5, 11)
    c.add(arrow((up[0], up[1] - 0.5), (up[0], up[1] - 6.5), t["acc"], 1.8, size=3.8, half=2.2))


@icon("edit", "edit-definition", "Edit Definition")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (11, 11, 0), (11, 0, 0), (0, 11, 0), (0, 0, 9)], 12, 19)
    box(c, iso, 0, 0, 0, 11, 11, 9)
    from icons.sketch import pencil
    pencil(c, (17.5, 20.5), -45, L=15, w=5.2)


@icon("edit", "suppress", "Suppress")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 11)], 16, 17)
    ghost = (mix(t["top"], t["bg"], 0.65), mix(t["left"], t["bg"], 0.65), mix(t["right"], t["bg"], 0.65))
    box(c, iso, 0, 0, 0, 13, 13, 11, fills=ghost, stroke=t["mute"], sw=1.4)
    c.add(line((6, 6), (26, 26), t["cut"], 2.4), line((26, 6), (6, 26), t["cut"], 2.4))


@icon("edit", "resume", "Resume")
def _(c, t):
    iso = fitted(1.0, [(0, 0, 0), (13, 13, 0), (13, 0, 0), (0, 13, 0), (0, 0, 11)], 15, 16.5)
    box(c, iso, 0, 0, 0, 13, 13, 11)
    c.add(circle(24, 24, 6.2, fill=t["acc"], stroke=t["bg"], sw=1.0))
    c.add(poly([(22, 20.8), (22, 27.2), (27.4, 24)], fill=t["white"], stroke=t["white"], sw=0.8))
