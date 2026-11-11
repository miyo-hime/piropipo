"""The ear's music theory: turns piro's note events into the numbers the page draws.

Every measure here is a best guess at a published idea, cut down far enough to read in one sitting.
research.md has the papers; the names below are the ones to look up.

- key: Krumhansl-Schmuckler key finding, on Krumhansl & Kessler's 1982 probe-tone profiles.
- chords: template matching over the pitch classes sounding in each beat group, bass weighted.
- harmonic tension: Lerdahl's chord distance from Tonal Pitch Space (2001), delta = i + j + k,
  measured from the home chord.
- note tension: the note's depth in its chord's "basic space" (root, fifth, chord, scale,
  chromatic), plus how hard it rubs against whatever else is sounding (an interval roughness
  table in the spirit of Plomp & Levelt 1965), plus the harmonic tension under it.
- syncopation: Longuet-Higgins & Lee (1984) on a metric hierarchy built from the meter.
- surprise: a small Markov model that learns the melody as it goes (IDyOM's short-term model,
  much simplified): a note is surprising if the song hasn't taught you to expect it yet.
- phrase comparison: onset sets for rhythm, transposition-free pitch distance for the tune.

Python 3.9, stdlib only.
"""

from __future__ import annotations

import math
import re
from collections import Counter, defaultdict

SHARPS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]
FLATS = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"]
PITCHED = ("pulse1", "pulse2", "triangle")

# Krumhansl & Kessler (1982) probe-tone ratings, tonic first.
KK_MAJOR = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88]
KK_MINOR = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17]

# How rough two notes sound together, by semitone distance mod 12 (0 smooth, 1 harsh).
# A best-guess table in the shape of the classic consonance rankings: the minor second and
# major seventh are worst, the tritone next, seconds and sevenths mild, thirds/sixths sweet.
ROUGHNESS = {0: 0.0, 1: 1.0, 2: 0.45, 3: 0.15, 4: 0.1, 5: 0.2, 6: 0.75, 7: 0.03, 8: 0.2, 9: 0.12, 10: 0.4, 11: 0.85}
INTERVAL_NAMES = {0: "unison", 1: "minor 2nd", 2: "major 2nd", 3: "minor 3rd", 4: "major 3rd", 5: "perfect 4th",
                  6: "tritone", 7: "perfect 5th", 8: "minor 6th", 9: "major 6th", 10: "minor 7th", 11: "major 7th"}

# (intervals, label suffix, simplicity penalty): the penalty keeps an arpeggio's passing
# ninth from turning every triad into something exotic.
CHORD_TYPES = [
    ((0, 4, 7), "", 0.0), ((0, 3, 7), "m", 0.0),
    ((0, 4, 7, 10), "7", 0.05), ((0, 3, 7, 10), "m7", 0.05), ((0, 4, 7, 11), "maj7", 0.06),
    ((0, 3, 6), "dim", 0.08), ((0, 4, 8), "aug", 0.12), ((0, 5, 7), "sus4", 0.12), ((0, 3, 6, 10), "m7b5", 0.09),
]

HARMONY_WEIGHT = {"pulse1": 0.2, "pulse2": 1.0, "triangle": 1.2}
EVEN_WEIGHT = {"pulse1": 1.0, "pulse2": 1.0, "triangle": 1.0}


# ---------------------------------------------------------------- pitch helpers

def fifths_pos(pc: int) -> int:
    """Where a pitch class sits on the circle of fifths, C=0, G=1, D=2 ... F=11."""
    return (pc * 7) % 12


def fifths_dist(a: int, b: int) -> int:
    d = abs(fifths_pos(a) - fifths_pos(b)) % 12
    return min(d, 12 - d)


def uses_flats(tonic: int, mode: str) -> bool:
    major = tonic if mode == "major" else (tonic + 3) % 12
    return fifths_pos(major) >= 6 and major != 6  # F, Bb, Eb, Ab, Db spell flat; F# spells sharp


def pc_name(pc: int, flats) -> str:
    """flats is either a key's own spelling list (key["names"]) or a plain sharps/flats choice."""
    if isinstance(flats, list):
        return flats[pc % 12]
    return (FLATS if flats else SHARPS)[pc % 12]


def spelling(tonic: int, mode: str) -> list:
    """The key's note names: flat keys spell flat, but a minor key's leading tone is always sharp (C# in D minor)."""
    names = list(FLATS if uses_flats(tonic, mode) else SHARPS)
    if mode == "minor":
        lt = (tonic + 11) % 12
        if "b" in names[lt]:
            names[lt] = SHARPS[lt]
    return names


def scale_of(tonic: int, mode: str) -> list:
    """The key's scale. Minor gets its raised seventh too: the V7 of a minor key is normal."""
    steps = [0, 2, 4, 5, 7, 9, 11] if mode == "major" else [0, 2, 3, 5, 7, 8, 10, 11]
    return [(tonic + s) % 12 for s in steps]


def diatonic_chain(tonic: int, mode: str) -> list:
    """The seven diatonic pitch classes in fifths order (Lerdahl's diatonic circle)."""
    major = tonic if mode == "major" else (tonic + 3) % 12
    start = (major + 5) % 12  # the fourth degree starts the chain: F C G D A E B in C
    return [(start + 7 * i) % 12 for i in range(7)]


def pearson(x: list, y: list) -> float:
    mx, my = sum(x) / len(x), sum(y) / len(y)
    num = sum((a - mx) * (b - my) for a, b in zip(x, y))
    den = math.sqrt(sum((a - mx) ** 2 for a in x) * sum((b - my) ** 2 for b in y))
    return num / den if den else 0.0


def find_key(weights: list) -> dict:
    """Krumhansl-Schmuckler: correlate the duration-weighted pitch-class histogram with every key."""
    scored = []
    for tonic in range(12):
        rotated = [weights[(tonic + i) % 12] for i in range(12)]
        scored.append((pearson(rotated, KK_MAJOR), tonic, "major"))
        scored.append((pearson(rotated, KK_MINOR), tonic, "minor"))
    scored.sort(reverse=True)
    r, tonic, mode = scored[0]
    runner = scored[1]
    flats = uses_flats(tonic, mode)
    return {"tonic": tonic, "mode": mode, "name": f"{pc_name(tonic, flats)} {mode}", "r": round(r, 3),
            "runner_up": f"{pc_name(runner[1], uses_flats(runner[1], runner[2]))} {runner[2]}",
            "margin": round(r - runner[0], 3), "flats": flats, "names": spelling(tonic, mode)}


# ---------------------------------------------------------------- Lerdahl's basic space

def basic_space(root: int, tones: list, key_scale: list) -> list:
    """Lerdahl's levels for a chord in a key: root, root+fifth, chord, scale (each includes the last)."""
    a = {root}
    b = a | ({(root + 7) % 12} if (root + 7) % 12 in tones else set())
    c = b | set(tones)
    d = c | set(key_scale)
    return [a, b, c, d]


def chord_distance(x: dict, y: dict, key: dict, key_shift: int = 0) -> int:
    """Lerdahl's delta(x -> y) = i + j + k within one key (i is how far the local key moved)."""
    chain = diatonic_chain(key["tonic"], key["mode"])
    if x["root"] in chain and y["root"] in chain:
        d = abs(chain.index(x["root"]) - chain.index(y["root"]))
        j = min(d, 7 - d)
    else:
        j = fifths_dist(x["root"], y["root"])
    sc = scale_of(key["tonic"], key["mode"])
    lx, ly = basic_space(x["root"], x["pcs"], sc), basic_space(y["root"], y["pcs"], sc)
    k = sum(len(ly[i] - lx[i]) for i in range(4))
    return key_shift + j + k


def depth(pc: int, chord: dict, key: dict) -> int:
    """0 root, 1 fifth, 2 other chord tone, 3 in the scale, 4 chromatic: how far from the chord's core."""
    for level, members in enumerate(basic_space(chord["root"], chord["pcs"], scale_of(key["tonic"], key["mode"]))):
        if pc in members:
            return level
    return 4


def tonic_chord(key: dict) -> dict:
    t = key["tonic"]
    third = (t + (4 if key["mode"] == "major" else 3)) % 12
    return {"root": t, "pcs": [t, third, (t + 7) % 12]}


# ---------------------------------------------------------------- meter

def groupings(pulses: int, denom) -> list:
    """Candidate beat groupings for a bar, the conventional one first."""
    if denom in (None, 4, 2, 1):
        fixed = {1: [[1]], 2: [[2]], 3: [[3]], 4: [[2, 2]], 6: [[3, 3]], 8: [[2, 2, 2, 2]]}
        if pulses in fixed:
            return fixed[pulses]
    out = []

    def comp(left, acc):
        if left == 0:
            out.append(acc)
            return
        for part in (2, 3):
            if part <= left:
                comp(left - part, acc + [part])

    comp(pulses, [])
    if not out:
        return [[pulses]]
    out.sort(key=lambda g: (-sum(1 for p in g if p == 2), g))  # 2+2+3 before 3+2+2: twos first is the common read
    return out[:6]


def parse_meter_spec(comment: str, grid: int, beats: int, total_slots: int):
    """'; 4/4, 8x 7/8' -> bar list, or None when the comment isn't a meter that fills the pattern."""
    text = comment.lstrip(";").strip()
    bars = []
    for item in text.split(","):
        m = re.fullmatch(r"\s*(?:(\d+)\s*x\s*)?(\d+)/(\d+)\s*", item)
        if not m:
            return None
        count, num, den = int(m.group(1) or 1), int(m.group(2)), int(m.group(3))
        slots_f = num * 4 / den * grid / beats
        pulse_f = 4 / den * grid / beats
        if slots_f != int(slots_f) or pulse_f != int(pulse_f) or pulse_f < 1:
            return None
        for _ in range(count):
            bars.append({"meter": f"{num}/{den}", "len": int(slots_f), "pulse": int(pulse_f), "pulses": num, "denom": den})
    if sum(b["len"] for b in bars) != total_slots:
        return None
    return bars


def box_bars(grid: int, beats: int, total_slots: int) -> list:
    pulse = grid // beats if grid % beats == 0 else 1
    pulses = beats if pulse > 1 or grid == beats else grid
    label = f"{beats}/4" if beats in (2, 3, 4, 5, 6) else f"{beats} beats"
    return [{"meter": label, "len": grid, "pulse": pulse, "pulses": pulses, "denom": 4 if beats in (2, 3, 4, 5, 6) else None}
            for _ in range(total_slots // grid)]


def metric_weights(bar: dict) -> list:
    """Per slot: 4 downbeat, 3 group start, 2 pulse, 1 half pulse, 0 anything finer."""
    starts, acc = set(), 0
    for g in bar["groups"]:
        starts.add(acc * bar["pulse"])
        acc += g
    half = bar["pulse"] // 2 if bar["pulse"] % 2 == 0 else 0
    out = []
    for o in range(bar["len"]):
        if o == 0:
            out.append(4)
        elif o in starts:
            out.append(3)
        elif o % bar["pulse"] == 0:
            out.append(2)
        elif half and o % half == 0:
            out.append(1)
        else:
            out.append(0)
    return out


DRUM_WEIGHT = {"kick": 1.0, "snare": 0.7, "hat": 0.2}


def drum_of(letter: str) -> str:
    """piro's noise lane: low letters thud, mid letters snare, high letters hiss."""
    pc = SHARPS.index(letter) if letter in SHARPS else FLATS.index(letter) if letter in FLATS else 11
    return "kick" if pc <= 3 else "snare" if pc <= 7 else "hat"


def infer_groupings(bars: list, notes: list) -> None:
    """For each meter in a section, pick the grouping whose group starts the drums and bass hit hardest."""
    by_meter = defaultdict(list)
    for b in bars:
        by_meter[(b["meter"], b["pulses"], b["pulse"])].append(b)
    for (meter, pulses, pulse), members in by_meter.items():
        evidence = Counter()
        for b in members:
            for n in notes:
                if b["start"] <= n["start"] < b["start"] + b["len"]:
                    o = n["start"] - b["start"]
                    if n["lane"] == "noise":
                        evidence[o] += DRUM_WEIGHT[drum_of(n["pitch"])] * (n["vol"] or 8) / 15
                    elif n["lane"] == "triangle":
                        evidence[o] += 0.6
                    else:
                        evidence[o] += 0.3 * (n["vol"] or 8) / 15
        best, best_score = None, -1e9
        for g in groupings(pulses, members[0]["denom"]):
            starts, acc = [], 0
            for part in g:
                starts.append(acc * pulse)
                acc += part
            inner = starts[1:]
            others = [p * pulse for p in range(1, pulses) if p * pulse not in starts]
            mean_in = sum(evidence[s] for s in inner) / len(inner) if inner else 0
            mean_out = sum(evidence[s] for s in others) / len(others) if others else 0
            score = mean_in - mean_out
            if score > best_score + 1e-9:
                best, best_score = g, score
        for b in members:
            b["groups"] = best


# ---------------------------------------------------------------- the song as notes

def notes_from_events(check: dict, sections: list) -> list:
    index = {(s["name"], s["play"]): s for s in sections}
    out = []
    for e in check["events"]:
        sec = index[(e["pattern"], e["play"])]
        g = sec["grid"]
        bar, slot = (int(x) for x in e["at"].split("."))
        ebar, eslot = (int(x) for x in e["end"].split("."))
        start = (bar - 1) * g + slot - 1
        end = (ebar - 1) * g + eslot - 1
        fx = e.get("effects") or {}
        arp = fx.get("arp", {}).get("offsets") if isinstance(fx.get("arp"), dict) else None
        slide = fx.get("from", {}).get("midi") if isinstance(fx.get("from"), dict) else None
        out.append({
            "lane": e["lane"], "section": sec["index"], "pitch": e["pitch"], "midi": e["midi"],
            "pc": e["midi"] % 12 if e["midi"] is not None else None,
            "start": start, "len": end - start + 1, "t0": e["start_seconds"], "t1": e["end_seconds"],
            "vol": e.get("volume"), "duty": e.get("duty"), "line": e["line"], "at": e["at"],
            "arp": arp, "slide": slide, "vib": "vib" in fx, "fade": bool(fx.get("fade")),
        })
    out.sort(key=lambda n: (n["section"], n["start"], n["lane"]))
    return out


def sounding_midis(n: dict) -> list:
    """Every pitch an arp note reaches; a plain note is just itself."""
    if n["midi"] is None:
        return []
    if n["arp"]:
        return sorted({n["midi"] + o for o in n["arp"]})
    return [n["midi"]]


def pc_weights(notes: list, lo: int = None, hi: int = None, lane_weight: dict = EVEN_WEIGHT) -> list:
    w = [0.0] * 12
    for n in notes:
        if n["lane"] not in PITCHED:
            continue
        a, b = n["start"], n["start"] + n["len"]
        if lo is not None:
            a, b = max(a, lo), min(b, hi)
        if b <= a:
            continue
        mids = sounding_midis(n)
        share = (b - a) * lane_weight[n["lane"]] / len(mids)
        for m in mids:
            w[m % 12] += share
    return w


# ---------------------------------------------------------------- chords

def label_chord(w: list, bass_pc, key: dict) -> dict:
    total = sum(w) or 1.0
    w = [x / total for x in w]
    best = None
    for root in range(12):
        for ivs, suffix, penalty in CHORD_TYPES:
            tones = [(root + i) % 12 for i in ivs]
            score = fit_of(tones, root, w, bass_pc) - penalty
            if root in scale_of(key["tonic"], key["mode"]):
                score += 0.03
            if best is None or score > best[0]:
                best = (score, root, tones, suffix)
    score, root, tones, suffix = best
    return {"root": root, "pcs": tones, "quality": suffix, "label": pc_name(root, key["names"]) + suffix, "fit": round(score, 3)}


def fit_of(tones: list, root: int, w: list, bass_pc) -> float:
    total = sum(w) or 1.0
    inside = sum(w[p] for p in tones) / total
    present = sum(1 for p in tones if w[p] / total > 0.02) / len(tones)
    return inside - 0.5 * (1 - inside) + 0.1 * present + (0.15 if bass_pc == root else 0)


def bass_of(pitched: list, lo: int, hi: int):
    """The lowest triangle note in the window, else the lowest note of anything."""
    for lanes in (("triangle",), PITCHED):
        sounding = [n for n in pitched if n["lane"] in lanes and n["start"] < hi and n["start"] + n["len"] > lo]
        if sounding:
            return min(sounding, key=lambda n: n["midi"])["midi"] % 12
    return None


def pedals(pitched: list, lo: int, hi: int) -> set:
    """Pitch classes struck again and again inside the window: a pedal or a pivot note the figure
    keeps returning to (the B5 in mask's broken chords). It colors every chord without defining any."""
    onsets = Counter(n["midi"] % 12 for n in pitched if lo <= n["start"] < hi)
    total = sum(onsets.values())
    return {pc for pc, c in onsets.items() if c >= 3 and total >= 5 and c / total >= 0.3}


def harmony_weights(pitched: list, lo: int, hi: int) -> list:
    w = pc_weights(pitched, lo, hi, HARMONY_WEIGHT)
    for pc in pedals(pitched, lo, hi):
        w[pc] *= 0.3
    return w


def chords_for_section(sec: dict, notes: list, key: dict) -> list:
    """One chord per bar, split at a beat group only when the bass moves and a new chord clearly fits.

    The melody gets a small vote (HARMONY_WEIGHT): it's the thing being judged against the
    chord, so it mustn't get to define the chord. A note the figure keeps returning to is a pedal
    and is discounted. With no bass playing, a bar is never split: the lowest note stands in for
    the bass, the way an organist reads the first note of a broken chord (the listener, mask |2-|5:
    "just E, C, A, B")."""
    pitched = [n for n in notes if n["lane"] in PITCHED]
    chords = []
    for b in sec["bars"]:
        lo_b, hi_b = b["start"], b["start"] + b["len"]
        wb = harmony_weights(pitched, lo_b, hi_b)
        if sum(wb) == 0:
            chords.append({"start": lo_b, "len": b["len"], "label": "", "root": None, "pcs": [], "quality": "", "fit": 0})
            continue
        bar_chord = label_chord(wb, bass_of(pitched, lo_b, hi_b), key)
        has_bass = any(n["lane"] == "triangle" and n["start"] < hi_b and n["start"] + n["len"] > lo_b for n in pitched)
        acc = 0
        for g in (b["groups"] if has_bass else [sum(b["groups"])]):
            lo, hi = lo_b + acc * b["pulse"], lo_b + (acc + g) * b["pulse"]
            acc += g
            w = harmony_weights(pitched, lo, hi)
            c = bar_chord
            if sum(w):
                bass_pc = bass_of(pitched, lo, hi)
                cand = label_chord(w, bass_pc, key)
                if cand["root"] != bar_chord["root"] and bass_pc is not None and bass_pc != bar_chord["root"] \
                        and cand["fit"] > fit_of(bar_chord["pcs"], bar_chord["root"], w, bass_pc) + 0.1:
                    c = cand
            chords.append(dict(c, start=lo, len=hi - lo))
    merged = []
    for c in chords:
        if merged and merged[-1]["label"] == c["label"]:
            merged[-1]["len"] += c["len"]
        else:
            merged.append(dict(c))
    return merged


def chord_at(chords: list, slot: int):
    for c in chords:
        if c["start"] <= slot < c["start"] + c["len"]:
            return c
    return None


# ---------------------------------------------------------------- tension and rubs

def note_rubs(n: dict, others: list, flats: bool) -> dict:
    """Slot by slot, the roughest interval between this note and anything else sounding.

    An arp counts as every pitch it reaches, all the time: it cycles about 60 times a second,
    so the ear hears it as the chord."""
    per_slot, pairs = [], {}
    mids = sounding_midis(n)
    for s in range(n["start"], n["start"] + n["len"]):
        best, hits = 0.0, set()
        for o in others:
            if o["lane"] == n["lane"] or not (o["start"] <= s < o["start"] + o["len"]):
                continue
            for om in sounding_midis(o):
                for m in mids:
                    span = abs(m - om)
                    r = ROUGHNESS[span % 12] * (1.0 if span < 12 else 0.8 if span < 24 else 0.65)
                    best = max(best, r)
                    if r >= 0.75:
                        hits.add((o["lane"], om, span % 12, round(r, 2)))
        for lane, om, ic, r in hits:
            p = pairs.setdefault((lane, om, ic), {"r": r, "slots": 0, "first": s})
            p["slots"] += 1
        per_slot.append(best)
    worst = None
    for (lane, om, ic), p in pairs.items():
        cand = {"r": p["r"], "lane": lane, "pitch": midi_name(om, flats), "interval": INTERVAL_NAMES[ic], "slots": p["slots"], "first": p["first"]}
        if worst is None or (cand["r"], cand["slots"]) > (worst["r"], worst["slots"]):
            worst = cand
    return {"avg": sum(per_slot) / len(per_slot) if per_slot else 0.0, "max": max(per_slot) if per_slot else 0.0, "worst": worst}


def parse_pitch(text: str):
    m = re.fullmatch(r"([A-G])([#b]?)(-?\d+)", text)
    if not m:
        return None
    pc = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}[m.group(1)] + {"#": 1, "b": -1, "": 0}[m.group(2)]
    return 12 * (int(m.group(3)) + 1) + pc


def written_name(lines: list, n: dict):
    """The note as the composer spelled it (piro reports Bb5 as A#5), when the score line still matches."""
    if not 0 < n["line"] <= len(lines):
        return None
    m = re.match(r"\s*(?:\d+\.)?\d+:\s+([A-G][#b]?-?\d+)", lines[n["line"] - 1])
    return m.group(1) if m and parse_pitch(m.group(1)) == n["midi"] else None


def midi_name(m: int, flats: bool = False) -> str:
    return f"{pc_name(m % 12, flats)}{m // 12 - 1}"


def score_notes(sec: dict, notes: list, key: dict) -> None:
    """Attach tension, depth, and rubs to every pitched note in a section."""
    sec_notes = [n for n in notes if n["section"] == sec["index"]]
    pitched = [n for n in sec_notes if n["lane"] in PITCHED]
    for n in pitched:
        weights, harm, dep = 0.0, 0.0, 0.0
        for s in range(n["start"], n["start"] + n["len"]):
            c = chord_at(sec["chords"], s)
            if c is None or c["root"] is None:
                continue
            weights += 1
            harm += c["tension"]
            dep += depth(n["midi"] % 12, c, key) / 4
        harm = harm / weights if weights else 0.0
        dep = dep / weights if weights else 0.5
        rub = note_rubs(n, pitched, key["names"])
        n["harm"] = round(harm, 3)
        n["depth"] = round(dep, 3)
        n["rub"] = round(0.5 * rub["max"] + 0.5 * rub["avg"], 3)
        n["worst_rub"] = rub["worst"]
        c0 = chord_at(sec["chords"], n["start"])
        n["chord"] = c0["label"] if c0 else ""
        n["role"] = role_name(n["midi"] % 12, c0, key) if c0 and c0["root"] is not None else ""
        n["tension"] = round(0.4 * harm + 0.4 * dep + 0.2 * n["rub"], 3)


def role_name(pc: int, chord: dict, key: dict) -> str:
    d = (pc - chord["root"]) % 12
    names = {0: "root", 3: "3rd", 4: "3rd", 7: "5th", 6: "5th" if chord["quality"] in ("dim", "m7b5") else "",
             8: "5th" if chord["quality"] == "aug" else "", 10: "7th", 11: "7th"}
    if pc in chord["pcs"] and names.get(d):
        return names[d]
    if pc in chord["pcs"]:
        return "chord tone"
    return "scale tone, not in chord" if pc in scale_of(key["tonic"], key["mode"]) else "outside the key"


# ---------------------------------------------------------------- rhythm

def syncopation(lane_notes: list, bars: list) -> dict:
    """Longuet-Higgins & Lee: an onset on a weak slot, then silence or a hold across a stronger one."""
    weights = {}
    for b in bars:
        for o, w in enumerate(b["weights"]):
            weights[b["start"] + o] = w
    onsets = sorted({n["start"] for n in lane_notes})
    per_bar = defaultdict(float)
    for i, s in enumerate(onsets):
        nxt = onsets[i + 1] if i + 1 < len(onsets) else max(weights) + 1
        strongest = max((weights.get(t, 0) for t in range(s + 1, nxt)), default=-1)
        if strongest > weights.get(s, 0):
            per_bar[bar_index(bars, s)] += strongest - weights.get(s, 0)
    return per_bar


def bar_index(bars: list, slot: int) -> int:
    for i, b in enumerate(bars):
        if b["start"] <= slot < b["start"] + b["len"]:
            return i
    return len(bars) - 1


class Surprise:
    """IDyOM's short-term model, much simplified: learns (interval, length) as the melody goes."""

    V = 25 * 16

    def __init__(self):
        self.c0, self.c1, self.c2 = Counter(), defaultdict(Counter), defaultdict(Counter)

    def _p(self, counts: Counter, sym, lower: float) -> float:
        n = sum(counts.values())
        if n == 0:
            return lower
        lam = n / (n + len(counts))
        return lam * counts[sym] / n + (1 - lam) * lower

    def step(self, ctx2, ctx1, sym) -> float:
        p0 = (self.c0[sym] + 1) / (sum(self.c0.values()) + self.V)
        p1 = self._p(self.c1[ctx1], sym, p0)
        p2 = self._p(self.c2[ctx2], sym, p1)
        self.c0[sym] += 1
        self.c1[ctx1][sym] += 1
        self.c2[ctx2][sym] += 1
        return -math.log2(p2)


# ---------------------------------------------------------------- phrases

def onset_offsets(lane_notes: list, lo: int, hi: int) -> set:
    return {n["start"] - lo for n in lane_notes if lo <= n["start"] < hi}


def sample_pitches(lane_notes: list, lo: int, hi: int, cell: int) -> list:
    out = []
    for s in range(lo, hi, cell):
        hit = [n for n in lane_notes if n["start"] <= s < n["start"] + n["len"] and n["midi"] is not None]
        out.append(hit[-1]["midi"] if hit else None)
    return out


def row_cells(lane_notes: list, lo: int, hi: int, pulse: int, flats: bool) -> dict:
    """The letter notation: one cell per pulse, Upper = a new note, lower = still held, . = silence."""
    offs = onset_offsets(lane_notes, lo, hi)
    cell = pulse
    while cell > 1 and any(o % cell for o in offs):
        cell = cell // 2 if cell % 2 == 0 else 1
    cells = []
    for s in range(lo, hi, cell):
        starting = [n for n in lane_notes if s <= n["start"] < s + cell and n["midi"] is not None]
        held = [n for n in lane_notes if n["start"] < s < n["start"] + n["len"] and n["midi"] is not None]
        if starting:
            name = re.sub(r"-?\d+$", "", starting[0].get("name") or pc_name(starting[0]["midi"] % 12, flats))
            cells.append({"t": name + ("+" if len(starting) > 1 else ""), "on": True})
        elif held:
            cells.append({"t": re.sub(r"-?\d+$", "", held[-1].get("name") or pc_name(held[-1]["midi"] % 12, flats)).lower(), "on": False})
        else:
            cells.append({"t": ".", "on": False})
    return {"cell": cell, "cells": cells}


def compare_rows(lane_notes: list, a: dict, b: dict) -> dict:
    """How row b differs from row a: rhythm (onsets) and tune (pitch, with any transposition removed)."""
    oa, ob = onset_offsets(lane_notes, a["start"], a["start"] + a["len"]), onset_offsets(lane_notes, b["start"], b["start"] + b["len"])
    union = oa | ob
    rhythm = len(oa ^ ob) / len(union) if union else 0.0
    cell = math.gcd(a.get("cell", 1) or 1, b.get("cell", 1) or 1) or 1
    pa = sample_pitches(lane_notes, a["start"], a["start"] + a["len"], cell)
    pb = sample_pitches(lane_notes, b["start"], b["start"] + b["len"], cell)
    pairs = [(x, y) for x, y in zip(pa, pb) if x is not None and y is not None]
    if not pairs:
        return {"rhythm": round(rhythm, 3), "pitch": None, "contour": None, "shift": 0, "moved": sorted(oa ^ ob)}
    diffs = sorted(y - x for x, y in pairs)
    shift = diffs[len(diffs) // 2]
    exact = sum(min(abs(y - x), 7) for x, y in pairs) / (7 * len(pairs))
    contour = sum(min(abs(y - shift - x), 7) for x, y in pairs) / (7 * len(pairs))
    return {"rhythm": round(rhythm, 3), "pitch": round(exact, 3), "contour": round(contour, 3), "shift": shift,
            "only_a": sorted(oa - ob), "only_b": sorted(ob - oa)}


NEAR_MISS = 0.35  # up to this share of note starts moved, a rhythm reads as "almost the same"


def verdict(cmp: dict) -> tuple:
    """the listener's rule: vary the rhythm or the notes, not both. A big deliberate change reads as
    contrast; the trouble is the near miss, a rhythm that's almost the same while the notes move."""
    r, p, c = cmp["rhythm"], cmp["pitch"], cmp["contour"]
    if p is None:
        return ("silent", "One of the two lines is silent.")
    if r == 0 and p <= 0.03:
        return ("repeat", "Repeat: same rhythm, same notes.")
    if r == 0:
        return ("answer", "Same rhythm, new notes: it answers the line it echoes.")
    if c <= 0.08:
        return ("rerhythm", "Same tune, new rhythm.")
    if r <= NEAR_MISS:
        return ("near-miss", "Almost the same rhythm, and new notes on top: complex on complex.")
    return ("contrast", "A clearly different rhythm and tune: contrast, not a variation.")


def same_dress(sn: list, a: dict, b: dict, melody_lane: str) -> list:
    """What else changed between two rows with the same melody: volume, tone, the other lanes."""
    changes = []
    def lane_notes(lane, row):
        return [(n["start"] - row["start"], n["len"], n["midi"], n["vol"], n["duty"]) for n in sn
                if n["lane"] == lane and row["start"] <= n["start"] < row["start"] + row["len"]]
    ma, mb = lane_notes(melody_lane, a), lane_notes(melody_lane, b)
    va = sum(x[3] or 0 for x in ma) / len(ma) if ma else 0
    vb = sum(x[3] or 0 for x in mb) / len(mb) if mb else 0
    if abs(va - vb) >= 1:
        changes.append(f"the melody is {'louder' if vb > va else 'quieter'}")
    if [x[4] for x in ma] != [x[4] for x in mb]:
        changes.append("the melody's tone (duty) changes")
    for lane in ("pulse2", "triangle", "noise"):
        if lane != melody_lane and lane_notes(lane, a) != lane_notes(lane, b):
            changes.append(f"{lane} plays something different")
    return changes


# ---------------------------------------------------------------- the whole analysis

def analyze(song_text: str, check: dict, meta: dict) -> dict:
    lines = song_text.splitlines()
    head = re.search(r'^song\s+"([^"]*)"\s+tempo\s+(\d+)', song_text, re.M)
    title, tempo = (head.group(1), int(head.group(2))) if head else ("untitled", 120)

    raw_sections = check["stats"]["sections"]
    sections = []
    for i, s in enumerate(raw_sections):
        slot_seconds = 60.0 / tempo * s["beats"] / s["grid"]
        total = int(round((s["end_seconds"] - s["start_seconds"]) / slot_seconds))
        comment = lines[s["line"]] if s["line"] < len(lines) else ""
        bars = parse_meter_spec(comment, s["grid"], s["beats"], total) if comment.strip().startswith(";") else None
        meter_source = "the score's meter comment" if bars else "piro's pattern boxes"
        meter_comment = comment.strip() if bars else ""
        if not bars:
            bars = box_bars(s["grid"], s["beats"], total)
        acc = 0
        for n, b in enumerate(bars):
            b.update({"n": n + 1, "start": acc, "t0": round(s["start_seconds"] + acc * slot_seconds, 6),
                      "t1": round(s["start_seconds"] + (acc + b["len"]) * slot_seconds, 6)})
            acc += b["len"]
        sections.append({"index": i, "name": s["pattern"], "play": s["play"], "transpose": s["transpose"],
                         "grid": s["grid"], "beats": s["beats"], "line": s["line"], "slots": total,
                         "slot_seconds": slot_seconds, "t0": s["start_seconds"], "t1": s["end_seconds"],
                         "bars": bars, "meter_source": meter_source, "meter_comment": meter_comment})

    notes = notes_from_events(check, sections)
    key = find_key(pc_weights(notes))
    home = tonic_chord(key)
    flats = key["names"]
    for n in notes:
        if n["midi"] is not None:
            n["name"] = written_name(lines, n) or midi_name(n["midi"], flats)

    melody_lane = meta.get("melody", "pulse1")
    surprise_model, ctx = Surprise(), [("start",), ("start",)]
    prev_melody = None
    lanes_present = sorted({n["lane"] for n in notes}, key=lambda l: ["pulse1", "pulse2", "triangle", "noise"].index(l) if l in ("pulse1", "pulse2", "triangle", "noise") else 9)

    for sec in sections:
        sn = [n for n in notes if n["section"] == sec["index"]]
        infer_groupings(sec["bars"], sn)
        for b in sec["bars"]:
            b["weights"] = metric_weights(b)
        local = find_key(pc_weights(sn)) if any(n["lane"] in PITCHED for n in sn) else key
        sec["key"] = local
        sec["chords"] = chords_for_section(sec, sn, key)
        for c in sec["chords"]:
            if c["root"] is not None:
                c["tension_raw"] = chord_distance(home, c, key)
                c["tension"] = round(min(c["tension_raw"] / 13, 1.0), 3)
            else:
                c["tension_raw"], c["tension"] = None, 0.0
        score_notes(sec, notes, key)

        # melody intervals, surprise
        mel = [n for n in sn if n["lane"] == melody_lane and n["midi"] is not None]
        for n in mel:
            if prev_melody is not None:
                iv = n["midi"] - prev_melody["midi"]
                n["interval"] = iv
                n["fifths_jump"] = fifths_dist(n["midi"] % 12, prev_melody["midi"] % 12)
            else:
                iv = 0
                n["interval"], n["fifths_jump"] = None, None
            sym = (max(-12, min(12, iv)), min(n["len"], 16))
            n["surprise"] = round(surprise_model.step(tuple(ctx[-2:]), ctx[-1], sym), 3)
            ctx.append(sym)
            prev_melody = n

        # per-lane syncopation per bar
        sync = {lane: syncopation([n for n in sn if n["lane"] == lane], sec["bars"]) for lane in lanes_present}

        # per-bar curves
        sounding = {lane: bytearray(sec["slots"] + 1) for lane in lanes_present}
        for n in sn:
            for s in range(n["start"], min(n["start"] + n["len"], sec["slots"])):
                sounding[n["lane"]][s] = 1
        per_bar = []
        for i, b in enumerate(sec["bars"]):
            lo, hi = b["start"], b["start"] + b["len"]
            mel_slots, mel_t = 0, 0.0
            for n in mel:
                ov = min(hi, n["start"] + n["len"]) - max(lo, n["start"])
                if ov > 0:
                    mel_slots += ov
                    mel_t += ov * n["tension"]
            harm = [chord_at(sec["chords"], s) for s in range(lo, hi)]
            harm_t = [c["tension"] for c in harm if c and c["root"] is not None]
            onsets = [n for n in mel if lo <= n["start"] < hi]
            leaps = [abs(n["interval"]) for n in onsets if n.get("interval") is not None]
            texture = sum(sum(sounding[lane][lo:hi]) for lane in lanes_present) / b["len"]
            per_bar.append({
                "bar": b["n"], "t0": b["t0"], "t1": b["t1"],
                "melodic_tension": round(mel_t / mel_slots, 3) if mel_slots else None,
                "harmonic_tension": round(sum(harm_t) / len(harm_t), 3) if harm_t else None,
                "syncopation": round(sync[melody_lane][i] / b["pulses"], 3) if melody_lane in sync else 0,
                "density": round(len(onsets) / b["pulses"], 3),
                "texture": round(texture, 3),
                "leap": round(sum(leaps) / len(leaps), 2) if leaps else 0,
                "surprise": round(sum(n["surprise"] for n in onsets) / len(onsets), 3) if onsets else None,
                "rests": round(1 - mel_slots / b["len"], 3),
                "sync_by_lane": {lane: round(sync[lane][i] / b["pulses"], 3) for lane in lanes_present},
            })
        sec["per_bar"] = per_bar

        # rows and phrase comparisons for 1, 2, 4 bars a row
        sec["layouts"] = {}
        for per in (1, 2, 4):
            rows, run = [], []
            for b in sec["bars"]:
                if run and (run[-1]["meter"] != b["meter"] or len(run) == per):
                    rows.append(run)
                    run = []
                run.append(b)
            if run:
                rows.append(run)
            out_rows = []
            for k, run in enumerate(rows):
                lo, ln = run[0]["start"], sum(b["len"] for b in run)
                row = {"bars": [b["n"] for b in run], "start": lo, "len": ln, "meter": run[0]["meter"]}
                cells = {}
                for lane in ("pulse1", "pulse2", "triangle"):
                    lane_notes = [n for n in sn if n["lane"] == lane]
                    cells[lane] = row_cells(lane_notes, lo, lo + ln, run[0]["pulse"], flats)
                row["cells"] = cells
                row["cell"] = cells[melody_lane]["cell"] if melody_lane in cells else run[0]["pulse"]
                out_rows.append(row)
            mel_all = [n for n in sn if n["lane"] == melody_lane]
            def judged(j, r):
                cmp = compare_rows(mel_all, out_rows[j], r)
                kind, words = verdict(cmp)
                cmp.update({"kind": kind, "words": words})
                if kind == "repeat":
                    cmp["dress"] = same_dress(sn, out_rows[j], r, melody_lane)
                return cmp

            def head(row):
                first = sec["bars"][row["bars"][0] - 1]
                return {"start": row["start"], "len": first["len"], "cell": row.get("cell", 1)}

            for k, r in enumerate(out_rows):
                # the listener's rule is about each line against the one before it (the call and its
                # answer). Separately, a line whose opening bar matches an earlier line is a
                # reprise of it, and gets judged against that line too.
                r["vs_prev"], r["echo"] = None, None
                same_shape = lambda a: a["len"] == r["len"] and a["meter"] == r["meter"]
                if k > 0 and same_shape(out_rows[k - 1]):
                    cmp = judged(k - 1, r)
                    if cmp["kind"] == "near-miss" and not cmp.get("only_b") and cmp.get("only_a"):
                        # the only difference is this line holding its last note where the other kept
                        # moving: a cadence, not a near miss
                        mine = onset_offsets(mel_all, r["start"], r["start"] + r["len"])
                        if mine and min(cmp["only_a"]) > max(mine):
                            cmp.update({"kind": "cadence", "words": "Same rhythm until this line holds its last note: a cadence."})
                    if cmp["pitch"] is not None:
                        r["vs_prev"] = cmp
                best = None
                for j in range(k - 1):
                    if not same_shape(out_rows[j]) or len(r["bars"]) < 2:
                        continue
                    h = compare_rows(mel_all, head(out_rows[j]), head(r))
                    if h["pitch"] is None or h["rhythm"] > 0 or h["contour"] > 0.05:
                        continue
                    cmp = judged(j, r)
                    score = cmp["rhythm"] + (cmp["pitch"] or 0)
                    if best is None or score < best[0]:
                        best = (score, j, cmp)
                if best:
                    r["echo"] = dict(best[2], ref=best[1])
            matrix = []
            for a in out_rows:
                line = []
                for b in out_rows:
                    if a["len"] == b["len"] and a["meter"] == b["meter"]:
                        c = compare_rows(mel_all, a, b)
                        line.append({"rhythm": c["rhythm"], "pitch": c["pitch"], "contour": c["contour"]})
                    else:
                        line.append(None)
                matrix.append(line)
            sec["layouts"][str(per)] = {"rows": out_rows, "matrix": matrix}

    findings = find_problems(sections, notes, key, melody_lane, flats)
    timeline = []
    for sec in sections:
        for pb in sec["per_bar"]:
            timeline.append(dict(pb, section=sec["index"]))
    load_bars(timeline)
    findings += busy_bars(timeline, sections)
    findings.sort(key=lambda f: (f["t0"], f["kind"]))

    for n in notes:
        n.pop("pc", None)
    return {
        "title": title, "tempo": tempo, "key": key, "melody": melody_lane, "lanes": lanes_present,
        "sections": sections, "notes": notes, "findings": findings, "timeline": timeline,
        "piro_harmony": check.get("harmony", {}).get("clashes", []),
        "weights": {"tension": "0.4 harmonic + 0.4 depth in chord + 0.2 roughness", "roughness": ROUGHNESS},
    }


def load_bars(timeline: list) -> None:
    """One 'how much is going on' number per bar: the mean of each measure scaled across the song."""
    keys = ["melodic_tension", "syncopation", "density", "texture", "surprise", "leap"]
    span = {}
    for k in keys:
        vals = [b[k] for b in timeline if b.get(k) is not None]
        span[k] = (min(vals), max(vals)) if vals else (0, 1)
    for b in timeline:
        parts = {}
        for k in keys:
            v = b.get(k)
            lo, hi = span[k]
            parts[k] = 0.0 if v is None or hi == lo else (v - lo) / (hi - lo)
        b["load_parts"] = {k: round(v, 3) for k, v in parts.items()}
        b["load"] = round(sum(parts.values()) / len(parts), 3)


def find_problems(sections: list, notes: list, key: dict, melody_lane: str, flats: bool) -> list:
    out = []

    def add(kind, severity, sec, bars, title, detail, lever):
        b0 = next(b for b in sec["bars"] if b["n"] == bars[0])
        b1 = next(b for b in sec["bars"] if b["n"] == bars[-1])
        out.append({"id": f"{kind}-{sec['index']}-{bars[0]}", "kind": kind, "severity": severity, "section": sec["index"],
                    "bars": bars, "t0": b0["t0"], "t1": b1["t1"], "title": title, "detail": detail, "lever": lever})

    for sec in sections:
        sn = [n for n in notes if n["section"] == sec["index"]]
        mel = [n for n in sn if n["lane"] == melody_lane and n["midi"] is not None]
        rows = sec["layouts"]["2"]["rows"]
        where = f"{sec['name']}"

        # 1. complex on complex: the near-miss rhythm under new notes
        for k, r in enumerate(rows):
            v = r.get("vs_prev")
            if v and v["kind"] == "near-miss":
                j = k - 1
                moved = []
                for o in v.get("only_a", []):
                    moved.append(f"at {slot_words(sec, r['start'] + o)} line {j + 1} starts a note and line {k + 1} doesn't")
                for o in v.get("only_b", []):
                    moved.append(f"at {slot_words(sec, r['start'] + o)} line {k + 1} starts a note line {j + 1} doesn't")
                shift = v["shift"]
                moved_by = f"moved {abs(shift)} semitone{'s' if abs(shift) != 1 else ''} {'down' if shift < 0 else 'up'}" if shift else "at the same pitch"
                add("complex", "warn", sec, r["bars"],
                    f"{where} line {k + 1} vs line {j + 1}: almost the same rhythm, new notes",
                    f"Line {k + 1} follows line {j + 1}'s shape ({moved_by}), but {len(v.get('only_a', [])) + len(v.get('only_b', []))} note start"
                    f"{'s' if len(v.get('only_a', [])) + len(v.get('only_b', [])) != 1 else ''} moved, and the tune differs "
                    f"{round(v['contour'] * 100)}% even after that shift. " + ("Where: " + "; ".join(moved[:4]) + "." if moved else ""),
                    f"Give line {k + 1} line {j + 1}'s exact rhythm and let the notes carry the difference, or change the rhythm on purpose and keep the notes.")

        # 2. phrase ends that never resolve
        for k, r in enumerate(rows):
            lo, hi = r["start"], r["start"] + r["len"]
            inside = [n for n in mel if lo <= n["start"] < hi]
            if not inside:
                continue
            last = inside[-1]
            after = [n for n in mel if n["start"] >= hi]
            nxt = after[0] if after else None
            c = chord_at(sec["chords"], last["start"] + last["len"] - 1)
            if not c or c["root"] is None:
                continue
            d = depth(last["midi"] % 12, c, key)
            rub = last.get("worst_rub")
            resolves = False
            if nxt is not None:
                step = nxt["midi"] - last["midi"]
                nc = chord_at(sec["chords"], nxt["start"]) if nxt["section"] == sec["index"] else None
                if 1 <= abs(step) <= 2 and (nc is None or depth(nxt["midi"] % 12, nc, key) <= 2):
                    resolves = True
            # a scale tone off the chord (D6 over C in mask |3: a ninth) is color, not a problem; the listener
            # called that melody fine. It's unresolved when it actually rubs, or sits outside the key.
            if (d >= 4 or (rub and rub["r"] >= 0.85)) and not resolves:
                pcs = " ".join(pc_name(p, flats) for p in c["pcs"])
                steps = []
                for delta in (-1, -2, 1, 2):
                    pc = (last["midi"] + delta) % 12
                    if pc in c["pcs"]:
                        steps.append(f"{'down' if delta < 0 else 'up'} to {midi_name(last['midi'] + delta, flats)}")
                rubtxt = f" It rubs a {rub['interval']} against {rub['lane']}'s {rub['pitch']}." if rub else ""
                nxttxt = f" The next melody note is {nxt['name']}, which isn't a step to a chord tone." if nxt else " Nothing follows it."
                bar_n = sec["bars"][bar_index(sec["bars"], last["start"])]["n"]
                title = (f"{where} |{bar_n}: line {k + 1} ends on {last['name']}, not in the {c['label']} chord" if d >= 3 else
                         f"{where} |{bar_n}: line {k + 1} ends on {last['name']}, rubbing against {rub['lane']}'s {rub['pitch']}")
                add("unresolved", "warn", sec, [bar_n], title,
                    f"{last['name']} sits over {c['label']} ({pcs}), so it's a {role_name(last['midi'] % 12, c, key)}.{rubtxt}{nxttxt}",
                    ("Step it " + " or ".join(steps) + " before the line ends." if steps else "End the line on a chord tone."))

        # 3. short clashes piro's harmony advisory doesn't report (it only counts 3/4 of a beat or longer)
        seen = set()
        for n in mel:
            w = n.get("worst_rub")
            if not w or w["r"] < 0.85:
                continue
            bar_n = sec["bars"][bar_index(sec["bars"], w["first"])]["n"]
            if (bar_n, w["lane"]) in seen:
                continue
            seen.add((bar_n, w["lane"]))
            add("rub", "info", sec, [bar_n],
                f"{where} |{bar_n}: {n['name']} against {w['lane']}'s {w['pitch']} ({w['interval']})",
                f"For {w['slots']} slot{'s' if w['slots'] != 1 else ''} the melody's {n['name']} and {w['lane']}'s {w['pitch']} sound a {w['interval']} apart. "
                "Too short for piro's own harmony advisory to report it.",
                "Fine if you mean it; otherwise move one of them a half step.")

        # 4. never leaves home / never comes home
        tens = [n["tension"] for n in mel]
        if len(tens) >= 8:
            spread = max(tens) - min(tens)
            if max(tens) < 0.3:
                add("flat", "info", sec, [sec["bars"][0]["n"], sec["bars"][-1]["n"]], f"{where}: the melody never leaves home",
                    f"Melody tension stays under 0.3 the whole section (peak {max(tens):.2f}).", "Let one phrase lean on a note outside the chord before it resolves.")
            ends = []
            for r in rows:
                inside = [n for n in mel if r["start"] <= n["start"] < r["start"] + r["len"]]
                if inside:
                    ends.append(inside[-1]["tension"])
            if ends and min(ends) > 0.45:
                add("homeless", "warn", sec, [sec["bars"][0]["n"], sec["bars"][-1]["n"]], f"{where}: no line ends somewhere calm",
                    f"Every line ends with tension above 0.45 (lowest {min(ends):.2f}).", "Let at least one line land on a chord tone of a home-ish chord.")
            if spread < 0.12:
                add("monotone", "info", sec, [sec["bars"][0]["n"], sec["bars"][-1]["n"]], f"{where}: the tension curve is flat",
                    f"Melody tension only moves {spread:.2f} across the whole section.", "Give the curve somewhere to go: a lean, then a landing.")

        # 6. a voice copying the melody a moment late
        for run in late_copies(sec, sn, melody_lane):
            ms, lane, oct_ = round(run["lag_s"] * 1000), run["lane"], run["octave"]
            where_oct = {0: "", 1: ", an octave down", 2: ", two octaves down", -1: ", an octave up"}.get(oct_, f", {abs(oct_)} octaves off")
            harsh = run["harsh"]
            harsh_txt = (f" While it lags it also sits under the melody's next note, and {len(harsh)} of those {run['overlaps']} overlaps are harsh: "
                         + "; ".join(f"{h['mel']} over {h['copy']} ({h['interval']}, |{h['bar']})" for h in harsh[:4]) + "."
                         if harsh else f" Its {run['overlaps']} overlaps with the melody's next notes are all consonant, so the trouble is timing, not pitch.")
            if run["lag_s"] < 0.15:
                add("flam", "warn", sec, [run["bars"][0], run["bars"][-1]],
                    f"{where} |{run['bars'][0]}–|{run['bars'][-1]}: {lane} copies the melody {run['lag']} slot{'s' if run['lag'] != 1 else ''} late ({ms} ms){where_oct}",
                    f"{len(run['pairs'])} notes in a row, each the melody's note {ms} ms later. A doubling that doesn't start with the voice it doubles "
                    "won't fuse into one voice, and at under 150 ms it's too close to hear as an echo, so the ear hears a flam: one line, sloppily struck twice." + harsh_txt,
                    "Start the copy with the melody and get the drive from repeated notes instead (the double tongue: ta · ta ka), "
                    "or push it to an eighth or more so it reads as an echo.")
            else:
                add("echo", "info", sec, [run["bars"][0], run["bars"][-1]],
                    f"{where} |{run['bars'][0]}–|{run['bars'][-1]}: {lane} echoes the melody {run['lag']} slots later ({ms} ms){where_oct}",
                    f"{len(run['pairs'])} notes in a row. Far enough behind to read as an echo, not a flam." + harsh_txt,
                    "Fine if it's meant as an echo; check the harsh overlaps.")

        # 5. repeats with nothing changed around them
        for k, r in enumerate(rows):
            for v, j in ((r.get("vs_prev"), k - 1), (r.get("echo"), (r.get("echo") or {}).get("ref"))):
                if not v or v["kind"] != "repeat" or v.get("dress"):
                    continue
                add("repeat", "info", sec, r["bars"], f"{where} line {k + 1} repeats line {j + 1} exactly",
                    "Same melody, same volume, same accompaniment. The composer's skill asks for one audible difference per repeat.",
                    "Change one thing: the echo, the octave, the ending.")

    return out


def late_copies(sec: dict, sn: list, melody_lane: str) -> list:
    """Runs of 4+ notes where another lane plays the melody's pitch classes a constant few slots later.

    Found by the listener's ear in mask |6-|9: pulse2 doubling the melody an octave down, one slot (83 ms)
    late. Every overlap there is consonant (B5 over E4 is a fifth), so the roughness table can't
    hear it; the fault is onset asynchrony. Voices that double each other fuse only when they start
    together (Huron, Tone and Voice, 2001: the onset-synchrony principle)."""
    mel = sorted((n for n in sn if n["lane"] == melody_lane and n["midi"] is not None), key=lambda n: n["start"])
    at = {}
    for m in mel:
        at.setdefault(m["start"], m)
    max_lag = max(1, int(0.35 / sec["slot_seconds"]))
    runs = []
    for lane in PITCHED:
        if lane == melody_lane:
            continue
        copies = sorted((n for n in sn if n["lane"] == lane and n["midi"] is not None and not n["arp"]), key=lambda n: n["start"])
        cur = None

        def close():
            if cur and len(cur["pairs"]) >= 4:
                runs.append(cur)

        for q in copies:
            hit = None
            for lag in range(1, max_lag + 1):
                m = at.get(q["start"] - lag)
                if m and (m["midi"] - q["midi"]) % 12 == 0:
                    hit = (lag, (m["midi"] - q["midi"]) // 12, m)
                    break
            if hit and cur and (cur["lag"], cur["octave"]) == hit[:2] and mel.index(hit[2]) == mel.index(cur["pairs"][-1][0]) + 1:
                cur["pairs"].append((hit[2], q))
            else:
                close()
                cur = {"lane": lane, "lag": hit[0], "octave": hit[1], "pairs": [(hit[2], q)]} if hit else None
        close()
    for run in runs:
        run["lag_s"] = run["lag"] * sec["slot_seconds"]
        run["bars"] = sorted({sec["bars"][bar_index(sec["bars"], q["start"])]["n"] for _, q in run["pairs"]})
        harsh, overlaps = [], 0
        for m, q in run["pairs"]:
            nxt = next((n for n in mel if q["start"] < n["start"] < q["start"] + q["len"]), None)
            if not nxt:
                continue
            overlaps += 1
            span = abs(nxt["midi"] - q["midi"])
            r = ROUGHNESS[span % 12] * (1.0 if span < 12 else 0.8 if span < 24 else 0.65)
            if r >= 0.6:
                harsh.append({"mel": nxt["name"], "copy": q["name"], "interval": INTERVAL_NAMES[span % 12],
                              "bar": sec["bars"][bar_index(sec["bars"], nxt["start"])]["n"], "r": round(r, 2)})
        run["harsh"], run["overlaps"] = harsh, overlaps
        run["pairs"] = [(m["start"], q["start"]) for m, q in run["pairs"]]
    return runs


def busy_bars(timeline: list, sections: list) -> list:
    """Bars where most of the load measures are high together: everything at once, nowhere to rest."""
    out = []
    for b in timeline:
        high = [k for k, v in b["load_parts"].items() if v >= 0.6]
        if len(high) >= 4:
            sec = sections[b["section"]]
            out.append({"id": f"busy-{b['section']}-{b['bar']}", "kind": "busy", "severity": "warn", "section": b["section"],
                        "bars": [b["bar"]], "t0": b["t0"], "t1": b["t1"],
                        "title": f"{sec['name']} |{b['bar']}: everything at once",
                        "detail": "High at the same time: " + ", ".join(k.replace("_", " ") for k in high) + ".",
                        "lever": "Pick the one thing this bar is about and calm the others."})
    return out


def slot_words(sec: dict, slot: int) -> str:
    b = sec["bars"][bar_index(sec["bars"], slot)]
    o = slot - b["start"]
    beat = o // b["pulse"] + 1
    unit = "eighth" if b.get("denom") == 8 else "beat"
    extra = "" if o % b["pulse"] == 0 else " (off the beat)"
    return f"|{b['n']} {unit} {beat}{extra}"
