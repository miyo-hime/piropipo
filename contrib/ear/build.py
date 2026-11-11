#!/usr/bin/env python3
"""Build the ear's data for one song: cut it to the first minute, render it, analyze it.

    python3 contrib/ear/build.py docs/but-we-endure.piro            # the first minute (whole sections)
    python3 contrib/ear/build.py docs/but-we-endure.piro --until 0  # the whole song

Writes contrib/ear/web/data/<song>/: mix.wav, one wav per lane (for mute and solo), analysis.json,
and refreshes web/data/index.json. The cut never changes line numbers: dropped play-block
entries and filtered lanes become comment lines, so every `line` in the analysis points straight
back into the original .piro.
"""

from __future__ import annotations

import argparse
import os
import array
import json
import math
import re
import subprocess
import sys
import time
import wave
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import theory  # noqa: E402

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
ROOT = HERE
PIRO = Path(os.environ["PIRO"]) if os.environ.get("PIRO") else REPO / "target" / "release" / "piro"
DATA = HERE / "web" / "data"
LANES = ("pulse1", "pulse2", "triangle", "noise")


class BuildError(Exception):
    pass


def piros(*calls) -> list:
    """Run several piro commands at once (each one renders, ~3s apiece) and return their json."""
    if not PIRO.exists():
        raise BuildError(f"no piro binary at {PIRO}; run cargo build --release at the repo root, or set PIRO")
    procs = [(args, subprocess.Popen([str(PIRO), *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)) for args in calls]
    outs = []
    for args, p in procs:
        stdout, stderr = p.communicate()
        try:
            out = json.loads(stdout)
        except json.JSONDecodeError:
            raise BuildError(f"piro {' '.join(args)} said:\n{stdout}{stderr}")
        if p.returncode != 0 or out.get("ok") is False:
            errs = out.get("errors") or []
            lines = [f"line {e.get('line')}: {e.get('message')}" for e in errs[:8]] or [stderr.strip() or "unknown error"]
            raise BuildError("piro refused the score:\n" + "\n".join(lines))
        outs.append(out)
    return outs


def piro(*args: str) -> dict:
    return piros(args)[0]


def plan(text: str):
    """Section order and lengths straight from the score text, so the cut needs no render.
    None if anything looks unusual; the caller then asks piro."""
    tempo = re.search(r'^song\s+"[^"]*"\s+tempo\s+(\d+)', text, re.M)
    if not tempo:
        return None
    patterns, order = {}, []
    for line in text.split("\n"):
        bare = line.split(";")[0].strip()
        m = re.match(r"pattern\s+(\S+)(.*)", bare)
        if m:
            params = dict(re.findall(r"(\w+)\s+(\d+)", m.group(2)))
            if "bars" not in params or "grid" not in params:
                return None
            patterns[m.group(1)] = int(params["bars"]) * int(params.get("beats", 4)) * 60.0 / int(tempo.group(1))
            order.append(m.group(1))
    lines = text.split("\n")
    at, entries = play_block(lines)
    if at is not None:
        cues = []
        for j in entries:
            m = re.match(r"\s*(\S+)\s*([+-]\d+)?", lines[j].split(";")[0])
            cues.append((m.group(1), int(m.group(2) or 0)))
    else:
        cues = [(name, 0) for name in order]
    out, t = [], 0.0
    for name, shift in cues:
        if name not in patterns:
            return None
        out.append({"pattern": name, "transpose": shift, "start_seconds": t, "end_seconds": t + patterns[name]})
        t += patterns[name]
    return out


def play_block(lines: list):
    """(index of the `play` line, [indices of its entries]) or (None, [])."""
    for i, line in enumerate(lines):
        if line.split(";")[0].strip() == "play":
            entries = []
            for j in range(i + 1, len(lines)):
                bare = lines[j].split(";")[0]
                if not bare.strip():
                    continue
                if not lines[j][:1].isspace():
                    break
                entries.append(j)
            return i, entries
    return None, []


def cut(text: str, sections: list, keep: int) -> str:
    """The same score, playing only its first `keep` sections, with every line where it was."""
    lines = text.split("\n")
    at, entries = play_block(lines)
    if at is None:
        names = [f"  {s['pattern']}" + (f" {s['transpose']:+d}" if s["transpose"] else "") for s in sections[:keep]]
        return text.rstrip("\n") + "\n\nplay\n" + "\n".join(names) + "\n"
    for j in entries[keep:]:
        lines[j] = "; (cut by the ear) " + lines[j].strip()
    return "\n".join(lines)


def only_lane(text: str, lane: str) -> str:
    """The same score with every other lane commented out, line for line."""
    out, current = [], None
    for line in text.split("\n"):
        bare = line.split(";")[0].strip()
        if re.match(r"pattern\s", bare):
            current = None
        m = re.match(r"lane\s+(\S+)", bare)
        if m:
            current = m.group(1)
        if current is not None and current != lane and bare and (m or bare[0].isdigit() or bare.startswith("-")):
            out.append(";" + line)
        else:
            out.append(line)
    return "\n".join(out)


def read_wav(path: Path):
    with wave.open(str(path)) as w:
        frames = w.readframes(w.getnframes())
        assert w.getsampwidth() == 2, "piro writes 16-bit"
        samples = array.array("h", frames)
        if w.getnchannels() == 2:
            samples = array.array("h", samples[0::2])
        return samples, w.getframerate()


def loudness(samples, rate: int, t0: float, t1: float):
    a, b = int(t0 * rate), int(t1 * rate)
    chunk = samples[a:b]
    if not chunk:
        return None
    rms = math.sqrt(sum(x * x for x in chunk) / len(chunk)) / 32768
    return round(20 * math.log10(rms), 1) if rms > 0 else -120.0


def build(song: Path, until: float = 60.0, quiet: bool = False) -> dict:
    song = song.resolve()
    sid = song.stem
    out = DATA / sid
    out.mkdir(parents=True, exist_ok=True)
    work = out / "work"
    work.mkdir(exist_ok=True)
    text = song.read_text()
    say = (lambda *a: None) if quiet else (lambda *a: print(f"[{sid}]", *a, flush=True))
    asked = re.search(r"^;\s*ear:\s*until\s+([\d.]+)", text, re.M)  # a score can ask for more than a minute
    if asked:
        until = float(asked.group(1))

    sections = plan(text) or piro("check", str(song))["stats"]["sections"]
    keep = len(sections)
    if until and until > 0:
        keep = next((i + 1 for i, s in enumerate(sections) if s["end_seconds"] >= until - 1e-6), len(sections))
    cut_text = cut(text, sections, keep) if keep < len(sections) else text
    cut_path = work / "cut.piro"
    cut_path.write_text(cut_text)
    say(f"{keep} of {len(sections)} sections, {sections[keep - 1]['end_seconds']:.1f}s")

    lanes_written = [l for l in LANES if re.search(rf"^\s*lane\s+{l}\b", cut_text, re.M)]
    calls = [("check", str(cut_path), "--events"), ("track", str(cut_path), "-o", str(out / "mix.wav"))]
    for lane in lanes_written:
        p = work / f"{lane}.piro"
        p.write_text(only_lane(cut_text, lane))
        calls.append(("track", str(p), "-o", str(out / f"{lane}.wav")))
    check = piros(*calls)[0]
    present = sorted({e["lane"] for e in check["events"]}, key=LANES.index)
    for lane in LANES:
        stem = out / f"{lane}.wav"
        if lane not in present and stem.exists():
            stem.unlink()
    say("rendered mix and", len(present), "stems")
    fx = song.with_suffix(".fx.json")
    if fx.exists():  # effects piro can't do (master.py): the page should hear them too
        import master as mastering
        mastering.process_files({lane: out / f"{lane}.wav" for lane in present}, cut_text, fx, out / "mix.wav")
        say("applied", fx.name)

    analysis = theory.analyze(cut_text, check, {"melody": "pulse1"})
    mix, rate = read_wav(out / "mix.wav")
    for sec in analysis["sections"]:
        for b in sec["per_bar"]:
            b["loudness_db"] = loudness(mix, rate, b["t0"], b["t1"])
    for tb in analysis["timeline"]:
        tb["loudness_db"] = loudness(mix, rate, tb["t0"], tb["t1"])

    # do the stems add back up to the mix? if so, muting a lane is honest (every 7th sample is plenty)
    stems = [read_wav(out / f"{lane}.wav")[0] for lane in present]
    worst = 0
    for i in range(0, len(mix), 7):
        total = sum(s[i] for s in stems if i < len(s))
        worst = max(worst, abs(total - mix[i]))
    analysis["stems_add_up"] = worst <= 2 * len(present)
    analysis["stems_worst_diff"] = worst

    analysis.update({
        "id": sid, "file": str(song.relative_to(REPO)) if REPO in song.parents else str(song),
        "built_at": time.time(), "source_mtime": song.stat().st_mtime,
        "cut_seconds": sections[keep - 1]["end_seconds"], "full_seconds": sections[-1]["end_seconds"],
        "sections_total": len(sections), "sample_rate": rate,
        "audio": {"mix": "mix.wav", "stems": {lane: f"{lane}.wav" for lane in present}},
        "source_lines": text.split("\n"),
    })
    (out / "analysis.json").write_text(json.dumps(analysis, separators=(",", ":")))
    say(f"key {analysis['key']['name']}, {len(analysis['findings'])} findings, stems add up: {analysis['stems_add_up']} (worst {worst})")
    index_song(sid, {"title": analysis["title"], "file": analysis["file"], "built_at": analysis["built_at"],
                     "source_mtime": analysis["source_mtime"], "error": None,
                     "sections": [s["name"] for s in analysis["sections"]], "seconds": analysis["cut_seconds"]})
    return analysis


def index_song(sid: str, entry: dict) -> None:
    DATA.mkdir(parents=True, exist_ok=True)
    path = DATA / "index.json"
    try:
        idx = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        idx = {"songs": {}}
    idx["songs"][sid] = {**idx["songs"].get(sid, {}), **entry}
    idx["version"] = time.time()
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(idx, indent=1))
    tmp.replace(path)


def build_or_report(song: Path, until: float, quiet: bool = False) -> bool:
    try:
        build(song, until, quiet)
        return True
    except BuildError as e:
        print(f"[{song.stem}] {e}", file=sys.stderr, flush=True)
        index_song(song.stem, {"error": str(e), "file": str(song), "source_mtime": song.stat().st_mtime})
        return False


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("songs", nargs="+", type=Path)
    ap.add_argument("--until", type=float, default=60.0, help="stop after the section that reaches this many seconds (0 = whole song)")
    args = ap.parse_args()
    ok = all([build_or_report(s, args.until) for s in args.songs])
    sys.exit(0 if ok else 1)
