---
name: piropipo
description: Compose chiptune music and game sound effects as wav files with the piro CLI and its .piro track format. Use when asked to write, edit, or verify 8-bit / NES-style music or sfx.
---

# composing with piro

you are deaf. piro is built around that: you'll write music, render it to a wav, and
never hear a sample of it, and piro's deal is that you don't have to. the checker verifies
everything that can be verified about a track - every pitch against the chip's real range,
every effect's true excursion, every note's position and length - and refuses with a
located, teaching error instead of quietly "fixing" anything. **if `piro check` is silent,
the chip really plays what you wrote.** your job is to write with intent and believe the
report, not your imagination of the sound.

## the loop

1. **read [references/format.md](references/format.md) before your first track.** it's the
   whole notation - grid, lanes, the play block, the six effect words - and it opens with
   a complete worked example.
2. **write the `.piro` file.** compose something you'd stand behind; the chip idioms below
   are the difference between notes and music.
3. **`piro check tune.piro`** (json by default, `--human` for people). exit 1 means errors,
   and every error names its line, position, and the fix - a bad vibrato depth tells you
   the shallowest depth that works, a cramped slide tells you the note length that would
   fit it. fix and re-check until exit 0. don't fight a refusal: the error is the chip's
   real limit, not a suggestion.
4. **read the stats like ears.** duration, peak dbfs, and per-lane solo activity - a lane
   that "renders silent" is a part you wrote and nobody will hear. effect lines report what
   was *achieved*, not what you asked: the vibrato's real rate and its true cent swings
   after rounding (they differ from your ask, honestly), the arp's actual steps per second.
   if a transposed play changes an effect, the report shows each variant.
5. **`piro track tune.piro`** writes the wav next to the file. **`piro roll tune.piro`**
   draws a piano-roll png - if you can see images, look at it: contour, holes, overlaps,
   and whether the bass actually moves are all visible at a glance.
6. iterate. a human with ears outranks the report on taste; the report outranks everyone
   on facts.

## the chip's idioms

the notation is in format.md; this is the craft that makes it sound like a chip and not a
doorbell. learned from real compositions, so trust it as a starting point, not a cage.

- **the four lanes are a band.** pulse1 carries the melody, pulse2 harmonizes or echoes,
  the triangle is the bass (it sounds about an octave lower than written and its volume is
  all-or-nothing), and noise is the drum kit.
- **noise drums: the letter picks the drum, the octave is ignored.** a high letter (B) is
  a bright hat, a mid one (E) a snare, low ones thud. write a kit, not pitches: `1: B4 x1
  vol 4` tick, `5: E4 x2 vol 11 fade` snare with a tail.
- **arps are the poor chip's chords.** a held note with `arp 0,4,7` shimmers a major
  triad, `0,3,7` minor, at ~60 steps a second. put them on a held pulse2 or triangle note
  under the melody. slow tempos can't arp - the error will say so plainly.
- **the echo.** repeat a phrase two slots late on the other pulse, quieter and thinner:
  `vol 4` and `duty 12.5`. the single most recognizable NES texture.
- **duty is tone color.** 50 is round and hollow, 25 is the classic bright lead, 12.5 is
  thin and sharp. a per-note `duty` flips color mid-phrase; the lane param sets the default.
- **fade endings.** a long held note with `vol 12 fade` is the classic outro. the triangle
  can't fade (no volume knob), so end the bass deliberately on a barline instead - a clean
  stop reads as intent, a mid-bar cut reads as a mistake.
- **the snare roll.** consecutive `x1` noise notes with climbing `vol` values. volume is
  the articulation; the pitch barely matters.
- **a slide (`from`) is a scoop, not a glissando.** the written pitch is the note you'll
  hear; `from` names the departure and the walk is quick (~60 steps/s), chromatic, honest
  about being stepped.

## what the chip refuses

format.md lists every constraint between effects. when you hit one, the error names the
exact note and the reason. treat a refusal like a guitar refusing a six-fret stretch:
rewrite the musical idea, don't hunt for a workaround.

## sound effects

`piro sfx coin` makes a coin. seven presets work with zero flags, and the dials that
exist speak Hz, ms, and semitones. the same command always writes the same bytes;
`--seed N` rolls a variation. the full flag table and preset-reshaping rules:
[references/sfx-mapping.md](references/sfx-mapping.md).
