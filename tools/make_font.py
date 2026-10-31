#!/usr/bin/env python3
"""Hollowstar 14x24 monospace font.

Every glyph is locked to the same lines:

  cap / digit / ascender top : row 2
  x-height top               : row 6
  baseline (bottom of H, n, 0): row 14
  descender bottom           : row 19

Stems are 2px. Diagonals are 2px wide but never paint below their endpoint,
so A and V sit on the same row as H.
"""
import pathlib

W, H = 16, 24
# Ink only uses columns 0..13. Advance 14 and keep every pixel, dropping the
# two always-empty columns on the right so letters sit closer without clipping.
ADVANCE = 14
SHIFT = 0
CAP_T, BASE = 2, 14
X_T = 6
DESC = 19
L, R, S = 2, 12, 2  # left stem, right stem, thickness

OUT = pathlib.Path(__file__).resolve().parents[1] / "src" / "kernel" / "src" / "font.bin"


class G:
    def __init__(self):
        self.p = [0] * (W * H)

    def dot(self, x, y):
        if 0 <= int(x) < W and 0 <= int(y) < H:
            self.p[int(y) * W + int(x)] = 1

    def rect(self, x, y, w, h):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                self.dot(xx, yy)

    def hbar(self, x, y, w, t=S):
        self.rect(x, y, w, t)

    def vbar(self, x, y, h, t=S):
        self.rect(x, y, t, h)

    def line(self, x0, y0, x1, y1):
        """2px-wide stroke kept inside the endpoints, so it cannot drop below the baseline."""
        steps = max(abs(x1 - x0), abs(y1 - y0), 1)
        x_lo, x_hi = min(x0, x1), max(x0, x1)
        y_lo, y_hi = min(y0, y1), max(y0, y1)
        for i in range(steps + 1):
            x = x0 + (x1 - x0) * i // steps
            y = y0 + (y1 - y0) * i // steps
            self.dot(x, y)
            if x + 1 <= x_hi:
                self.dot(x + 1, y)
            if y + 1 <= y_hi:
                self.dot(x, y + 1)

    def stamp(self, top, rows):
        for dy, row in enumerate(rows):
            for x, pixel in enumerate(row):
                if pixel == "#":
                    self.dot(x, top + dy)

    def pack(self):
        out = bytearray()
        for y in range(H):
            bits = 0
            for x in range(ADVANCE):
                src = x + SHIFT
                if src < W and self.p[y * W + src]:
                    bits |= 0x8000 >> x
            out.append((bits >> 8) & 0xFF)
            out.append(bits & 0xFF)
        return bytes(out)

    def rows(self):
        ink = [y for y in range(H) if any(self.p[y * W:(y + 1) * W])]
        return (min(ink), max(ink)) if ink else (None, None)


def stem_l(g, top, bot):
    g.vbar(L, top, bot - top + 1)


def stem_r(g, top, bot):
    g.vbar(R, top, bot - top + 1)


def stem_c(g, top, bot):
    g.vbar(7, top, bot - top + 1)


def bar(g, y, x0=L, x1=R + S - 1):
    g.hbar(x0, y, x1 - x0 + 1)


def oval(g, top, bot, x=L, w=12):
    """Shared rounded box. Top and bottom sit on the given rows."""
    t = 2
    h = bot - top + 1
    g.hbar(x + 2, top, w - 4, t)
    g.hbar(x + 2, bot - 1, w - 4, t)
    g.vbar(x, top + 2, h - 4, t)
    g.vbar(x + w - t, top + 2, h - 4, t)
    # Join the corners so every oval has the same silhouette.
    g.rect(x + 1, top + 1, 2, 2)
    g.rect(x + w - 3, top + 1, 2, 2)
    g.rect(x + 1, bot - 2, 2, 2)
    g.rect(x + w - 3, bot - 2, 2, 2)


def open_right(g, top, bot, x=L, w=12):
    """C-shape: oval with the right side removed, ends still on top/bottom."""
    oval(g, top, bot, x, w)
    g.rect(x + w - 4, top + 4, 4, (bot - top + 1) - 8)


def build():
    table = [G() for _ in range(256)]

    def ch(c):
        return table[ord(c)]

    # Digits share the cap box (rows 2..14) and a 2px stroke, same as the letters.
    ch("0").stamp(CAP_T, [
        "...######.....",
        "..##....##....",
        ".##......##...",
        "##........##..",
        "##........##..",
        "##........##..",
        "##........##..",
        "##........##..",
        "##........##..",
        "##........##..",
        ".##......##...",
        "..##....##....",
        "...######.....",
    ]);
    ch("1").stamp(CAP_T, [
        ".....##.......",
        "....###.......",
        "...#.##.......",
        "..#..##.......",
        ".....##.......",
        ".....##.......",
        ".....##.......",
        ".....##.......",
        ".....##.......",
        ".....##.......",
        ".....##.......",
        "..#######.....",
        "..#######.....",
    ]);
    ch("2").stamp(CAP_T, [
        "...######.....",
        "..##....##....",
        ".##......##...",
        ".........##...",
        "........##....",
        ".......##.....",
        "......##......",
        ".....##.......",
        "....##........",
        "...##.........",
        "..##..........",
        ".##########...",
        ".##########...",
    ]);
    ch("3").stamp(CAP_T, [
        "...######.....",
        "..##....##....",
        ".##......##...",
        ".........##...",
        "........##....",
        ".....####.....",
        ".....####.....",
        "........##....",
        ".........##...",
        ".##......##...",
        ".##......##...",
        "..##....##....",
        "...######.....",
    ]);
    ch("4").stamp(CAP_T, [
        ".......##.....",
        "......###.....",
        ".....##.##....",
        "....##..##....",
        "...##...##....",
        "..##....##....",
        ".##.....##....",
        "##......##....",
        "###########...",
        "###########...",
        "........##....",
        "........##....",
        "........##....",
    ]);
    ch("5").stamp(CAP_T, [
        ".##########...",
        ".##########...",
        ".##...........",
        ".##...........",
        ".##...........",
        ".########.....",
        ".##.....##....",
        ".........##...",
        "..........##..",
        "..........##..",
        ".##.......##..",
        "..##.....##...",
        "...######.....",
    ]);
    ch("6").stamp(CAP_T, [
        "...######.....",
        "..##....##....",
        ".##...........",
        "##............",
        "##............",
        "##............",
        "########......",
        "##......##....",
        "##........##..",
        "##........##..",
        ".##......##...",
        "..##....##....",
        "...######.....",
    ]);
    ch("7").stamp(CAP_T, [
        ".##########...",
        ".##########...",
        "..........##..",
        ".........##...",
        "........##....",
        ".......##.....",
        "......##......",
        "......##......",
        ".....##.......",
        ".....##.......",
        "....##........",
        "....##........",
        "...##.........",
    ]);
    ch("8").stamp(CAP_T, [
        "...######.....",
        "..##....##....",
        ".##......##...",
        ".##......##...",
        "..##....##....",
        "...######.....",
        "..##....##....",
        ".##......##...",
        "##........##..",
        "##........##..",
        ".##......##...",
        "..##....##....",
        "...######.....",
    ]);
    ch("9").stamp(CAP_T, [
        "...######.....",
        "..##....##....",
        ".##......##...",
        "##........##..",
        "##........##..",
        ".##......##...",
        "..##.....##...",
        "...#######....",
        "..........##..",
        ".........##...",
        ".##......##...",
        "..##....##....",
        "...######.....",
    ]);

    # --- uppercase, rows 2..14 ---
    def A(g):
        g.line(3, BASE, 7, CAP_T)
        g.line(8, CAP_T, 12, BASE)
        bar(g, 8, 4, 11)

    def B(g):
        stem_l(g, CAP_T, BASE)
        oval(g, CAP_T, 8, L, 10)
        oval(g, 8, BASE, L, 12)

    def C(g):
        g.stamp(CAP_T, [
            "...######.....",
            "..##....##....",
            ".##...........",
            "##............",
            "##............",
            "##............",
            "##............",
            "##............",
            "##............",
            "##............",
            ".##...........",
            "..##....##....",
            "...######.....",
        ])

    def D(g):
        stem_l(g, CAP_T, BASE)
        oval(g, CAP_T, BASE, L, 12)

    def E(g):
        stem_l(g, CAP_T, BASE)
        bar(g, CAP_T)
        bar(g, 7, L, 10)
        bar(g, BASE - 1)

    def F(g):
        stem_l(g, CAP_T, BASE)
        bar(g, CAP_T)
        bar(g, 7, L, 10)

    def gee(g):
        g.stamp(CAP_T, [
            "...######.....",
            "..##....##....",
            ".##...........",
            "##............",
            "##............",
            "##............",
            "##....####....",
            "##.....##.....",
            "##.....##.....",
            "##.....##.....",
            ".##....##.....",
            "..##..##......",
            "...####.......",
        ])

    def H(g):
        stem_l(g, CAP_T, BASE)
        stem_r(g, CAP_T, BASE)
        bar(g, 7)

    def I(g):
        g.stamp(CAP_T, [
            "..######......",
            "..######......",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "..######......",
            "..######......",
        ])

    def J(g):
        g.vbar(10, CAP_T, 11)
        g.hbar(4, BASE - 1, 8)
        g.vbar(3, 10, 4)

    def K(g):
        stem_l(g, CAP_T, BASE)
        g.line(12, CAP_T, 4, 8)
        g.line(5, 8, 13, BASE)

    def LL(g):
        stem_l(g, CAP_T, BASE)
        bar(g, BASE - 1)

    def M(g):
        stem_l(g, CAP_T, BASE)
        stem_r(g, CAP_T, BASE)
        g.line(3, CAP_T + 1, 7, 9)
        g.line(12, CAP_T + 1, 8, 9)

    def N(g):
        stem_l(g, CAP_T, BASE)
        stem_r(g, CAP_T, BASE)
        g.line(3, CAP_T + 1, 12, BASE - 1)

    def O(g):
        oval(g, CAP_T, BASE)

    def P(g):
        stem_l(g, CAP_T, BASE)
        oval(g, CAP_T, 9, L, 11)

    def Q(g):
        oval(g, CAP_T, BASE)
        g.line(8, 11, 13, DESC)

    def arr(g):
        P(g)
        g.line(7, 9, 13, BASE)

    def ess(g):
        g.stamp(CAP_T, [
            "..######......",
            ".##....##.....",
            "##............",
            "##............",
            ".##...........",
            "..######......",
            "......##......",
            ".......##.....",
            ".......##.....",
            "........##....",
            "##.....##.....",
            ".##....##.....",
            "..######......",
        ])

    def T(g):
        bar(g, CAP_T)
        stem_c(g, CAP_T, BASE)

    def U(g):
        stem_l(g, CAP_T, BASE - 2)
        stem_r(g, CAP_T, BASE - 2)
        g.hbar(4, BASE - 1, 8)

    def V(g):
        g.line(3, CAP_T, 7, BASE)
        g.line(12, CAP_T, 8, BASE)

    def W(g):
        g.line(2, CAP_T, 4, BASE)
        g.line(7, 8, 4, BASE)
        g.line(8, 8, 11, BASE)
        g.line(13, CAP_T, 11, BASE)

    def X(g):
        g.line(3, CAP_T, 12, BASE)
        g.line(12, CAP_T, 3, BASE)

    def Y(g):
        g.line(3, CAP_T, 7, 8)
        g.line(12, CAP_T, 8, 8)
        stem_c(g, 8, BASE)

    def Z(g):
        bar(g, CAP_T)
        g.line(12, CAP_T + 2, 3, BASE - 1)
        bar(g, BASE - 1)

    uppers = {
        "A": A, "B": B, "C": C, "D": D, "E": E, "F": F, "G": gee, "H": H,
        "I": I, "J": J, "K": K, "L": LL, "M": M, "N": N, "O": O, "P": P,
        "Q": Q, "R": arr, "S": ess, "T": T, "U": U, "V": V, "W": W, "X": X,
        "Y": Y, "Z": Z,
    }
    for name, fn in uppers.items():
        fn(ch(name))

    # --- lowercase ---
    def a(g):
        g.stamp(X_T, [
            "..######......",
            ".##....##.....",
            ".......##.....",
            "...#####......",
            "..##...##.....",
            ".##....##.....",
            "##.....##.....",
            ".##...###.....",
            "..#####.##....",
        ])

    def b(g):
        g.vbar(L, CAP_T, BASE - CAP_T + 1)
        oval(g, X_T, BASE, L, 11)

    def c(g):
        g.stamp(X_T, [
            "..######......",
            ".##....##.....",
            "##............",
            "##............",
            "##............",
            "##............",
            "##............",
            ".##....##.....",
            "..######......",
        ])

    def d(g):
        g.vbar(11, CAP_T, BASE - CAP_T + 1)
        oval(g, X_T, BASE, 2, 11)

    def e(g):
        g.stamp(X_T, [
            "..######......",
            ".##....##.....",
            "##.....##.....",
            "#########.....",
            "##............",
            "##............",
            "##............",
            ".##....##.....",
            "..######......",
        ])

    def f(g):
        g.hbar(6, CAP_T, 6)
        g.vbar(4, CAP_T + 1, BASE - CAP_T)
        bar(g, 8, 3, 10)

    def g(g):
        oval(g, X_T, BASE, 3, 10)
        g.vbar(11, X_T, DESC - X_T + 1)
        g.hbar(4, DESC - 1, 8)

    def h(g):
        g.vbar(L, CAP_T, BASE - CAP_T + 1)
        g.hbar(L + 1, X_T, 7)
        g.vbar(10, X_T + 2, BASE - (X_T + 2) + 1)

    def i(g):
        g.rect(6, CAP_T, 2, 2)
        g.vbar(6, X_T, BASE - X_T + 1)

    def j(g):
        g.rect(8, CAP_T, 2, 2)
        g.vbar(8, X_T, DESC - X_T - 1)
        g.hbar(3, DESC - 1, 7)

    def k(g):
        g.vbar(L, CAP_T, BASE - CAP_T + 1)
        g.line(11, X_T, 4, 11)
        g.line(5, 10, 12, BASE)

    def l(g):
        g.stamp(CAP_T, [
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....##........",
            "....####......",
        ])

    def m(g):
        g.vbar(2, X_T, BASE - X_T + 1)
        g.vbar(7, X_T + 2, BASE - X_T - 1)
        g.vbar(12, X_T + 2, BASE - X_T - 1)
        g.hbar(3, X_T, 4)
        g.hbar(8, X_T, 4)

    def n(g):
        g.vbar(L, X_T, BASE - X_T + 1)
        g.hbar(L + 1, X_T, 7)
        g.vbar(10, X_T + 2, BASE - X_T - 1)

    def o(g):
        oval(g, X_T, BASE, 3, 10)

    def p(g):
        g.vbar(L, X_T, DESC - X_T + 1)
        oval(g, X_T, BASE, L, 11)

    def q(g):
        g.vbar(11, X_T, DESC - X_T + 1)
        oval(g, X_T, BASE, 2, 11)

    def r(g):
        g.vbar(L, X_T, BASE - X_T + 1)
        g.hbar(L + 1, X_T, 6)
        g.vbar(9, X_T + 1, 3)

    def s(g):
        g.stamp(X_T, [
            "..######......",
            ".##....##.....",
            "##............",
            ".##...........",
            "..#####.......",
            "......##......",
            ".......##.....",
            "##....##......",
            ".######.......",
        ])

    def t(g):
        g.vbar(6, 4, BASE - 4 + 1)
        bar(g, 8, 3, 10)
        g.hbar(7, BASE - 1, 5)

    def u(g):
        g.vbar(L, X_T, 7)
        g.vbar(10, X_T, BASE - X_T + 1)
        g.hbar(L + 1, BASE - 1, 8)

    def v(g):
        g.line(3, X_T, 7, BASE)
        g.line(12, X_T, 8, BASE)

    def w(g):
        g.line(2, X_T, 4, BASE)
        g.line(7, 10, 4, BASE)
        g.line(8, 10, 11, BASE)
        g.line(13, X_T, 11, BASE)

    def x(g):
        g.line(3, X_T, 12, BASE)
        g.line(12, X_T, 3, BASE)

    def y(g):
        g.line(3, X_T, 7, 13)
        g.line(12, X_T, 5, DESC)

    def z(g):
        bar(g, X_T, 3, 12)
        g.line(11, X_T + 2, 3, BASE - 1)
        bar(g, BASE - 1, 3, 12)

    for name, fn in {
        "a": a, "b": b, "c": c, "d": d, "e": e, "f": f, "g": g, "h": h,
        "i": i, "j": j, "k": k, "l": l, "m": m, "n": n, "o": o, "p": p,
        "q": q, "r": r, "s": s, "t": t, "u": u, "v": v, "w": w, "x": x,
        "y": y, "z": z,
    }.items():
        fn(ch(name))

    # punctuation, anchored to the same lines
    ch("!").vbar(7, CAP_T, 8)
    ch("!").rect(7, BASE - 1, 2, 2)
    ch('"').vbar(5, CAP_T, 4)
    ch('"').vbar(9, CAP_T, 4)
    ch("#").vbar(4, 4, 11)
    ch("#").vbar(9, 4, 11)
    bar(ch("#"), 6, 3, 12)
    bar(ch("#"), 11, 3, 12)
    stem_c(ch("$"), CAP_T - 1, BASE + 1)
    ess(ch("$"))
    ch("%").rect(2, CAP_T, 4, 4)
    ch("%").rect(9, 11, 4, 4)
    ch("%").line(12, CAP_T, 3, BASE)
    oval(ch("&"), CAP_T, 8, 2, 8)
    ch("&").line(6, 8, 12, BASE)
    ch("&").line(3, 11, 8, BASE)
    ch("'").vbar(7, CAP_T, 4)
    ch("(").line(9, CAP_T, 5, 8)
    ch("(").vbar(5, 8, 5)
    ch("(").line(5, 12, 9, BASE + 2)
    ch(")").line(5, CAP_T, 9, 8)
    ch(")").vbar(9, 8, 5)
    ch(")").line(9, 12, 5, BASE + 2)
    ch("*").line(4, 5, 11, 12)
    ch("*").line(11, 5, 4, 12)
    ch("*").vbar(7, 4, 9)
    bar(ch("+"), 8, 3, 12)
    ch("+").vbar(7, 5, 9)
    ch(",").rect(6, BASE - 1, 2, 3)
    ch(",").rect(5, BASE + 1, 2, 2)
    bar(ch("-"), 9, 3, 11)
    ch(".").rect(6, BASE - 1, 2, 2)
    ch("/").line(12, CAP_T, 3, BASE)
    ch(":").rect(7, 6, 2, 2)
    ch(":").rect(7, BASE - 1, 2, 2)
    ch(";").rect(7, 6, 2, 2)
    ch(";").rect(6, BASE - 1, 2, 3)
    ch(";").rect(5, BASE + 1, 2, 2)
    ch("<").line(11, 5, 4, 9)
    ch("<").line(4, 9, 11, BASE)
    bar(ch("="), 6, 3, 12)
    bar(ch("="), 11, 3, 12)
    ch(">").line(4, 5, 11, 9)
    ch(">").line(11, 9, 4, BASE)
    ch("?").hbar(4, CAP_T, 7)
    ch("?").vbar(10, CAP_T + 1, 4)
    ch("?").hbar(7, 7, 4)
    ch("?").rect(7, BASE - 1, 2, 2)
    oval(ch("@"), 3, BASE, 2, 12)
    oval(ch("@"), 7, 12, 5, 6)
    ch("[").vbar(5, CAP_T, BASE - CAP_T + 1)
    ch("[").hbar(5, CAP_T, 6)
    ch("[").hbar(5, BASE - 1, 6)
    ch("\\").line(3, CAP_T, 12, BASE)
    ch("]").vbar(9, CAP_T, BASE - CAP_T + 1)
    ch("]").hbar(5, CAP_T, 6)
    ch("]").hbar(5, BASE - 1, 6)
    ch("^").line(3, 9, 8, CAP_T)
    ch("^").line(8, CAP_T, 13, 9)
    bar(ch("_"), 17, 2, 13)
    ch("`").line(5, CAP_T, 9, 6)
    ch("{").line(9, CAP_T, 6, 6)
    ch("{").vbar(6, 6, 4)
    ch("{").line(6, 10, 4, 12)
    ch("{").line(4, 12, 6, 14)
    ch("{").vbar(6, 14, 2)
    ch("{").line(6, 15, 9, BASE + 2)
    ch("|").vbar(7, CAP_T, 16)
    ch("}").line(5, CAP_T, 8, 6)
    ch("}").vbar(8, 6, 4)
    ch("}").line(8, 10, 10, 12)
    ch("}").line(10, 12, 8, 14)
    ch("}").vbar(8, 14, 2)
    ch("}").line(8, 15, 5, BASE + 2)
    ch("~").line(3, 10, 6, 7)
    ch("~").line(6, 7, 10, 10)
    ch("~").line(10, 10, 13, 7)

    box = G()
    oval(box, 4, 16, 3, 10)
    for i in range(32):
        table[i] = G()
    table[127] = box
    for i in range(128, 256):
        table[i] = box
    table[ord(" ")] = G()
    return table


FLAT = set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcefhiklmnorstuvwxz0123456789")
ASCENDERS = set("bdfhiklt")
DESCENDERS = set("gjpqy")


def check(table):
    bad = []
    for c in FLAT:
        lo, hi = table[ord(c)].rows()
        if c in DESCENDERS:
            continue
        if hi != BASE:
            bad.append(f"{c} bottom {hi}, want {BASE}")
        if c.isupper() or c.isdigit() or c in ASCENDERS:
            if lo != CAP_T and c not in "t":
                bad.append(f"{c} top {lo}, want {CAP_T}")
        elif c not in "i":
            if lo != X_T and c not in ASCENDERS:
                bad.append(f"{c} x-top {lo}, want {X_T}")
    return bad


def main():
    table = build()
    problems = check(table)
    for problem in problems:
        print("align:", problem)

    data = bytearray()
    for glyph in table:
        data.extend(glyph.pack())
    assert len(data) == 256 * H * 2
    OUT.write_bytes(data)
    print(f"wrote {OUT} ({len(data)} bytes), {len(problems)} alignment notes")

    # Preview with a baseline rule so uneven letters are obvious.
    scale = 4
    lines = [
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "abcdefghijklmnopqrstuvwxyz",
        "0123456789  The quick brown fox",
        "Hollowstar  jumps over the lazy dog",
        "Hnx0  Il1  OCo  gjpqy",
    ]
    gap_y = 8
    pw = (max(len(s) for s in lines) * ADVANCE + 8) * scale
    ph = (len(lines) * (H + gap_y) + 8) * scale
    img = bytearray([16, 18, 24]) * (pw * ph)

    def px(x, y, color):
        if 0 <= x < pw and 0 <= y < ph:
            i = (y * pw + x) * 3
            img[i:i + 3] = bytes(color)

    for li, text in enumerate(lines):
        oy = 4 + li * (H + gap_y)
        for row in range(H):
            color = (70, 40, 48) if row == BASE else (28, 32, 40)
            for x in range(pw):
                for s in range(scale):
                    px(x, (oy + row) * scale + s, color)
        for i, c in enumerate(text):
            raw = table[ord(c)].pack()
            ox = 4 + i * ADVANCE
            for y in range(H):
                bits = (raw[y * 2] << 8) | raw[y * 2 + 1]
                for x in range(ADVANCE):
                    if bits & (0x8000 >> x):
                        for sy in range(scale):
                            for sx in range(scale):
                                px((ox + x) * scale + sx, (oy + y) * scale + sy, (236, 240, 245))
    import struct
    import zlib
    raw = bytearray()
    for y in range(ph):
        raw.append(0)
        raw += img[y * pw * 3:(y + 1) * pw * 3]

    def chunk(tag, payload):
        return struct.pack(">I", len(payload)) + tag + payload + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", pw, ph, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    preview = pathlib.Path(__file__).resolve().parents[1] / "build" / "font_preview.png"
    preview.parent.mkdir(parents=True, exist_ok=True)
    preview.write_bytes(png)
    print(f"preview {preview}")


if __name__ == "__main__":
    main()
