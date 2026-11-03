---
name: piropipo
description: Compose chiptune music and game sound effects as wav files with the piro CLI and its .piro track format. Use when asked to write, edit, or verify 8-bit / NES-style music or sfx.
---

# composing with piro

you are deaf. you write music and render wavs you will never hear. the checker verifies
every pitch, effect, and timing against the chip's real limits, refusing with a located,
teaching error instead of quietly fixing anything. **if `piro check` is silent, the chip
really plays what you wrote.** believe the report, not your imagination of the sound.

## the loop

1. **read [references/format.md](references/format.md) before your first track.** it's the
   whole notation: grid, lanes, the play block, the six effect words, the constraints
   between effects, what the chip refuses. composing past jingle length, revising under a human's
   timestamped notes, or writing odd meter? also read
   [references/advanced-composing.md](references/advanced-composing.md) before you write a
   bar: the measuring loop and the craft that full-length pieces need.
2. **write the `.piro` file.** compose something you'd stand behind; the chip idioms below
   are the difference between notes and music. positions are slot arithmetic, and you don't
   have to do it: write a **sketch** (the same file, note lines without positions, in
   order) and `piro sketch song.sketch` places every note and writes the positioned .piro,
   refusing any lane that overflows its pattern. format.md's last section has the shape.
3. **`piro check tune.piro`** (json by default, `--human` for people). exit 1 means errors;
   every error names its line, position, and the fix. re-check until exit 0. the error is
   the chip's real limit, not a suggestion. `--strict` also refuses any slot no note or
   written rest (`- xN`) covers, each gap reported with its span.
4. **read the stats like ears.** format.md's check section catalogs every field. the
   levers: the per-bar loudness curve for section contrast, sounding-% to catch lanes that
   render silent (a part you wrote and nobody will hear), the harmony advisory for
   sustained clashes, and effect lines for achieved values, not your ask. `--summary` for
   the headline; `--events` for per-note data downstream.
5. **`piro track tune.piro`** writes the wav next to the file. **`piro roll tune.piro`**
   draws a piano-roll png - if you can see images, look at it: contour, holes, overlaps,
   and whether the bass actually moves are all visible at a glance. a long piece is
   unreadable whole: `--patterns chorus,bridge` or `--bars 17..24` crops one section to
   full attention.
6. iterate. a human with ears outranks the report on taste; the report outranks everyone
   on facts.

## the chip's idioms

the notation is in format.md; this is the craft that makes it sound like a chip and not a
doorbell. a starting point, not a cage.

- **the four lanes are a band.** pulse1 carries the melody, pulse2 harmonizes or echoes,
  the triangle is the bass (it sounds about an octave lower than written and its volume is
  all-or-nothing), and noise is the drum kit.
- **noise drums: the letter picks the drum, the octave is ignored.** a high letter (B) is
  a bright hat, a mid one (E) a snare, low ones thud. write a kit, not pitches: `1: B4 x1
  vol 4` tick, `5: E4 x2 vol 11 fade` snare with a tail.
- **arps are the poor chip's chords.** a held note with `arp 0,4,7` shimmers a major
  triad, `0,3,7` minor, at ~60 steps a second. put them on a held pulse2 or triangle note
  under the melody.
- **the echo.** repeat a phrase two slots late on the other pulse, quieter and thinner:
  `vol 4` and `duty 12.5`. the single most recognizable NES texture.
- **duty is tone color.** 50 is round and hollow, 25 is the classic bright lead, 12.5 is
  thin and sharp. a per-note `duty` flips color mid-phrase; the lane param sets the default.
- **fade endings.** a long held note with `vol 12 fade` is the classic outro, and `arp
  0,4,7 vol 12 fade` on that note fades the whole chord - the textbook NES ending (the
  fade's last step is silent on purpose). the triangle can't fade, so end the bass on a
  barline. a clean stop reads as intent; a mid-bar cut reads as mistake.
- **the snare roll.** consecutive `x1` noise notes with climbing `vol` values. volume is
  the articulation; the pitch barely matters.
- **a slide (`from`) is a scoop, not a glissando.** the written pitch is the note you'll
  hear; `from` names the departure and the walk is quick (~60 steps/s), chromatic, honest
  about being stepped.

## sound effects

`piro sfx coin` makes a coin. seven presets work with zero flags, and the dials that
exist speak Hz, ms, and semitones. the same command always writes the same bytes;
`--seed N` rolls a variation. the full flag table and preset-reshaping rules:
[references/sfx-mapping.md](references/sfx-mapping.md).
