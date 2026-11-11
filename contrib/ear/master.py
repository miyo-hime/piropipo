#!/usr/bin/env python3
"""Render a song all the way through, add the effects piro can't, and write something to listen to.

    python3 contrib/ear/master.py contrib/ear/songs/but-we-endure-rewrite.piro

Renders each lane on its own with piro, applies the song's .fx.json (if there is one) to the lanes
and stretches of time it names, mixes, and writes contrib/ear/out/<song>.wav and .mp3 (the mp3 needs ffmpeg). Needs numpy.

An .fx.json is a list of effects:
  {"section": "fragments", "bars": [7, 8], "lanes": ["pulse1"],
   "crush": {"bits": [8, 4], "hold": [1, 4]}, "drive": [1.5, 3.0], "why": "..."}
- section: every play of that pattern; bars (optional) narrows it to a bar range of the section.
- crush: bitcrush. bits is the bit depth and hold the sample-and-hold (decimation) factor, each
  ramping from the first value at the start of the window to the second at the end.
- drive: soft-clip overdrive gain, ramping the same way.
- filter: {"cutoff": [9000, 1400]} a two-pole low-pass whose cutoff (Hz) slides across the window.
- gate: {"rate": 12, "depth": [0, 0.5]} chops the sound at `rate` Hz (12 is 16ths at tempo 180),
  the depth ramping from nothing to half. The notes stay whole; the sound around them breaks up.

Effects run in that order: drive, filter, crush, gate. On a chip pulse (already a hard square)
drive and a light crush barely register, so the filter and the gate do most of the work.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import wave
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import build  # noqa: E402

OUT = HERE / "out"


def bar_counts(text: str) -> dict:
    """pattern name -> bars, from the '; Nx M' meter comment under each pattern header (else its boxes)."""
    out, lines = {}, text.split("\n")
    for i, line in enumerate(lines):
        m = re.match(r"pattern\s+(\S+).*?bars\s+(\d+)", line)
        if m:
            nxt = lines[i + 1] if i + 1 < len(lines) else ""
            c = re.match(r";\s*(\d+)x\s+\d+/\d+\s*$", nxt)
            out[m.group(1)] = int(c.group(1)) if c else int(m.group(2))
    return out


def ramp(a, b, n):
    return np.linspace(a, b, n) if n > 0 else np.zeros(0)


def crush(x, bits, hold):
    """Bit depth and sample-and-hold, both ramping across the window."""
    n = len(x)
    depth = ramp(bits[0], bits[1], n)
    steps = 2.0 ** (depth - 1)
    y = np.round(x * steps) / steps
    h = np.maximum(1, np.round(ramp(hold[0], hold[1], n))).astype(int)
    idx = np.arange(n)
    # each sample takes the value at the start of its hold block (block size varies with h)
    held = idx - (idx % h)
    return y[held]


def lowpass(x, rate, cutoff):
    """Two one-pole low-passes in a row, cutoff sliding from cutoff[0] to cutoff[1] Hz."""
    c = ramp(cutoff[0], cutoff[1], len(x))
    alpha = 1 - np.exp(-2 * np.pi * c / rate)
    y = x.copy()
    for _ in range(2):
        out, prev = np.empty_like(y), 0.0
        for i in range(len(y)):
            prev += alpha[i] * (y[i] - prev)
            out[i] = prev
        y = out
    return y


def gate(x, rate, hz, depth):
    t = np.arange(len(x)) / rate
    on = (np.floor(t * hz * 2) % 2 == 0).astype(float)      # half of each cycle open
    d = ramp(depth[0], depth[1], len(x))
    return x * (1 - d * (1 - on))


def drive(x, gain):
    g = ramp(gain[0], gain[1], len(x))
    return np.tanh(x * g) / np.tanh(g)


def apply(stem, rate, t0, t1, fx):
    a, b = int(t0 * rate), min(len(stem), int(t1 * rate))
    if b <= a:
        return
    dry = stem[a:b].copy()
    wet = dry
    if "drive" in fx:
        wet = drive(wet, fx["drive"])
    if "filter" in fx:
        wet = lowpass(wet, rate, fx["filter"]["cutoff"])
    if "crush" in fx:
        wet = crush(wet, fx["crush"]["bits"], fx["crush"]["hold"])
    if "gate" in fx:
        wet = gate(wet, rate, fx["gate"]["rate"], fx["gate"]["depth"])
    edge = min(int(0.012 * rate), (b - a) // 2)  # 12 ms crossfades so the effect doesn't click in
    mix = np.ones(b - a)
    if edge:
        mix[:edge] = np.linspace(0, 1, edge)
        mix[-edge:] = np.linspace(1, 0, edge)
    stem[a:b] = dry * (1 - mix) + wet * mix


def apply_all(stems: dict, rate: int, text: str, effects: list, say=lambda *a: None) -> None:
    plan = build.plan(text) or []
    bars = bar_counts(text)
    for fx in effects:
        for sec in (s for s in plan if s["pattern"] == fx["section"]):
            t0, t1 = sec["start_seconds"], sec["end_seconds"]
            if "bars" in fx:
                per = (t1 - t0) / bars[fx["section"]]
                t0, t1 = t0 + (fx["bars"][0] - 1) * per, t0 + fx["bars"][1] * per
            for lane in fx["lanes"]:
                if lane in stems:
                    apply(stems[lane], rate, t0, t1, fx)
            say(f"  {fx['section']}{' |' + '–|'.join(map(str, fx['bars'])) if 'bars' in fx else ''} "
                f"{t0:.1f}–{t1:.1f}s on {', '.join(fx['lanes'])}: {fx.get('why', '')}")


def process_files(paths: dict, text: str, fx_path: Path, mix_to: Path) -> None:
    """The ear's hook: put a song's effects on its rendered lanes in place, and rewrite the mix as
    their sum, so the page plays what the master sounds like and mute still works."""
    stems, rate = {}, None
    for lane, p in paths.items():
        with wave.open(str(p)) as w:
            rate = w.getframerate()
            stems[lane] = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64) / 32768.0
    apply_all(stems, rate, text, json.loads(fx_path.read_text()))
    n = max(len(x) for x in stems.values())
    total = np.zeros(n)
    for x in stems.values():
        total[:len(x)] += x
    gain = min(1.0, 0.98 / (float(np.max(np.abs(total))) or 1.0))   # one gain for all, so they still add up
    for lane, p in paths.items():
        _write(p, rate, stems[lane] * gain)
    _write(mix_to, rate, total * gain)


def _write(path: Path, rate: int, x) -> None:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes((np.clip(x, -1, 1) * 32767).astype("<i2").tobytes())


def master(song: Path) -> Path:
    song = song.resolve()
    text = song.read_text()
    sid = song.stem
    OUT.mkdir(exist_ok=True)
    work = OUT / "work"
    work.mkdir(exist_ok=True)
    lanes = [l for l in build.LANES if re.search(rf"^\s*lane\s+{l}\b", text, re.M)]
    calls = []
    for lane in lanes:
        p = work / f"{sid}.{lane}.piro"
        p.write_text(build.only_lane(text, lane))
        calls.append(("track", str(p), "-o", str(work / f"{sid}.{lane}.wav")))
    build.piros(*calls)

    stems, rate = {}, None
    for lane in lanes:
        with wave.open(str(work / f"{sid}.{lane}.wav")) as w:
            rate = w.getframerate()
            stems[lane] = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64) / 32768.0

    fx_path = song.with_suffix(".fx.json")
    apply_all(stems, rate, text, json.loads(fx_path.read_text()) if fx_path.exists() else [], say=print)

    n = max(len(s) for s in stems.values())
    mix = np.zeros(n)
    for s in stems.values():
        mix[:len(s)] += s
    peak = float(np.max(np.abs(mix))) or 1.0
    mix *= 10 ** (-1 / 20) / peak                       # peak at -1 dBFS
    wav = OUT / f"{sid}.wav"
    with wave.open(str(wav), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes((np.clip(mix, -1, 1) * 32767).astype("<i2").tobytes())
    print(f"wrote {wav.relative_to(ROOT)} ({n / rate:.1f}s)")
    if shutil.which("ffmpeg"):
        mp3 = wav.with_suffix(".mp3")
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(wav), "-codec:a", "libmp3lame", "-q:a", "2", str(mp3)], check=True)
        print(f"wrote {mp3.relative_to(ROOT)}")
    return wav


if __name__ == "__main__":
    for arg in sys.argv[1:] or [str(HERE / "songs" / "but-we-endure-rewrite.piro")]:
        master(Path(arg))
