"""View orientation, zoom/pan/orbit, display styles, visibility toggles."""
import math
from kit import *  # noqa: F401,F403
from kit import icon, arc_d, SCREEN, Iso, mix, box, solid_plan, REGISTRY
from icons.features import fitted, curved_arrow, arc_plan
from icons import datum as D

S_ = 12.0


def cube_iso():
    return fitted(1.1, [(0, 0, 0), (S_, S_, 0), (S_, 0, 0), (0, S_, 0), (0, 0, S_)], 16, 16.5)


def faces(iso):
    p, S = iso.p, S_
    return {
        "top": [p(0, 0, S), p(S, 0, S), p(S, S, S), p(0, S, S)],
        "front": [p(0, S, 0), p(S, S, 0), p(S, S, S), p(0, S, S)],
        "right": [p(S, 0, 0), p(S, S, 0), p(S, S, S), p(S, 0, S)],
        "back": [p(0, 0, 0), p(S, 0, 0), p(S, 0, S), p(0, 0, S)],
        "left": [p(0, 0, 0), p(0, S, 0), p(0, S, S), p(0, 0, S)],
        "bottom": [p(0, 0, 0), p(S, 0, 0), p(S, S, 0), p(0, S, 0)],
    }


def view_cube(c, which):
    t = c.t
    iso = cube_iso()
    F = faces(iso)
    hidden = which in ("back", "left", "bottom")
    col = {"top": t["hl_top"], "front": t["hl_left"], "right": t["hl_right"]}
    if hidden:
        c.add(poly(F[which], fill=t["acc"], fop=0.9, stroke=t["acc"], sw=1.6))
    base = {"top": t["top"], "front": t["left"], "right": t["right"]}
    for k in ("front", "right", "top"):
        fill = col[k] if which == k else base[k]
        c.add(poly(F[k], fill=fill, fop=0.45 if hidden else None, stroke=t["ink"], sw=SW))
    if hidden:
        p, S = iso.p, S_
        o = p(0, 0, 0)
        for q in (p(S, 0, 0), p(0, S, 0), p(0, 0, S)):
            c.add(line(o, q, t["ink"], 1.2, dash=DASH, op=0.9))


for _n, _l in (("top", "Top"), ("front", "Front"), ("right", "Right"), ("back", "Back"), ("left", "Left"), ("bottom", "Bottom")):
    icon("view", f"view-{_n}", f"{_l} View")(lambda c, t, _n=_n: view_cube(c, _n))


@icon("view", "view-isometric", "Isometric View")
def _(c, t):
    iso = cube_iso()
    box(c, iso, 0, 0, 0, S_, S_, S_)
    x, y = iso.p(S_, S_, 0)
    c.add(circle(x, y, 2.6, fill=t["acc"], stroke=t["bg"], sw=0.9))


@icon("view", "view-perspective", "Perspective View")
def _(c, t):
    back = [(11, 8), (24, 8), (24, 19), (11, 19)]
    front = [(5, 13), (22, 13), (22, 29), (5, 29)]
    c.add(poly([back[1], front[1], front[2], back[2]], fill=t["right"], stroke=t["ink"], sw=SW))
    c.add(poly([back[0], back[1], front[1], front[0]], fill=t["top"], stroke=t["ink"], sw=SW))
    c.add(poly(front, fill=t["left"], stroke=t["ink"], sw=SW))


@icon("view", "view-normal-to", "Normal To (Sketch View)")
def _(c, t):
    c.add(rect(5, 5, 22, 22, r=2.2, fill=t["top"], stroke=t["ink"], sw=1.7))
    c.add(circle(16, 16, 6.5, stroke=t["acc"], sw=2.0))
    c.add(circle(16, 16, 2.4, fill=t["acc"]))


@icon("view", "view-saved", "Saved Views")
def _(c, t):
    c.add(rect(4, 9, 24, 17, r=3, fill=t["top"], stroke=t["ink"], sw=1.7))
    c.add(polyline([(10, 9), (12, 5.5), (20, 5.5), (22, 9)], t["ink"], 1.7))
    c.add(circle(16, 17.5, 5.2, fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(circle(16, 17.5, 1.9, fill=t["acc"]))
    c.add(circle(24, 12.6, 1.1, fill=t["acc"]))


@icon("view", "view-orbit", "Orbit (Spin)")
def _(c, t):
    c.add(circle(16, 16, 7, fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(ellipse(13.3, 12.6, 2.4, 1.6, rot=-35, fill=t["top"], op=0.85))
    d, e, tn = arc_d(SCREEN, 16, 16, 5, 160, 20, ru=14)
    d, e, tn = arc_d(SCREEN, 16, 16, 5.5, 168, 372, ru=13.5)
    c.add(path_arrow(d, e, tn, t["acc"], 2.0, 4.4, 2.5))


@icon("view", "view-pan", "Pan")
def _(c, t):
    for a in range(4):
        ang = math.radians(90 * a - 90)
        dx, dy = math.cos(ang), math.sin(ang)
        c.add(arrow((16 + dx * 3, 16 + dy * 3), (16 + dx * 13.5, 16 + dy * 13.5), t["ink"], 2.0, size=4.6, half=2.7))
    c.add(circle(16, 16, 2.6, fill=t["acc"], stroke=t["bg"], sw=0.8))


@icon("view", "view-spin-center", "Spin Center")
def _(c, t):
    c.add(circle(16, 16, 2.2, fill=t["acc"], stroke=t["bg"], sw=0.8))
    d, e, tn = arc_d(SCREEN, 16, 16, 9.5, 200, 520)
    c.add(path_arrow(d, e, tn, t["ink"], 1.9, 4.2, 2.4))
    c.add(line((16, 3), (16, 6.5), t["ink"], 1.4), line((16, 25.5), (16, 29), t["ink"], 1.4))
    c.add(line((3, 16), (6.5, 16), t["ink"], 1.4), line((25.5, 16), (29, 16), t["ink"], 1.4))


def glass(c, extra=None):
    t = c.t
    c.add(line((19.5, 19.5), (27.5, 27.5), t["ink"], 3.6))
    c.add(circle(13.5, 13.5, 9, fill=t["top"], fop=0.8, stroke=t["ink"], sw=2.0))
    c.add(P("M8 12 A6 6 0 0 1 12 8", stroke=t["bg"], sw=1.4, op=0.9))


@icon("view", "zoom-in", "Zoom In")
def _(c, t):
    glass(c)
    c.add(line((9.2, 13.5), (17.8, 13.5), t["acc"], 2.2), line((13.5, 9.2), (13.5, 17.8), t["acc"], 2.2))


@icon("view", "zoom-out", "Zoom Out")
def _(c, t):
    glass(c)
    c.add(line((9.2, 13.5), (17.8, 13.5), t["acc"], 2.2))


@icon("view", "zoom-fit", "Refit (Zoom to Fit)")
def _(c, t):
    for sx, sy in ((1, 1), (-1, 1), (1, -1), (-1, -1)):
        x, y = 16 + sx * 12.5, 16 + sy * 12.5
        c.add(polyline([(x - sx * 6, y), (x, y), (x, y - sy * 6)], t["ink"], 2.2))
    iso = fitted(0.7, [(0, 0, 0), (8, 8, 0), (8, 0, 0), (0, 8, 0), (0, 0, 8)], 16, 16)
    box(c, iso, 0, 0, 0, 8, 8, 8, sw=1.3)


@icon("view", "zoom-window", "Zoom to Window")
def _(c, t):
    c.add(rect(3.5, 3.5, 15, 13, r=0.5, stroke=t["acc"], sw=1.7, dash="3 2"))
    c.add(line((20.5, 20.5), (28, 28), t["ink"], 3.4))
    c.add(circle(15.5, 15.5, 7.6, fill=t["top"], fop=0.85, stroke=t["ink"], sw=2.0))


# ----------------------------------------------------------- display styles
def style_cube(c, mode):
    t = c.t
    iso = cube_iso()
    F = faces(iso)
    p, S = iso.p, S_
    hid = [(p(0, 0, 0), p(S, 0, 0)), (p(0, 0, 0), p(0, S, 0)), (p(0, 0, 0), p(0, 0, S))]
    if mode == "wireframe":
        for a, b in hid:
            c.add(line(a, b, t["ink"], 1.6))
        for k in ("front", "right", "top"):
            c.add(poly(F[k], stroke=t["ink"], sw=1.6))
    elif mode == "hidden":
        for a, b in hid:
            c.add(line(a, b, t["ink"], 1.3, dash=DASH, op=0.85))
        for k in ("front", "right", "top"):
            c.add(poly(F[k], stroke=t["ink"], sw=1.7))
    elif mode == "nohidden":
        for k in ("front", "right", "top"):
            c.add(poly(F[k], stroke=t["ink"], sw=1.8))
    elif mode == "shaded":
        for k, col in (("front", t["left"]), ("right", t["right"]), ("top", t["top"])):
            c.add(poly(F[k], fill=col, stroke=col, sw=0.8))
    elif mode == "shaded-edges":
        box(c, iso, 0, 0, 0, S, S, S)


for _m, _l in (("wireframe", "Wireframe"), ("hidden", "Hidden Line"), ("nohidden", "No Hidden"),
               ("shaded", "Shaded"), ("shaded-edges", "Shaded With Edges")):
    icon("view", f"display-{_m}", _l)(lambda c, t, _m=_m: style_cube(c, _m))


@icon("view", "display-section", "Section View")
def _(c, t):
    plan = [(0, 0), (13, 0), (13, 6.5), (6.5, 6.5), (6.5, 13), (0, 13)]
    iso = fitted(1.05, [(x, y, 0) for x, y in plan] + [(x, y, 11) for x, y in plan], 16, 16.5)
    solid_plan(c, iso, plan, 0, 11, tint={2, 3}, tint_cols=(t["cut_left"], t["cut_right"]))
    p = iso.p
    # hatching on the two section faces
    for k in range(1, 5):
        f = k / 5
        c.add(line(p(6.5 + 6.5 * f, 6.5, 0.8), p(6.5 + 6.5 * f - 1.4, 6.5, 10.2), t["cut"], 0.9, op=0.75))
        c.add(line(p(6.5, 6.5 + 6.5 * f, 0.8), p(6.5, 6.5 + 6.5 * f - 1.4, 10.2), t["cut"], 0.9, op=0.75))


@icon("view", "display-appearance", "Appearance")
def _(c, t):
    c.add(circle(16, 16, 11, fill=t["acc"], stroke=t["ink"], sw=1.7))
    c.add(P("M16 5 A11 11 0 0 1 27 16 L16 16 Z", fill=t["datum"]))
    c.add(P("M27 16 A11 11 0 0 1 16 27 L16 16 Z", fill=t["surf"]))
    c.add(circle(16, 16, 11, stroke=t["ink"], sw=1.7))
    c.add(ellipse(11.8, 11.2, 3.1, 1.9, rot=-40, fill=t["white"], op=0.7))


# -------------------------------------------------------------- visibility
def eye(c, cx, cy, s=1.0, slash=False):
    t = c.t
    c.add(P(f"M{fmt(cx - 6 * s)} {fmt(cy)} Q{fmt(cx)} {fmt(cy - 7.4 * s)} {fmt(cx + 6 * s)} {fmt(cy)} "
            f"Q{fmt(cx)} {fmt(cy + 7.4 * s)} {fmt(cx - 6 * s)} {fmt(cy)} Z", fill=t["bg"], stroke=t["ink"], sw=1.6))
    c.add(circle(cx, cy, 2.3 * s, fill=t["acc"]))
    if slash:
        c.add(line((cx - 6.5 * s, cy + 6 * s), (cx + 6.5 * s, cy - 6 * s), t["bg"], 3.4))
        c.add(line((cx - 6.5 * s, cy + 6 * s), (cx + 6.5 * s, cy - 6 * s), t["cut"], 1.8))


def toggle_of(name, label):
    def deco(fn_name):
        def draw(c, t):
            src = next(r for r in REGISTRY if r[1] == fn_name)
            start = len(c.items)
            src[3](c, t)
            moved = c.items[start:]
            del c.items[start:]
            c.add('<g transform="translate(1.2 0.6) scale(0.8)">' + "".join(moved) + "</g>")
            eye(c, 24, 25, 0.95)
        icon("view", name, label)(draw)
    return deco


toggle_of("show-datum-planes", "Show Datum Planes")("datum-plane")
toggle_of("show-datum-axes", "Show Datum Axes")("datum-axis")
toggle_of("show-datum-points", "Show Datum Points")("datum-point")
toggle_of("show-csys", "Show Coordinate Systems")("datum-csys")


@icon("view", "show-annotations", "Show Annotations")
def _(c, t):
    c.add(line((5, 6), (5, 17), t["ink"], 1.3), line((25, 6), (25, 17), t["ink"], 1.3))
    c.add(arrow((6, 11.5), (24, 11.5), t["acc"], 1.7, two=True, size=3.8, half=2.1))
    eye(c, 16, 23.5, 1.15)


@icon("view", "hide-selected", "Hide")
def _(c, t):
    eye(c, 16, 16, 2.0, slash=True)


@icon("view", "show-selected", "Unhide")
def _(c, t):
    eye(c, 16, 16, 2.0)
