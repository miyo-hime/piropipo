#!/usr/bin/env python3
"""Rewrite "but we endure" by the rules in LESSONS.md, and write the score.

    python3 contrib/ear/rewrite.py      # writes contrib/ear/songs/but-we-endure-rewrite.piro and its .fx.json

The original's tunes stay; what changes is how they're played. Every rule below comes from
the listener's ear (LESSONS.md) or from the listener's notes for this rewrite:

- An answering line keeps its call's rhythm and lets the notes carry the difference (lesson 1).
- Line endings resolve: under B7 the held E steps down to D# (lesson 2).
- No late copies: pulse2 doubles the melody in time and gets its drive from repeated notes, the
  double tongue "ta . ta ka" (lesson 5).
- The accompaniment plays chord tones only, an octave or more under the melody, and the generator
  swaps out any accompaniment note that would rub a half step against the melody.
- A change of meter is announced: the bar before it swells with white noise into a snare roll,
  the downbeat hits, and the first bar of the new meter is heavy (kick on every beat group, the
  bass pounding the root, a chord stab on every group).
- Meter changes happen at section boundaries only. The original's one- and two-bar excursions
  (crack's tail, bow) are rewritten into the section's meter.
- A fragmented mind is a sound, not a broken melody: fragments keeps a whole melody and gets its
  fracture from tone flips, warbling vibrato, slides, and a bitcrush that deepens (in the .fx.json,
  applied by master.py). Shatter gets the same treatment at its peak.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = HERE / "songs" / "but-we-endure-rewrite.piro"
LANES = ("pulse1", "pulse2", "triangle", "noise")

LETTER = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
SHARP = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def midi(p: str) -> int:
    m = re.fullmatch(r"([A-G])([#b]?)(-?\d+)", p)
    return 12 * (int(m.group(3)) + 1) + LETTER[m.group(1)] + {"#": 1, "b": -1, "": 0}[m.group(2)]


def pname(m: int) -> str:
    return f"{SHARP[m % 12]}{m // 12 - 1}"


CHORDS = {  # root, intervals; the first three are the triad
    "Em": ("E", [0, 3, 7]), "C": ("C", [0, 4, 7]), "Am": ("A", [0, 3, 7]), "B7": ("B", [0, 4, 7, 10]),
    "D": ("D", [0, 4, 7]), "Bm": ("B", [0, 3, 7]), "F": ("F", [0, 4, 7, 11]), "E5": ("E", [0, 7]),
    "Dm": ("D", [0, 3, 7]), "Cmaj7": ("C", [0, 4, 7, 11]),
}


def chord_pcs(ch: str) -> set:
    root, ivs = CHORDS[ch]
    return {(LETTER[root] + i) % 12 for i in ivs}


def near(root: str, around: int) -> int:
    """The root's midi number closest to `around`."""
    pc = LETTER[root]
    return min((m for m in range(around - 6, around + 7) if m % 12 == pc), key=lambda m: abs(m - around))


def bad(a: int, b: int, chord: str) -> bool:
    """Two simultaneous pitches that rub: a half step or a tritone (any octave), unless both are the
    chord's own tones (B over C in Cmaj7 is the chord the listener named, not a clash)."""
    ic = abs(a - b) % 12
    return ic in (1, 11, 6) and not {a % 12, b % 12} <= chord_pcs(chord)


class Sec:
    """One pattern. Times are in units: a 32nd note when unit=2, a 16th when unit=1, or a tuplet slot."""

    def __init__(self, name, meter, bars, unit=2, grid=None, lanes=None, comment=True):
        self.name, self.meter, self.n, self.unit, self.grid = name, meter, bars, unit, grid
        num, den = map(int, meter.split("/"))
        self.num, self.den = num, den
        self.bar = grid if grid else num * 16 // den * unit
        self.e = unit * 2 if not grid else None          # units per eighth
        self.notes = {lane: [] for lane in LANES}
        self.lanes = lanes or {}
        self.chords = {}
        self.comment = comment

    # groups of eighths: 7/8 is 2+2+3, 4/4 is four quarters, 5/4 is 3+2 quarters
    def groups(self):
        return {"7/8": [2, 2, 3], "4/4": [2, 2, 2, 2], "5/4": [2, 2, 2, 2, 2]}[self.meter]

    def accents(self):
        """Eighth offsets of the strong beats (group starts that matter most)."""
        return {"7/8": [0, 2, 4], "4/4": [0, 2, 4, 6], "5/4": [0, 6]}[self.meter]

    def at(self, bar, eighth=0.0):
        return (bar - 1) * self.bar + int(round(eighth * (self.e or 1)))

    def add(self, lane, start, length, pitch, fx="", tag=None):
        assert length > 0 and start >= 0 and start + length <= self.n * self.bar, (self.name, lane, start, length, pitch)
        for s, l, p, *_ in self.notes[lane]:
            assert start >= s + l or start + length <= s, f"{self.name} {lane}: {pitch} at {start} overlaps {p} at {s}"
        self.notes[lane].append([start, length, pitch, fx, tag])

    def sounding(self, lane, t):
        return [n for n in self.notes[lane] if n[0] <= t < n[0] + n[1] and n[2] != "-"]

    def chord_at(self, t):
        return self.chords.get(t // self.bar + 1)

    # ------------------------------------------------------------ writing it out
    def header(self):
        if self.grid:
            return f"pattern {self.name} bars {self.n} grid {self.grid}"
        if self.meter == "7/8":
            assert self.n % 2 == 0, "7/8 sections come in pairs of bars (one 7-beat box each)"
            return f"pattern {self.name} bars {self.n // 2} beats 7 grid {28 * self.unit}"
        if self.meter == "5/4":
            return f"pattern {self.name} bars {self.n} beats 5 grid {20 * self.unit}"
        return f"pattern {self.name} bars {self.n} grid {16 * self.unit}"

    def box(self):
        return self.bar * 2 if (self.meter == "7/8" and not self.grid) else self.bar

    def pos(self, t):
        return f"{t // self.box() + 1}.{t % self.box() + 1}:"

    def text(self):
        out = [self.header()]
        if self.comment and not self.grid:
            out.append(f"; {self.n}x {self.meter}")
        for lane in LANES:
            notes = sorted(self.notes[lane])
            if not notes:
                continue
            out.append("")
            out.append(f"lane {lane} {self.lanes.get(lane, '')}".rstrip())
            for start, length, pitch, fx, tag in notes:
                mark = f"  ; |{start // self.bar + 1}" if start % self.bar == 0 and lane == "pulse1" else ""
                out.append(f"  {self.pos(start):<8}{pitch} x{length}{(' ' + fx) if fx else ''}{mark}")
        return "\n".join(out)


# ------------------------------------------------------------------ melody
def tune(spec: str):
    """'E5:4 B5:2|vol 9 -:2' -> [(pitch, sixteenths, fx)]"""
    out = []
    for tok in spec.split():
        head, _, fx = tok.partition("|")
        p, l = head.split(":")
        out.append((p, float(l), fx.replace("_", " ")))
    return out


def line(sec, bar, spec, lane="pulse1", start16=0.0, fx_all=""):
    per16 = sec.unit if not sec.grid else 1
    t = sec.at(bar) + int(start16 * per16)
    for p, l16, fx in tune(spec):
        length = int(round(l16 * per16))
        if p != "-":
            sec.add(lane, t, length, p, " ".join(x for x in (fx, fx_all) if x), "mel")
        t += length


# ------------------------------------------------------------------ pulse2
def tongue(sec, b0, b1, vols=(5, 5, 3), down=12):
    """the double tongue: under every melody note, the same note an octave down, struck with it,
    in 32nds: ta . ta ka. The softer ka makes the chip strike again instead of holding."""
    assert sec.unit == 2, "the double tongue needs 32nds"
    lo, hi = sec.at(b0), sec.at(b1 + 1)
    for s, l, p, fx, tag in sorted(sec.notes["pulse1"]):
        if not lo <= s < hi:
            continue
        low = pname(midi(p) - down)
        u = 0
        while u < l:
            rem = l - u
            if rem >= 4:
                cells, step = [(0, vols[0]), (2, vols[1]), (3, vols[2])], 4
            elif rem >= 2:
                cells, step = [(0, vols[0]), (1, vols[2])], 2
            else:
                cells, step = [(0, vols[0])], 1
            for off, v in cells:
                sec.add("pulse2", s + u + off, 1, low, f"vol {v}", "tongue")
            u += step


def double(sec, b0, b1, down=12, vol=None):
    """pulse2 plays the melody's own notes an octave down, in time (for runs too quick to tongue)."""
    lo, hi = sec.at(b0), sec.at(b1 + 1)
    for s, l, p, fx, tag in sorted(sec.notes["pulse1"]):
        if lo <= s < hi:
            sec.add("pulse2", s, l, pname(midi(p) - down), f"vol {vol}" if vol else "", "double")


def roll(sec, bar, chord, around=52, vol=None):
    """16th-note arpeggio of the chord's own tones, low: root 5th 3rd 5th per pair of eighths,
    root 5th 3rd 5th 7th/8ve 5th per three. Any tone that would rub against the melody note
    sounding at that moment is swapped for another chord tone."""
    root, ivs = CHORDS[chord]
    r = near(root, around)
    third = r + ivs[1]
    fifth = r + ivs[2] if len(ivs) > 2 else r + 7
    top = r + ivs[3] if len(ivs) > 3 else r + 12
    pool = sorted({r, third, fifth, top, r + 12})
    per16 = sec.unit
    t = sec.at(bar)
    for g in sec.groups():
        figure = [r, fifth, third, fifth] if g == 2 else [r, fifth, third, fifth, top, fifth]
        for k, m in enumerate(figure):
            when = t + k * per16
            mel = sec.sounding("pulse1", when)
            if mel and bad(m, midi(mel[0][2]), chord):
                ok = [c for c in pool if not bad(c, midi(mel[0][2]), chord)]
                m = min(ok, key=lambda c: abs(c - m)) if ok else m
            sec.add("pulse2", when, per16, pname(m), f"vol {vol}" if vol else "", "roll")
        t += g * sec.e


def stab(sec, bar, chord, eighths, vol=10, octave_around=64):
    """A short arp chord on each listed eighth, fading like a struck piano. A chord tone that would
    rub against the melody note sounding right then is left out of that stab."""
    root, ivs = CHORDS[chord]
    r = near(root, octave_around)
    for e in eighths:
        when = sec.at(bar, e)
        mel = sec.sounding("pulse1", when)
        keep = [i for i in ivs[:4] if not (mel and bad(r + i, midi(mel[0][2]), chord))]
        if 0 not in keep:
            keep = [0] + keep
        if len(keep) < 2:
            keep = [0, 12]
        sec.add("pulse2", when, sec.e, pname(r), f"arp {','.join(str(i) for i in keep)} vol {vol} fade", "stab")


def pad(sec, bar, chord, bars=1, around=64, vol=2, fade=False):
    root, ivs = CHORDS[chord]
    r = near(root, around)
    sec.add("pulse2", sec.at(bar), sec.bar * bars, pname(r), f"arp {','.join(str(i) for i in ivs[:4])} vol {vol}" + (" fade" if fade else ""), "pad")


def answer(sec, bar, chord, start_e, count16=6, around=64, vol=5):
    """Chord tones falling and rising in the melody's rests: call and response, never overlap."""
    root, ivs = CHORDS[chord]
    r = near(root, around)
    tones = [r + ivs[2], r + ivs[1], r, r + ivs[1], r + ivs[2], r + 12]
    t = sec.at(bar, start_e)
    for k in range(count16):
        sec.add("pulse2", t + k * sec.unit, sec.unit, pname(tones[k % len(tones)]), f"vol {vol}", "answer")


# ------------------------------------------------------------------ triangle
def bass(sec, bar, chord, style="walk"):
    root, ivs = CHORDS[chord]
    r = near(root, 40)
    if r > 43:
        r -= 12
    fifth = r + 7
    if style == "pound":
        figure = [r] * sum(sec.groups())
    elif sec.meter == "7/8":
        figure = [r, r + 12, r, r + 12, r, fifth, r + 12]
    elif sec.meter == "5/4":
        figure = [r, r + 12, r, r + 12, r, r + 12, r, r + 12, fifth, r + 12]
    else:
        figure = [r, r + 12] * 4
    for k, m in enumerate(figure):
        when = sec.at(bar, k)
        mel = sec.sounding("pulse1", when)
        if mel and bad(m, midi(mel[0][2]), chord):
            m = r if not bad(r, midi(mel[0][2]), chord) else fifth
        length = sec.e if style != "pound" else sec.e // 2 if sec.e >= 2 else 1
        sec.add("triangle", when, length, pname(m), "", "bass")


def hold_bass(sec, bar, chord, bars=1):
    root, _ = CHORDS[chord]
    r = near(root, 40)
    sec.add("triangle", sec.at(bar), sec.bar * bars, pname(r if r <= 43 else r - 12), "", "bass")


# ------------------------------------------------------------------ noise
KICK, SNARE, HAT, HISS = "C4", "E4", "B4", "B4"


def hit(sec, when, length, letter, vol, fade=True):
    sec.add("noise", when, length, letter, f"vol {vol}" + (" fade" if fade else ""), "drum")


def groove(sec, bar, hats=3, kick=12, snare=10):
    """The meter's own beat: kick on the strong eighths, snare on the backbeats, hats between."""
    e = sec.e
    pattern = {"7/8": "KhShKhS", "4/4": "KhShKhSh", "5/4": "KhhhShKhSh"}[sec.meter]
    for k, c in enumerate(pattern):
        letter, vol = {"K": (KICK, kick), "S": (SNARE, snare), "h": (HAT, hats)}[c]
        if c == "h" and not hats:
            continue
        hit(sec, sec.at(bar, k), e, letter, vol)


def heavy(sec, bar, chord, stabs=True):
    """The first bar of a new meter: crash on the downbeat, a kick on every beat group, snares on
    everything between, the last group in 16ths, the bass pounding, a chord on every group."""
    e = sec.e
    groups = sec.groups()
    starts, acc = set(), 0
    for g in groups:
        starts.add(acc)
        acc += g
    strong = set(sec.accents())
    total = sum(groups)
    last = total - groups[-1]
    for k in range(total):
        when = sec.at(bar, k)
        if k == 0:
            hit(sec, when, e, HISS, 15)  # the crash
        elif k >= last and e >= 2:
            hit(sec, when, e // 2, SNARE, 12 + (k - last))
            hit(sec, when + e // 2, e - e // 2, SNARE, 13 + (k - last))
        elif k in starts or k in strong:
            hit(sec, when, e, KICK, 15)
        else:
            hit(sec, when, e, SNARE, 12)
    bass(sec, bar, chord, "pound")
    if stabs:
        stab(sec, bar, chord, sorted(starts | strong), vol=11)


def riser(sec, bar, lo=1, hi=14, roll16=4):
    """The bar before a new meter: white noise swelling from nothing, turning into a snare roll."""
    per16 = sec.unit if not sec.grid else 1
    n16 = sec.bar // per16
    for k in range(n16):
        when = sec.at(bar) + k * per16
        if k >= n16 - roll16:
            hit(sec, when, per16, SNARE, min(15, 12 + (k - (n16 - roll16))), fade=False)
        else:
            vol = round(lo + (hi - lo) * k / max(1, n16 - roll16 - 1))
            hit(sec, when, per16, HISS, vol, fade=False)


def fill(sec, bar, from_eighth, lo=6, hi=14):
    """A snare roll in 16ths through the end of the bar (a fill, not a meter change)."""
    per16 = sec.unit
    t, end = sec.at(bar, from_eighth), sec.at(bar + 1)
    n = (end - t) // per16
    for k in range(n):
        hit(sec, t + k * per16, per16, SNARE, round(lo + (hi - lo) * k / max(1, n - 1)), fade=False)


# ================================================================== the song
THEME = [  # the broken chord of mask, binding and bow: the listener read it as E, C, A, B
    ("Em", "E5 B5 F#5 B5 G5 B5 D6"), ("Cmaj7", "C5 B5 E5 B5 G5 B5 D6"),
    ("Am", "A4 B5 C5 B5 E5 B5 C6"), ("B7", "B4 A5 D#5 A5 F#5 A5 B5"),
]


def theme_bar(sec, bar, which, fx=""):
    chord, notes = THEME[which % 4]
    sec.chords[bar] = chord
    line(sec, bar, " ".join(f"{p}:2" for p in notes.split()), fx_all=fx)


def mask():
    s = Sec("mask", "7/8", 10, unit=2, lanes={"pulse1": "duty 12.5 vol 6", "pulse2": "duty 12.5 vol 4"})
    # |1-2 the door chime, doubled in time an octave down, ringing for two bars
    s.chords[1] = s.chords[2] = "Em"
    line(s, 1, "B5:6|duty_50_vol_9_fade G5:22|duty_50_vol_9_fade")
    s.add("pulse2", s.at(1), 12, "B4", "vol 3 fade")
    s.add("pulse2", s.at(1, 3), 44, "G4", "vol 3 fade")
    for i in range(8):
        theme_bar(s, 3 + i, i)
    tongue(s, 7, 10)                       # the listener's fix for the late copy (lesson 5)
    bass(s, 9, "Am")
    bass(s, 10, "B7")
    for b in (7, 8, 9):
        groove(s, b, hats=2, kick=5, snare=4)
    for k in range(4):
        hit(s, s.at(10, k), s.e, [KICK, HAT, SNARE, HAT][k], [6, 2, 5, 2][k])
    fill(s, 10, 4, lo=4, hi=12)            # same meter into service: a fill, not a riser
    return s


def service():
    s = Sec("service", "7/8", 16, unit=2, lanes={"pulse1": "duty 25 vol 7", "pulse2": "duty 25 vol 4"})
    call_a, call_b = "E5:4 B5:2 A5:2 G5:2 F#5:4", "G5:2 A5:2 B5:4 D6:2 B5:4"
    # line 2 answers in line 1's rhythm (lesson 1) and its E steps down to D# under B7 (lesson 2)
    ans_a, ans_b = "A5:4 A5:2|vol_10 G5:2 F#5:2 E5:4", "F#5:2 G5:2 B4:4 E5:2 D#5:3 -:1"
    bars = [call_a, call_b, ans_a, ans_b,
            call_a, "E5:12|vol_5 -:2", ans_a, "F#5:12|vol_5 -:2"]
    prog = ["Em", "C", "Am", "B7"] * 4
    for b, ch in enumerate(prog, 1):
        s.chords[b] = ch
    for b, spec in enumerate(bars, 1):
        line(s, b, spec)
    loud = {call_a: "E5:4|vol_9 B5:2|vol_9 A5:2|vol_9 G5:2|vol_9 F#5:4|vol_9",
            call_b: "G5:2|vol_9 A5:2|vol_9 B5:4|vol_9 D6:2|vol_9 B5:4|vol_9",
            ans_a: "A5:4|vol_9 A5:2|vol_12 G5:2|vol_9 F#5:2|vol_9 E5:4|vol_9",
            ans_b: "F#5:2|vol_9 G5:2|vol_9 B4:4|vol_9 E5:2|vol_9 D#5:3|vol_9 -:1"}
    for b, spec in enumerate([call_a, call_b, ans_a, ans_b], 9):
        line(s, b, loud[spec])
    runs = ["E5 B5 A5 G5 F#5 G5 A5 B5", "G5 D6 C6 B5 A5 B5 C6 D6", "A5 E6 D6 C6 B5 C6 D6 E6"]
    for b, run in enumerate(runs, 13):
        line(s, b, " ".join(f"{p}:1|vol_9" for p in run.split()))
    line(s, 16, " ".join(f"{p}:1|vol_10" for p in "B4 D#5 F#5 A5 G5 F#5 D#5 F#5".split()) + " -:6")  # the riser fills the rest
    for b in range(1, 9):
        roll(s, b, prog[b - 1])
    tongue(s, 9, 12, vols=(7, 7, 4))       # the loud repeat gets the double tongue, not the echo
    for b, ch in zip((13, 14, 15), ("Em", "C", "Am")):
        answer(s, b, ch, start_e=4, count16=6, around=64)
    double(s, 16, 16, vol=6)
    for b in range(1, 17):
        bass(s, b, prog[b - 1])
        if b != 16:
            groove(s, b, hats=3 if b < 9 else 4, kick=11 if b < 9 else 13, snare=9 if b < 9 else 11)
    riser(s, 16)                            # 7/8 -> 4/4 next
    return s


def crack(name, drive):
    s = Sec(name, "4/4", 11, unit=2, lanes={"pulse1": "duty 25 vol 13", "pulse2": "duty 25 vol 6"})
    prog = ["C", "D", "Bm", "Em", "C", "D", "B7", "B7", "Em", "Am", "B7"]
    for b, ch in enumerate(prog, 1):
        s.chords[b] = ch
    head = "E6:6|from_B5 D6:2 E6:4 G6:4"
    melody = [head, "F#6:6 E6:2 D6:4 B5:4", "B5:6 A5:2 B5:4 D6:4", "E6:14|vib_30ct_6hz -:2",
              head, "F#6:6 G6:2 A6:4 F#6:4", "G6:4 F#6:4 D#6:4 F#6:4", "D#6:12|vib_30ct_6hz -:4"]
    for b, spec in enumerate(melody, 1):
        line(s, b, spec)
    # the original's 7/8 + 5/4 tail, rewritten into 4/4 as the opening's broken-chord theme in
    # eighths: crack winds down by quoting where the song began. (16ths here were flagged
    # "everything at once": busy rhythm and busy leaps together. the trade: calm one.)
    tail = ["E5 B5 F#5 B5 G5 B5 D6 B5", "A4 B5 C5 B5 E5 B5 C6 B5", "B4 A5 D#5 A5 F#5 A5 B5 A5"]
    for b, notes in enumerate(tail, 9):
        line(s, b, " ".join(f"{p}:2|vol_12" for p in notes.split()))
    heavy(s, 1, "C")                         # 7/8 -> 4/4: the new meter announces itself
    if drive:
        tongue(s, 2, 8, vols=(8, 8, 5))
    else:
        for b in range(2, 9):
            roll(s, b, prog[b - 1], around=55, vol=6)
    if drive:
        double(s, 9, 11, down=12, vol=6)    # the first time, the busy tail gets room: pulse2 sits out
    for b in range(2, 12):
        bass(s, b, prog[b - 1])
    for b in range(2, 11):
        groove(s, b, hats=4, kick=13, snare=11)
    for k in range(4):
        hit(s, s.at(11, k), s.e, [KICK, HAT, SNARE, HAT][k], [13, 4, 11, 4][k])
    fill(s, 11, 4, lo=7, hi=14)
    return s


def endure():
    s = Sec("endure", "4/4", 2, unit=1, lanes={"pulse1": "duty 50 vol 4", "pulse2": "duty 12.5 vol 2"})
    s.chords = {1: "Em", 2: "B7"}
    line(s, 1, "E5:4 B5:4 A5:4 G5:4")
    line(s, 2, "F#5:12|fade -:4")
    pad(s, 1, "Em", around=64)
    pad(s, 2, "B7", around=59, fade=True)
    hold_bass(s, 2, "B7")
    # 4/4 -> 5/4 next: a two-bar swell, the longest breath in the song
    for k in range(32):
        if k >= 28:
            hit(s, k, 1, SNARE, 12 + (k - 28), fade=False)
        else:
            hit(s, k, 1, HISS, round(1 + 13 * k / 27), fade=False)
    return s


def fragments():
    s = Sec("fragments", "5/4", 8, unit=1, lanes={"pulse1": "duty 25 vol 10", "pulse2": "duty 25 vol 4"})
    prog = ["Em", "F", "Em", "F", "Am", "F", "B7", "B7"]
    for b, ch in enumerate(prog, 1):
        s.chords[b] = ch
    # one rhythm all the way (4 2 2 2 2 8), new notes every bar: the listener's rule, the steady frame
    # the distortion can crack against. Each note gets a different tone; the long notes warble.
    # over F the line steps through D, never B: B against the F bass is the tritone that rubbed
    bars = [("E5 B5 A5 G5 F#5 E5", False), ("F5 C6 D6 A5 G5 F5", True),
            ("G5 E6 D6 B5 A5 G5", False), ("A5 F6 E6 D6 C6 A5", True),
            ("C6 A5 G5 E5 D5 C5", False), ("A5 F5 E5 D5 C5 A4", True),
            ("B4 D#5 F#5 A5 B5 F#5", False)]
    lengths = [4, 2, 2, 2, 2, 8]
    duties = ["12.5", "50", "25"]
    for b, (notes, slip) in enumerate(bars, 1):
        parts = []
        for k, (p, l) in enumerate(zip(notes.split(), lengths)):
            fx = [f"duty {duties[(k + b) % 3]}"]
            if l == 8:
                fx.append("vib 40ct 6hz 100ms")
            if k == 0 and slip:
                fx.append(f"from {pname(midi(p) - 1)}")   # each F bar slides in from a half step below
            parts.append(f"{p}:{l}|{'_'.join(f.replace(' ', '_') for f in fx)}")
        line(s, b, " ".join(parts))
    line(s, 8, "D#5:20|vib_50ct_6hz")
    for b in range(1, 9):
        if b > 1:
            roll(s, b, prog[b - 1], around=52)
            bass(s, b, prog[b - 1])
    heavy(s, 1, "Em")                       # 4/4 -> 5/4: say so
    for b in range(2, 8):
        groove(s, b, hats=3, kick=12, snare=10)
    riser(s, 8)                             # 5/4 -> the triplet bar next
    return s


def stretch():
    s = Sec("stretch", "4/4", 1, grid=12, lanes={"pulse1": "duty 50 vol 12", "pulse2": "duty 12.5 vol 7"})
    s.chords = {1: "Em"}
    line(s, 1, "E6:3|vib_20ct_6hz B5:3|vib_20ct_6hz A5:3|vib_20ct_6hz G5:3|vib_20ct_6hz")
    line(s, 1, "E5:3 B4:3 A4:3 G4:3", lane="pulse2")          # in time now
    s.add("triangle", 0, 12, "E2", "from E3")
    # 5/4 -> 4/4 in triplets: crash, a kick on every beat, triplet snares climbing between
    hit(s, 0, 1, HISS, 15)
    for k in range(1, 12):
        if k % 3 == 0:
            hit(s, k, 1, KICK, 15)
        else:
            hit(s, k, 1, SNARE, 7 + k // 2)
    return s


def compress():
    s = Sec("compress", "4/4", 1, grid=20, lanes={"pulse1": "duty 50 vol 12", "pulse2": "duty 12.5 vol 8"})
    s.chords = {1: "Em"}
    down = "B6 A6 G6 F6 E6 D6 C6 B5 A5 G5 F5 E5 D5 C5 B4 A4".split()
    for k, p in enumerate(down):
        s.add("pulse1", k, 1, p, "", "mel")
        # pulse2 an octave down, in time. (The original ran the other way and crossed it; a sixth
        # below was tried next, and a falling scale in sixths is the same scale two notes late:
        # the ear flagged it as a flam.)
        s.add("pulse2", k, 1, pname(midi(p) - 12), "", "double")
    for k in range(0, 16, 2):
        s.add("triangle", k, 1, "E2", "", "bass")
    # the quintuplet snare roll is the riser into tamagoe
    for k in range(20):
        hit(s, k, 1, SNARE if k >= 8 else HISS, min(15, 4 + k * 11 // 19), fade=False)
    return s


def tamagoe():
    s = Sec("tamagoe", "4/4", 6, unit=1, lanes={"pulse1": "duty 50 vol 5", "pulse2": "duty 12.5 vol 2"})
    prog = ["E5", "F", "E5", "Am", "Dm", "E5"]
    for b, ch in enumerate(prog, 1):
        s.chords[b] = ch
    melody = ["E5:8 B5:4 A5:4", "F5:8 E5:4 C5:4",
              "B4:8|vib_20ct_5hz C5:4 E5:4",       # was B for 12 and two 16ths: now line 1's rhythm
              "A5:8 B5:4 C6:4", "B5:8|vib_20ct_5hz A5:4 F5:4", "E5:16|fade"]
    for b, spec in enumerate(melody, 1):
        line(s, b, spec)
    halo = ["B6", "C7", "A6", "E6", "A6", "B5"]     # the original's high pad, now struck with the bar
    for b, p in enumerate(halo, 1):
        s.add("pulse2", s.at(b), 16, p, "fade", "pad")
    for b, ch in enumerate(prog, 1):
        if b == 1:
            continue                        # the heavy bar pounds the bass itself
        root, _ = CHORDS[ch]
        r = near(root, 40)
        low = r if r <= 43 else r - 12
        s.add("triangle", s.at(b), 3, pname(low), "", "bass")
        above = s.sounding("pulse1", s.at(b, 4))
        second = low + 7 if above and bad(low, midi(above[0][2]), "E5") else low
        s.add("triangle", s.at(b, 4), 3, pname(second), "", "bass")
    # the tuplets give way to plain 4/4: the first bar says so as loudly as the others, then it breathes
    heavy(s, 1, "E5", stabs=False)
    for b in range(2, 6):
        for q in range(4):
            hit(s, s.at(b, q * 2), 2, HAT, 2)
    riser(s, 6, lo=1, hi=12)               # 4/4 -> 7/8 next
    return s


def binding():
    s = Sec("binding", "7/8", 8, unit=2, lanes={"pulse1": "duty 12.5 vol 8", "pulse2": "duty 12.5 vol 5"})
    for i in range(8):
        theme_bar(s, 1 + i, i)
    heavy(s, 1, "Em")                       # 4/4 -> 7/8
    tongue(s, 2, 8, vols=(6, 6, 4))
    for b in range(2, 9):
        bass(s, b, s.chords[b])
    for b in range(2, 8):
        groove(s, b, hats=3 if b < 5 else 4, kick=11 if b < 5 else 13, snare=9 if b < 5 else 11)
    riser(s, 8)                             # 7/8 -> 4/4 next
    return s


def shatter():
    s = Sec("shatter", "4/4", 8, unit=1, lanes={"pulse1": "duty 25 vol 14", "pulse2": "duty 25 vol 6"})
    prog = ["C", "D", "Em", "Em", "C", "D", "B7", "Em"]
    for b, ch in enumerate(prog, 1):
        s.chords[b] = ch
    head = "E6:4|from_B5 G6:4 B6:8|vib_30ct_6hz"
    melody = [head, "A6:8|vib_30ct_6hz F#6:4 A6:4", "B6:16|vib_30ct_6hz", "B6:4 A6:4 G6:4 D6:4",
              head, "A6:8|vib_30ct_6hz F#6:4 A6:4", "B6:4 A6:4 F#6:4 D#6:4", "E6:16|vib_40ct_6hz"]
    for b, spec in enumerate(melody, 1):
        line(s, b, spec)
    s.add("pulse2", s.at(1), 16, "C5", "arp 0,4,7,11 vol 6", "pad")
    s.add("pulse2", s.at(2), 16, "D5", "arp 0,4,7 vol 6", "pad")
    s.add("pulse2", s.at(3), 32, "E5", "arp 0,3,7,10 vol 6", "pad")
    # the second half harmonizes the tune a sixth below, struck with it (the original's line)
    line(s, 5, "C6:4|from_G5 E6:4 G6:8|vib_30ct_6hz", lane="pulse2", fx_all="vol 11")
    line(s, 6, "F#6:8|vib_30ct_6hz D6:4 F#6:4", lane="pulse2", fx_all="vol 11")
    line(s, 7, "D#6:4 F#6:4 D#6:4 B5:4", lane="pulse2", fx_all="vol 11")
    line(s, 8, "B5:16|vib_30ct_6hz", lane="pulse2", fx_all="vol 11")
    for b in range(1, 9):
        bass(s, b, prog[b - 1])
    for b in (1, 5):
        hit(s, s.at(b), 2, HISS, 15)
        for k, c in enumerate("hShKhSh"):
            letter, vol = {"K": (KICK, 15), "S": (SNARE, 13), "h": (HAT, 5)}[c]
            hit(s, s.at(b, k + 1), 2, letter, vol)
    for b in (2, 3, 4, 6, 7):
        groove(s, b, hats=5, kick=14, snare=12)
    riser(s, 8)                             # 4/4 -> 7/8 next
    return s


def bow():
    s = Sec("bow", "7/8", 6, unit=1, lanes={"pulse1": "duty 12.5 vol 6", "pulse2": "duty 12.5 vol 3"})
    for i in range(4):
        theme_bar(s, 1 + i, i)
    heavy(s, 1, "Em")                       # back home to 7/8, said once, then let go
    for b in range(2, 5):
        double(s, b, b, vol=3)
        hold_bass(s, b, s.chords[b])
    s.chords[5] = s.chords[6] = "Em"
    line(s, 5, "B5:6|duty_50_vol_9_fade G5:22|duty_50_vol_8_fade")   # the door chime closes the song
    s.add("pulse2", s.at(5), 28, "E4", "arp 0,3,7 vol 3 fade", "pad")
    hold_bass(s, 5, "Em")
    return s


ORDER = ["mask", "service", "crack", "endure", "fragments", "stretch", "compress", "tamagoe", "binding", "crack2", "shatter", "bow"]

FX = [  # applied after rendering by master.py; times are section names and bar ranges
    {"section": "fragments", "lanes": ["pulse1"], "filter": {"cutoff": [9000, 1500]},
     "crush": {"bits": [8, 4], "hold": [1, 10]}, "gate": {"rate": 12, "depth": [0.0, 0.45]},
     "why": "the fragmented mind: the melody stays whole while its sound darkens, grits and stutters"},
    {"section": "fragments", "lanes": ["pulse2"], "filter": {"cutoff": [7000, 1200]}, "crush": {"bits": [8, 5], "hold": [1, 6]},
     "why": "the accompaniment frays behind it"},
    {"section": "shatter", "lanes": ["pulse1", "pulse2"], "drive": [1.5, 2.5],
     "why": "the climax pushed into the red"},
    {"section": "shatter", "bars": [7, 8], "lanes": ["pulse1", "pulse2", "triangle", "noise"],
     "crush": {"bits": [8, 3], "hold": [1, 14]}, "gate": {"rate": 12, "depth": [0.0, 0.6]},
     "why": "shatter: the whole band breaks apart before bow puts it back together"},
]


def main():
    sections = {"mask": mask(), "service": service(), "crack": crack("crack", drive=False), "endure": endure(),
                "fragments": fragments(), "stretch": stretch(), "compress": compress(), "tamagoe": tamagoe(),
                "binding": binding(), "crack2": crack("crack2", drive=True), "shatter": shatter(), "bow": bow()}
    # the clash check: melody against everything else, chord by chord
    problems = []
    for sec in sections.values():
        for s, l, p, fx, tag in sec.notes["pulse1"]:
            if p == "-" or l < (sec.e or 2):      # 16th passing notes are allowed to pass
                continue
            ch = sec.chord_at(s) or "Em"
            for lane in ("pulse2", "triangle"):
                for o in sec.notes[lane]:
                    if o[2] == "-" or not (o[0] < s + l and o[0] + o[1] > s):
                        continue
                    arp = re.search(r"arp ([\d,]+)", o[3])
                    others = [midi(o[2]) + int(x) for x in arp.group(1).split(",")] if arp else [midi(o[2])]
                    for m in others:
                        if bad(midi(p), m, ch) and o[4] != "pad":
                            problems.append(f"{sec.name} |{s // sec.bar + 1}: melody {p} vs {lane} {pname(m)} ({o[4]}) over {ch}")
    text = [f'song "but we endure (rewrite)" tempo 180 profile nes',
            "; ear: until 0",
            "; written by contrib/ear/rewrite.py from the rules in contrib/ear/LESSONS.md; edit the generator, not this file",
            "", "play"] + [f"  {n}" for n in ORDER]
    for n in ORDER:
        text += ["", sections[n].text()]
    OUT.write_text("\n".join(text) + "\n")
    (OUT.with_suffix(".fx.json")).write_text(json.dumps(FX, indent=1) + "\n")
    print(f"wrote {OUT.relative_to(ROOT)} ({sum(len(v) for s in sections.values() for v in s.notes.values())} notes)")
    print(f"{len(problems)} clashes left between the melody and the rest:")
    for p in problems[:40]:
        print("  ", p)


if __name__ == "__main__":
    main()
