#!/usr/bin/env python3
"""Serve the ear at http://localhost:4850 and rebuild a song whenever its .piro changes.

    python3 contrib/ear/serve.py              # every song in contrib/ear/songs/ and docs/, first minute each
    python3 contrib/ear/serve.py --until 0    # whole songs

Edit any of those .piro files (by hand or with an agent), save, and the page picks it up in a few
seconds: it polls data/index.json and reloads the song it's showing, keeping your place.
"""

from __future__ import annotations

import argparse
import functools
import http.server
import json
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build  # noqa: E402

SONG_DIRS = [build.HERE / "songs", build.REPO / "docs"]
WEB = build.HERE / "web"


def all_songs():
    return sorted(p for d in SONG_DIRS if d.exists() for p in d.glob("*.piro"))


class Quiet(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


def needs_build(song: Path, until: float) -> bool:
    try:
        idx = json.loads((build.DATA / "index.json").read_text())
        entry = idx["songs"][song.stem]
        built = build.DATA / song.stem / "analysis.json"
        return entry.get("source_mtime") != song.stat().st_mtime or entry.get("until") != until or (not built.exists() and not entry.get("error"))
    except (OSError, KeyError, json.JSONDecodeError):
        return True


def watch(until: float) -> None:
    while True:
        for song in all_songs():
            if needs_build(song, until):
                t = time.time()
                ok = build.build_or_report(song, until, quiet=True)
                build.index_song(song.stem, {"until": until})
                print(f"{time.strftime('%H:%M:%S')} {'built' if ok else 'FAILED'} {song.name} in {time.time() - t:.1f}s", flush=True)
        live = {p.stem for p in all_songs()}
        try:
            idx = json.loads((build.DATA / "index.json").read_text())
            gone = [sid for sid in idx["songs"] if sid not in live]
            if gone:
                for sid in gone:
                    idx["songs"].pop(sid)
                idx["version"] = time.time()
                (build.DATA / "index.json").write_text(json.dumps(idx, indent=1))
        except (OSError, json.JSONDecodeError, KeyError):
            pass
        time.sleep(1)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=4850)
    ap.add_argument("--until", type=float, default=60.0)
    args = ap.parse_args()
    threading.Thread(target=watch, args=(args.until,), daemon=True).start()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), functools.partial(Quiet, directory=str(WEB)))
    print(f"the ear is at http://localhost:{args.port}  (watching {', '.join(str(d) for d in SONG_DIRS)})", flush=True)
    server.serve_forever()
