"""General application icons."""
import math
from kit import *  # noqa: F401,F403
from kit import icon, arc_d, SCREEN, Iso, mix, box
from icons.view import eye
from icons.assembly import plus_badge, lock_badge
from icons.sketch import pencil, ink_dot
from kit import pt as _pt


def page(c, x=7, y=3.5, w=18, h=25, fold=6):
    t = c.t
    d = f"M{x} {y} L{x + w - fold} {y} L{x + w} {y + fold} L{x + w} {y + h} L{x} {y + h} Z"
    c.add(P(d, fill=t["top"], stroke=t["ink"], sw=1.7))
    c.add(P(f"M{x + w - fold} {y} L{x + w - fold} {y + fold} L{x + w} {y + fold}", fill=t["left"], stroke=t["ink"], sw=1.5))


@icon("general", "new", "New")
def _(c, t):
    page(c, 6, 3.5, 18, 25)
    plus_badge(c, 23.5, 23.5, 5.6)


@icon("general", "open", "Open")
def _(c, t):
    c.add(P("M3.5 25.5 L3.5 8 Q3.5 6.5 5 6.5 L11.5 6.5 L14 9.5 L22.5 9.5 Q24 9.5 24 11 L24 14", fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(P("M3.5 25.5 L7.5 14 Q8 13 9.2 13 L28 13 Q29.3 13 28.8 14.4 L25.2 24.8 Q24.8 25.5 23.8 25.5 Z", fill=t["top"], stroke=t["ink"], sw=1.7))


@icon("general", "save", "Save")
def _(c, t):
    c.add(P("M5 4.5 L23 4.5 L27.5 9 L27.5 26 Q27.5 27.5 26 27.5 L6 27.5 Q4.5 27.5 4.5 26 L4.5 6 Q4.5 4.5 6 4.5 Z", fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(rect(9, 4.8, 12, 7.6, fill=t["top"], stroke=t["ink"], sw=1.5))
    c.add(rect(8.5, 17, 15, 10.5, r=1, fill=t["top"], stroke=t["ink"], sw=1.5))
    c.add(rect(17, 6.4, 2.6, 4.2, fill=t["ink"]))


@icon("general", "save-as", "Save As")
def _(c, t):
    c.add(P("M4.5 4.5 L21 4.5 L25 8.5 L25 24 Q25 25.5 23.5 25.5 L6 25.5 Q4.5 25.5 4.5 24 Z", fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(rect(8.5, 4.8, 10.5, 6.6, fill=t["top"], stroke=t["ink"], sw=1.4))
    c.add(rect(8, 15.5, 13, 10, r=1, fill=t["top"], stroke=t["ink"], sw=1.4))
    pencil(c, (20.5, 28), -50, L=12, w=4.4)


@icon("general", "print", "Print")
def _(c, t):
    c.add(rect(9, 4, 14, 8.5, fill=t["bg"], stroke=t["ink"], sw=1.6))
    c.add(rect(4, 12, 24, 11, r=2.5, fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(rect(9, 19, 14, 9, fill=t["bg"], stroke=t["ink"], sw=1.6))
    c.add(line((11.5, 22.5), (20.5, 22.5), t["ink"], 1.2), line((11.5, 25.2), (20.5, 25.2), t["ink"], 1.2))
    c.add(circle(24, 16, 1.4, fill=t["acc"]))


@icon("general", "undo", "Undo")
def _(c, t):
    d = "M9 12 H19 A7 7 0 0 1 19 26 H11"
    c.add(P(d, stroke=t["ink"], sw=2.4))
    c.add(poly(head_pts((4.6, 12), (-1, 0), 6.6, 4.0), fill=t["acc"], stroke=t["acc"], sw=0.6))


@icon("general", "redo", "Redo")
def _(c, t):
    d = "M23 12 H13 A7 7 0 0 0 13 26 H21"
    c.add(P(d, stroke=t["ink"], sw=2.4))
    c.add(poly(head_pts((27.4, 12), (1, 0), 6.6, 4.0), fill=t["acc"], stroke=t["acc"], sw=0.6))


@icon("general", "regenerate", "Regenerate")
def _(c, t):
    d, e, tn = arc_d(SCREEN, 16, 16, 10, 215, 345)
    c.add(path_arrow(d, e, tn, t["acc"], 2.4, 5, 3.0))
    d, e, tn = arc_d(SCREEN, 16, 16, 10, 35, 165)
    c.add(path_arrow(d, e, tn, t["acc"], 2.4, 5, 3.0))
    iso = fitted_small()
    box(c, iso, 0, 0, 0, 6, 6, 6, sw=1.3)


def fitted_small():
    return Iso(0.95, 16, 14.7).fit([(0, 0, 0), (6, 6, 0), (6, 0, 0), (0, 6, 0), (0, 0, 6)], 16, 16.2)


@icon("general", "delete", "Delete")
def _(c, t):
    c.add(line((5, 8), (27, 8), t["ink"], 2.2))
    c.add(polyline([(12, 8), (12, 4.5), (20, 4.5), (20, 8)], t["ink"], 1.8))
    c.add(P("M7.5 8 L9 27 Q9.1 28 10.2 28 L21.8 28 Q22.9 28 23 27 L24.5 8 Z", fill=t["top"], stroke=t["ink"], sw=1.8))
    for x in (13, 16, 19):
        c.add(line((x, 12.5), (x, 23.5), t["ink"], 1.4))


@icon("general", "copy", "Copy")
def _(c, t):
    page(c, 4.5, 3.5, 15, 20, 5)
    t_ = t
    c.add(P("M13 9 L21.5 9 L27.5 15 L27.5 28.5 L13 28.5 Z", fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(P("M21.5 9 L21.5 15 L27.5 15", stroke=t["ink"], sw=1.5))


@icon("general", "paste", "Paste")
def _(c, t):
    c.add(rect(5, 6, 17, 22, r=2.2, fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(rect(9.5, 3.5, 8, 5.5, r=1.6, fill=t["acc"], stroke=t["ink"], sw=1.5))
    c.add(P("M15 13.5 L23 13.5 L28 18.5 L28 29 L15 29 Z", fill=t["top"], stroke=t["ink"], sw=1.7))
    c.add(P("M23 13.5 L23 18.5 L28 18.5", stroke=t["ink"], sw=1.4))


@icon("general", "cut", "Cut")
def _(c, t):
    c.add(line((10.5, 20), (23, 4), t["ink"], 2.2))
    c.add(line((21.5, 20), (9, 4), t["ink"], 2.2))
    c.add(circle(9.5, 23.5, 4.2, fill=t["bg"], stroke=t["acc"], sw=2.2))
    c.add(circle(22.5, 23.5, 4.2, fill=t["bg"], stroke=t["acc"], sw=2.2))


@icon("general", "select", "Select")
def _(c, t):
    c.add(poly([(8, 4.5), (8, 25.5), (13.2, 20.5), (16.6, 28.2), (20.2, 26.6), (16.8, 19.2), (24, 19.2)],
               fill=t["top"], stroke=t["ink"], sw=1.8))


@icon("general", "select-rect", "Select Box")
def _(c, t):
    c.add(rect(3.5, 4.5, 20, 17, stroke=t["acc"], sw=1.7, dash="3 2.2"))
    c.add(poly([(15, 12), (15, 28), (19.4, 24), (22.4, 30), (25, 28.8), (22.2, 23), (28, 23)], fill=t["top"], stroke=t["ink"], sw=1.6))


@icon("general", "measure", "Measure")
def _(c, t):
    c.add(f'<g transform="rotate(-38 16 16)">' + rect(1.5, 10.5, 29, 11, r=1.8, fill=t["hl_top"], stroke=t["ink"], sw=1.7)
          + "".join(line((x, 10.8), (x, 14.6 if i % 2 else 16.4), t["ink"], 1.3) for i, x in enumerate(range(5, 30, 3))) + "</g>")


@icon("general", "measure-angle", "Measure Angle")
def _(c, t):
    c.add(line((5, 26), (27, 26), t["ink"], 2.2))
    c.add(line((5, 26), (21, 6), t["ink"], 2.2))
    d, e, tn = arc_d(SCREEN, 5, 26, 12, -52, 0)
    c.add(P(d, stroke=t["acc"], sw=2.0))
    ink_dot(c, (5, 26), 2.0)


def gear_path(cx, cy, R, r, teeth=8):
    pts = []
    for i in range(teeth):
        a0 = 360 / teeth * i
        for da, rad in ((-0.30, r), (-0.17, R), (0.17, R), (0.30, r)):
            a = math.radians(a0 + da * 360 / teeth)
            pts.append((cx + rad * math.cos(a), cy + rad * math.sin(a)))
    return pts_d(pts, True)


@icon("general", "settings", "Settings")
def _(c, t):
    d = gear_path(16, 16, 12.5, 9.6) + f" M{fmt(16 + 4.2)} 16 A4.2 4.2 0 1 0 {fmt(16 - 4.2)} 16 A4.2 4.2 0 1 0 {fmt(16 + 4.2)} 16 Z"
    c.add(evenodd(P(d, fill=t["left"], stroke=t["ink"], sw=1.7)))


@icon("general", "help", "Help")
def _(c, t):
    c.add(circle(16, 16, 12, fill=t["top"], stroke=t["ink"], sw=1.8))
    c.add(P("M11.6 12.6 Q11.6 8.4 16 8.4 Q20.4 8.4 20.4 12.2 Q20.4 14.6 17.8 15.8 Q16 16.7 16 18.8", stroke=t["ink"], sw=2.2))
    c.add(circle(16, 22.8, 1.5, fill=t["acc"]))


@icon("general", "search", "Search")
def _(c, t):
    c.add(line((19.5, 19.5), (27.5, 27.5), t["ink"], 3.6))
    c.add(circle(13.5, 13.5, 9, fill=t["top"], fop=0.8, stroke=t["ink"], sw=2.0))


@icon("general", "layers", "Layers")
def _(c, t):
    for i, col in enumerate((t["right"], t["left"], t["top"])):
        y = 22 - i * 6.5
        c.add(poly([(16, y - 7), (28, y - 1), (16, y + 5), (4, y - 1)], fill=col, stroke=t["ink"], sw=1.6))


@icon("general", "model-tree", "Model Tree")
def _(c, t):
    c.add(polyline([(7, 7), (7, 25), (13, 25)], t["ink"], 1.5))
    c.add(line((7, 16), (13, 16), t["ink"], 1.5))
    c.add(rect(3.5, 3.5, 7, 6.5, r=1.2, fill=t["acc"], stroke=t["ink"], sw=1.4))
    c.add(rect(13.5, 12.8, 14, 6.4, r=1.2, fill=t["left"], stroke=t["ink"], sw=1.4))
    c.add(rect(13.5, 22, 14, 6.4, r=1.2, fill=t["left"], stroke=t["ink"], sw=1.4))
    c.add(line((13.5, 6.8), (27.5, 6.8), t["ink"], 1.6))


@icon("general", "ok", "OK / Accept")
def _(c, t):
    c.add(circle(16, 16, 12.5, fill=t["acc"], stroke=t["ink"], sw=1.7))
    c.add(polyline([(9.6, 16.4), (14.2, 21), (22.6, 11.2)], t["white"], 3.0))


@icon("general", "cancel", "Cancel")
def _(c, t):
    c.add(circle(16, 16, 12.5, fill=t["cut"], stroke=t["ink"], sw=1.7))
    c.add(line((10.6, 10.6), (21.4, 21.4), t["white"], 3.0), line((21.4, 10.6), (10.6, 21.4), t["white"], 3.0))


@icon("general", "info", "Information")
def _(c, t):
    c.add(circle(16, 16, 12.5, fill=t["top"], stroke=t["ink"], sw=1.8))
    c.add(circle(16, 9.6, 1.7, fill=t["acc"]))
    c.add(line((16, 14), (16, 23), t["ink"], 2.6))


@icon("general", "warning", "Warning")
def _(c, t):
    c.add(poly([(16, 4.5), (29, 27), (3, 27)], fill=t["hl_top"], stroke=t["ink"], sw=1.8))
    c.add(line((16, 12.5), (16, 19.5), t["ink"], 2.6))
    c.add(circle(16, 23.4, 1.6, fill=t["ink"]))


@icon("general", "add", "Add")
def _(c, t):
    c.add(line((16, 6), (16, 26), t["ink"], 3.2), line((6, 16), (26, 16), t["ink"], 3.2))


@icon("general", "remove", "Remove")
def _(c, t):
    c.add(line((6, 16), (26, 16), t["ink"], 3.2))


@icon("general", "import", "Import")
def _(c, t):
    c.add(polyline([(5, 19), (5, 27), (27, 27), (27, 19)], t["ink"], 2.2))
    c.add(arrow((16, 3.5), (16, 21), t["acc"], 2.6, size=5.6, half=3.4))


@icon("general", "export", "Export")
def _(c, t):
    c.add(polyline([(5, 19), (5, 27), (27, 27), (27, 19)], t["ink"], 2.2))
    c.add(arrow((16, 21), (16, 3.5), t["acc"], 2.6, size=5.6, half=3.4))


@icon("general", "folder", "Folder")
def _(c, t):
    c.add(P("M3.5 25.5 L3.5 8.5 Q3.5 7 5 7 L12 7 L14.6 10.2 L26.5 10.2 Q28 10.2 28 11.7 L28 25.5 Q28 27 26.5 27 L5 27 Q3.5 27 3.5 25.5 Z", fill=t["left"], stroke=t["ink"], sw=1.7))
    c.add(line((3.7, 14.6), (27.8, 14.6), t["ink"], 1.4, op=0.8))


@icon("general", "home", "Home")
def _(c, t):
    c.add(poly([(5.5, 15), (16, 5.5), (26.5, 15), (26.5, 27), (5.5, 27)], fill=t["top"], stroke=t["ink"], sw=1.8))
    c.add(rect(13, 18, 6, 9, fill=t["acc"], stroke=t["ink"], sw=1.5))
    c.add(polyline([(3, 16), (16, 4.2), (29, 16)], t["ink"], 2.2))


@icon("general", "filter", "Filter")
def _(c, t):
    c.add(poly([(4.5, 6), (27.5, 6), (18.6, 16.5), (18.6, 26.5), (13.4, 23.5), (13.4, 16.5)], fill=t["top"], stroke=t["ink"], sw=1.8))
    c.add(line((8, 9.2), (24, 9.2), t["acc"], 1.8))


@icon("general", "link", "Reference / Link")
def _(c, t):
    c.add(rect(2.5, 11, 15, 10, r=5, stroke=t["ink"], sw=2.2))
    c.add(rect(14.5, 11, 15, 10, r=5, stroke=t["acc"], sw=2.2))


@icon("general", "chevron-down", "Expand")
def _(c, t):
    c.add(polyline([(7, 12), (16, 21), (25, 12)], t["ink"], 3.0))


@icon("general", "chevron-right", "Collapse")
def _(c, t):
    c.add(polyline([(12, 7), (21, 16), (12, 25)], t["ink"], 3.0))


@icon("general", "lock", "Lock")
def _(c, t):
    d, e, tn = arc_d(SCREEN, 16, 14, 5.5, 180, 360)
    c.add(P(d + " L21.5 16 M10.5 14 L10.5 16", stroke=t["ink"], sw=2.2))
    c.add(rect(7, 14.5, 18, 13, r=2.5, fill=t["left"], stroke=t["ink"], sw=1.8))
    c.add(circle(16, 20, 2.0, fill=t["ink"]))
    c.add(line((16, 21), (16, 24), t["ink"], 1.8))


@icon("general", "play", "Resume / Play")
def _(c, t):
    c.add(poly([(9, 5.5), (9, 26.5), (26, 16)], fill=t["acc"], stroke=t["ink"], sw=1.8))


@icon("general", "pause", "Pause")
def _(c, t):
    c.add(rect(7.5, 5.5, 6.2, 21, r=1.2, fill=t["ink"]), rect(18.3, 5.5, 6.2, 21, r=1.2, fill=t["ink"]))
