# the grid format, v0

> status: draft. the parser doesn't exist yet; this doc is the contract we're building it against. the open questions at the bottom are genuinely open, not rhetorical.

## why a grid and not MML

agents are the users here, and agents are reliably bad at one specific thing: knowing *when* a note plays in a format where position has to be computed by summing every duration that came before it. MML and ABC both have that shape. one wrong length early in the bar and every note after it shifts, and nothing on the page looks wrong.

so the format's one hard rule: **a note's position is always written down, never derived.** every note says where it starts and how long it rings. a wrong length stays a local mistake, and the checker can verify each line on its own.

(we also tried the full tracker shape, one cell per tick with hold markers. it can't drift, but writing a long note as a pile of dashes is tedious enough that agents route around it, and the format they reach for when routing around it is this one.)

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

line-based, one thing per line: a directive (`song`, `pattern`, `lane`) or a note. comments start with `;` and run to end of line. that's the whole syntax; there is nothing to quote or escape inside a lane.

## positions

a position is `bar.slot`, both 1-indexed. **these are two integers with a dot between them, not a decimal number.** `1.5` is bar 1 slot 5, `1.10` is bar 1 slot 10, and `1.10` comes after `1.5`. a parser that runs positions through a float parser is wrong, and so is a writer sorting them as decimals. the checker rejects slot 0 and any slot above the pattern's grid size, naming the lane and bar in the error.

a bare slot (`5:` instead of `1.5:`) means bar 1. fine for one-bar jingles; multi-bar music reads better with the full form everywhere.

`grid` is how many slots a bar divides into. `grid 16` in 4/4 means a slot is a sixteenth note. the grid is time resolution only; it has no effect on tempo.

## lengths

`xN` is how many slots the note rings. rules, all checked:

- a length **may cross a barline**. a half note tied to a quarter is just `x12`; there is no tie syntax because a length already is one.
- a note may **not** run past the end of its pattern. (whether a note can cross into the next pattern of the song is open, see below.)
- two notes in the same lane may **not** overlap. a lane is one voice.

## rests

you never have to write a rest; a gap in the slots is silence. but you *may* write one, with `-` in the pitch position (`1.13: - x4`). the honest reason to bother: the note after a gap is where positions get miscomputed, because the gap has no line of its own. writing the rest out gives the arithmetic a visible anchor, and gives the checker a chance to tell you when a bar doesn't add up.

## pitch

scientific pitch notation, C4 is middle C. sharps with `#` (`C#4`), flats with `b` (`Db4`), same note. range limits come from the active profile, not the notation: the checker knows an NES pulse can't go below ~A1 and a triangle sits an octave lower, and says so per note.

## lanes

the profile defines which lanes exist and what they can do. for `nes`: `pulse1`, `pulse2`, `triangle`, `noise`. parameters follow the lane name (`lane pulse1 duty 25`); which parameters a lane accepts is also the profile's call. a lane appears at most once per pattern, and lanes you don't use can be omitted.

## what `piro check` promises

every error names the lane, the bar, and the slot, in the same `bar.slot` syntax you write. the checker also prints each note's computed end position, so a seam like "does the bass stop exactly where the next note starts" is checkable by eye without doing the carry yourself.

"end" always means **the last slot the note rings in**, inclusive. a note at `1.9` with `x16` rings through `2.8`, and the next note may start at `2.9`. a perfect seam is end + 1 = next start, never end = next start.

two more facts the parser commits to, so they're spec now: notes within a lane may be written in any order (the checker sorts; position is truth, line order is convenience), and pattern names are unique per file (the future song order will reference them by name).

## open questions

- **the `x` sigil.** `x2` reads as "play twice" to anyone with tracker habits. it might need to be a different character, or nothing at all.
- **song structure.** patterns need an order, repeats, and probably sections and transpose, in the format itself. a format without them gets script generators written on top of it, which defeats the point of having a checkable format. shape not designed yet.
- **effects and ornaments.** vibrato, arpeggio, slides, duty changes mid-note: deliberately absent from v0. they're half the chip sound, so they're coming, but they're a design of their own.
- **the noise lane.** noise "pitch" is one of 16 timer settings, not a note. probably its own small notation, defined by the profile.
- **notes crossing pattern boundaries** in the song order: error, clip, or legal tie? currently unspecified, which means error until decided.
- **retrigger.** two back-to-back notes of the same pitch: does the envelope restart? (it should, but the engine has to actually do it.)
- **strict mode.** an opt-in check that every slot in every bar is accounted for, notes or written rests, no silent gaps. probably a flag on `piro check`.
