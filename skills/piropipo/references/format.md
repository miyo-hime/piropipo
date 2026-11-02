# the .piro format

one hard rule explains everything else: **a note's position is always written down, never
derived.** every note says where it starts and how long it rings, so a wrong length stays
a local mistake and the checker can verify each line on its own.

## a complete file

```
song "first light" tempo 112 profile nes

pattern main bars 2 grid 16

lane pulse1 duty 25
  1.1:  E5 x2
  1.3:  G5 x2
  1.5:  B5 x4
  1.9:  A5 x2
  1.11: G5 x2
  1.13: - x4          ; rest, written out so the bar is fully accounted for
  2.1:  E5 x8
  2.9:  D5 x8

lane pulse2 duty 50
  1.1: E4 x8
  1.9: C4 x8
  2.1: B3 x16

lane triangle
  1.1: E2 x8
  1.9: A2 x16         ; rings across the barline, ends mid bar 2
  2.9: B2 x8
```

`tempo` is quarter-note BPM, and a bar is four beats: with `grid 16` each slot is a
sixteenth note at that tempo, with `grid 8` an eighth.

line-based, one thing per line: a directive (`song`, `pattern`, `lane`, `play`) or a note.
comments start with `;` and run to end of line. that's the whole syntax; there is nothing
to quote or escape inside a lane.

## positions

a position is `bar.slot`, both 1-indexed. **these are two integers with a dot between
them, not a decimal number**: `1.10` is bar 1 slot 10, and it comes after `1.5`. slot 0,
or any slot above the pattern's grid, is an error naming the lane and bar.

a bare slot (`5:` instead of `1.5:`) means bar 1. fine for one-bar jingles; multi-bar
music reads better with the full form everywhere.

`grid` is how many slots a bar divides into. `grid 16` in 4/4 means a slot is a sixteenth
note. the grid is time resolution only; it has no effect on tempo.

## lengths

`xN` is how many slots the note rings. rules, all checked:

- a length **may cross a barline**. a half note tied to a quarter is just `x12`; there is
  no tie syntax because a length already is one.
- a note may **not** run past the end of its pattern, and that includes ringing on into
  whichever pattern plays next. each pattern is a closed box.
- two notes in the same lane may **not** overlap. a lane is one voice.

## rests

you never have to write a rest; a gap in the slots is silence. but you *may* write one,
with `-` in the pitch position (`1.13: - x4`). the reason to bother: the note after a gap
is where positions get miscomputed, because the gap has no line of its own. writing the
rest out gives the arithmetic a visible anchor.

## pitch

scientific pitch notation, C4 is middle C. sharps with `#` (`C#4`), flats with `b`
(`Db4`), same note. range limits come from the active profile: the checker knows an NES
pulse plays C2 through B7 and a triangle C1 through B6, and says so per note.

## lanes

the profile defines which lanes exist and what they can do. two profiles exist: `nes`
(the default, true hardware ranges) and `nes-free` (the same four voices stretched to
the engine's wider edges - pulses A1 through G#8, triangle A0 through G#7). both have
`pulse1`, `pulse2`, `triangle`, `noise`. parameters follow the lane name (`lane pulse1
duty 25 vol 9`). a lane appears at most once per pattern, and lanes you don't use can be
omitted.

on `nes`, the pulses take `duty` (12.5, 25, 50, or 75) and `vol` (0 to 15, default 10),
the noise takes `vol`, and the triangle takes nothing: it has no volume, it's either on
or off.

## effects

effects are extra words after the length. they belong to that one note, their order on
the line means nothing, and every one starts with a keyword:

| effect | spelling | example |
|---|---|---|
| vibrato | `vib <N>ct <N>hz [<N>ms]` | `G5 x12 vib 20ct 6hz 250ms` |
| arpeggio | `arp <off>,<off>,...` | `A3 x16 arp 0,3,7` |
| slide | `from <PITCH>` | `C5 x8 from A4` |
| duty | `duty <width>` | `B4 x8 duty 12.5` |
| volume | `vol <0-15>` | `C5 x4 vol 5` |
| fade | `fade` | `A4 x16 vol 12 fade` |

```
lane pulse1 duty 25 vol 9
  1.1:  E5 x4 vib 20ct 6hz 125ms
  1.5:  G5 x4 duty 12.5
  1.9:  C5 x4 from A4
  1.13: B4 x4 vol 13 fade
  2.1:  A4 x8 arp 0,4,7 vol 7
```

- **vib** shakes the pitch. depth is in cents (peak, not peak-to-peak), rate in hertz,
  delay in milliseconds and optional. units are case-blind (`6Hz` parses fine). the chip's
  rates are 20 Hz divided by a whole number (20, 10, 6.667, 5, 4, ...), so you get the
  nearest one, and an ask more than 25% from every rate is an error naming the two
  around it. the delay rounds to the nearest 1/60 s frame and tops out at 4250 ms. a
  delay that meets or outlasts the note is an error, since that vibrato would never start.
- **the cents are an ask, not a promise.** the chip bends its period register, so depth
  compiles to whole register ticks, and one tick is a different number of cents on every
  pitch (it also climbs further than it falls). a depth that rounds to zero ticks, needs
  more than 255, or would drive the register through zero is an error. the checker
  range-checks the swing the rounded depth *really* makes, both edges, and `piro check`
  reports both.
- **arp** cycles semitone offsets from the written note, about 60 steps a second. at
  least two offsets, signed, repeats allowed; the 0 is a habit, not a rule (`arp 4,7`
  cycles only +4 and +7, never sounding the written pitch itself). the steps always add up to exactly the written
  length, and every offset has to get at least one step. the achieved rate is reported,
  and outside 45 to 75 steps a second it's an error saying what this tempo can do. some
  tempos can't arp at all, and the error says that too.
- **from** is a slide, and the pitch column holds where it lands: `C5 x8 from A4` scoops
  up from A4 into C5. the walk is chromatic, a semitone per step at the arp's pace, and
  the target holds everything after. a note too short for every step plus a hold of the
  target is an error naming a length that fits. `from` its own pitch goes nowhere and is
  refused.
- **duty** swaps the pulse width for this note only (same widths as the lane param).
  pulses only.
- **vol** sets this note's volume, 0 to 15, over the lane's default. pulses and noise.
- **fade** ramps from the note's starting volume (its `vol`, or the lane's) down to 0,
  and the last frame that sounds is the 0. the note has to span at least 2 frames.
  pulses and noise.

the rules between them:

- one pitch effect per note: `vib`, `arp`, or `from`. two is an error naming both.
- `vol` and `duty` stack with anything. `fade` stacks with `vib` but not with `arp` or
  `from` (their steps would restart the fade every time).
- effects on a rest are an error. a rest has nothing to bend, shake, or fade.
- the noise lane refuses `vib`, `arp`, and `from` (its pitch is a timer setting), and
  the triangle refuses `vol` and `fade`.
- nothing leaks: the note after an effect plays with the lane's own duty, volume, and
  steady pitch.
- every pitch an effect reaches is checked against the profile like a written note: both
  vibrato edges, every arp offset, the slide's departure. with a `play` transpose, that
  check (and the cents-to-ticks rounding) runs again for each transposed play.
- a song holds 128 different vibratos and 128 different fades. reusing the same settings
  is free; the 129th distinct one is an error.

## the song order

with one pattern, the file is the song. with several, a `play` block says what plays when:

```
play
  intro
  main
  main
  bridge +3
  main
```

pattern names, top to bottom, and that's the song. the block can sit anywhere in the file
(patterns declared further down are fine) and runs until the next `pattern` line. leave it
out and the patterns play once each, in the order they're written.

- **a repeat is the name written again.** there is no count sugar like `main x2`.
- **`+N` / `-N` after a name transposes that play by N semitones.** only the pitched
  lanes move (pulses and triangle); the noise lane stays exactly as written.
- transposed notes are checked against the profile all over again. if `bridge +3` pushes
  a note past the top of the pulse range, that's an error naming the note, the pattern,
  and the play line. nothing gets clamped.
- a song tops out at 10 minutes of rendered time, counting every play, repeats included.
- a pattern the play block never names doesn't render and costs nothing against the cap -
  but its notes still answer to the profile. an unplayable note is an error wherever it's
  written.
- pattern names are single words. the directive words themselves (`play`, `song`, `lane`,
  `pattern`) can't be listed in a play block, so don't name a pattern after them.

there are no sections or loop markers; the flat play list is the whole structure.

## what `piro check` promises

every error names the lane, the bar, and the slot, in the same `bar.slot` syntax you
write. the checker also prints each note's computed end position, so a seam like "does
the bass stop exactly where the next note starts" is checkable by eye without doing the
carry yourself.

effects get the same treatment: for every vibrato it reports the rate and delay you
actually got and both real swings in cents (once per transposed play), and for every arp
its step count and steps a second.

"end" always means **the last slot the note rings in**, inclusive. a note at `1.9` with
`x16` rings through `2.8`, and the next note may start at `2.9`. a perfect seam is
end + 1 = next start, never end = next start.

notes within a lane may be written in any order (the checker sorts; position is truth,
line order is convenience), and pattern names are unique per file.
