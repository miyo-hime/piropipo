# the ear

Hey. We messed around with piropipo and got this. What do you think?

It started with a comment on the Reddit thread about "but we endure": line 2 of the main melody
almost repeats line 1's rhythm but not quite, and it ends on an E that doesn't sit right. Working
out *why* turned into three things, all in this folder:

1. **An ear for a deaf composer.** piro's check verifies what the chip can play. It doesn't say
   whether it sounds right. This is a small analyzer plus a web page. It reads a `.piro`, renders
   it with piro, and draws the tune as rows stacked on their beats, so line 1 sits right above
   line 2. With the rows come the chords, a tension curve, and a list of things that probably
   sound wrong, each with a fix to try. You can loop any bar, mute or solo a lane, and swap
   between two versions of a song without stopping the loop.
2. **What we learned**, one finding at a time, in [LESSONS.md](LESSONS.md).
3. **A rewrite of but we endure** built on those lessons. [REWRITE.md](REWRITE.md) walks through
   every change and the idea behind it, and [media/](media/) has it as a video.

[RESEARCH.md](RESEARCH.md) is the reading list the measures come from: tonal tension,
expectation and surprise, syncopation, phrase repetition, and corpora of pop melodies. Every
entry is a link.

None of this needs merging to be useful; take whatever helps. It's quick code, written in an
afternoon with Claude, and every number in it is a first guess.

## Running it

```
cargo build --release
python3 contrib/ear/serve.py          # then open http://localhost:4850
```

The server builds every `.piro` in `docs/` and `contrib/ear/songs/` and rebuilds whichever one
you save. It shows the first minute of each song unless the score asks for more with a
`; ear: until 0` line. Python 3.9 or newer, standard library only. The one exception is the
rewrite's effects, which need numpy (and ffmpeg for the mp3).

```
python3 contrib/ear/rewrite.py        # regenerate songs/but-we-endure-rewrite.piro from the rules
python3 contrib/ear/master.py         # render it with its effects to contrib/ear/out/ (.wav and .mp3)
```

To check the page after changing it, open `http://localhost:4850/#selftest=1`. It clicks through
everything and leaves a report at the top.

## What the ear measures

Each measure is a cut-down version of a published idea. The papers are in RESEARCH.md, and
`theory.py` names each source next to its code.

| measure | how |
|---|---|
| key | Krumhansl–Schmuckler key finding |
| chords | template matching per beat group. The melody gets a small vote (it's what's being judged against the chord), and a note a figure keeps returning to counts as a pedal |
| tension of a note | 0.4 × how far its chord is from home (Lerdahl's chord distance) + 0.4 × how far the note is from its chord (root, fifth, chord tone, scale tone, outside the key) + 0.2 × how hard it rubs against everything else sounding |
| syncopation | Longuet-Higgins & Lee, on beat groups inferred from where the kick and snare land (it finds 2+2+3 in but we endure's 7/8) |
| surprise | a tiny Markov model that learns the melody as it goes, after IDyOM's short-term model |
| line vs line | note starts for rhythm, and pitch distance with any transposition taken out |

The page's list of findings comes from these detectors:
- **near miss:** a line that's almost the same rhythm as the line before, with new notes on top.
- **unresolved ending:** a line ending on a note that rubs against the chord and doesn't step to a
  chord tone.
- **short clashes:** a half step or tritone between the melody and anything else, even for one
  slot.
- **late copies:** another lane playing the melody a few slots behind. Under 150 ms it's a flam;
  past that, an echo.
- **flat or homeless tension:** a tension curve that never leaves home, or never comes back.
- **everything at once:** bars where most of the busy-ness measures are high together.
- **exact repeats.**

## Ideas piro could pick up (ideas, not asks)

- **Short clashes.** The harmony advisory only counts intervals held 3/4 of a beat or longer.
  Most of the clashes we heard were shorter: a 16th of accompaniment or an arp against the melody.
- **Late copies.** An echo one 16th behind reads as a flam. The skill already says to trail by
  two slots and keep echoes on eighth boundaries; nothing checks it.
- **Rhythm between lines.** `check --events` has everything needed to flag a near-miss rhythm
  between phrases. Composers keep doing it, and listeners hear it.
- **Real barlines.** but we endure keeps its real barlines in `; |n` and `; 16x 7/8` comments
  because it predates `beats`. The ear reads those comments; `check` and `roll` could too.

## Files

| file | what |
|---|---|
| `theory.py` | the measures and the detectors |
| `build.py` | cuts a song, renders it and each lane with piro, applies any `.fx.json`, and analyzes it into `web/data/` |
| `serve.py` | the page at :4850, rebuilding on save |
| `web/index.html` | the page itself (Tailwind from a CDN, vanilla JS, Web Audio) |
| `web/selftest.js` | the click-through test |
| `rewrite.py` | generates the rewrite's score from the rules in LESSONS.md |
| `master.py` | renders a song with its `.fx.json` effects (filter, bitcrush, tremolo, overdrive) to wav and mp3 |
| `songs/` | the rewrite, plus the single-fix versions LESSONS.md compares: `fix-D`, `fix-Dsharp`, `tongued` |
| `media/` | the rewrite as a video |
