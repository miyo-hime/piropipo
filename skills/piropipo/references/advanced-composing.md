# composing at full length

the main skill's loop carries a jingle. a two-minute piece with sections, a human giving
feedback in timestamps, or a meter beyond 4/4 each add craft of their own.

## the deaf loop

the discipline in one line: **render, measure, change something because of a measurement,
log the round.** a change you can't trace to a number is a guess wearing a work boot.

check's output is the measuring kit (cataloged in format.md). `piro roll` crops one section
to full width. when a balance question survives those, render variants: the same pattern
with a lane's notes removed tells you what that lane owns.

what the measurements are for:

- **a loudness map, not a loudness.** sections need designed contrast: a verse 3 to 5 db
  under its chorus, quiet sections far below that, and the map rechecked after every change.
  the classic failure reads as every section within one db of the same rms. everything
  screaming is nothing screaming.
- **the triangle holds quiet sections up.** a sustained bass drone keeps a "whisper" section
  loud. the fix is fewer or shorter triangle notes, not volume.
- **terraces beat ramps.** the chip does dynamics in steps: let sections enter at their
  level instead of crawling, and drop the floor out for a bar before a climax so the climax
  has somewhere to land.
- **the opening level is UX.** a piece that starts near silence reads as a broken file to
  someone scrolling. around -18 db rms at the first bar is an opening; -23 is dead air.
- **vibrato depth is bought in ticks.** one register tick is roughly 15 to 21 cents, and an
  ask of 20ct often rounds to a single tick you'll never hear over the band. ask for 30 to
  40ct and read the achieved swing in the report; the report is the truth, the ask is a wish.
- **duty carries loudness.** 25 sits about 3 db under 50 at the same vol. casting a lane's
  duty is also setting its weight in the mix.
- **tempo picks your effect quality.** check reports the achieved arp and slide rates; if a
  tempo fights the effects, consider moving the tempo, not the music.

where the kit ends: the native ears are per-bar and per-lane-share. what they don't measure
is the relationship between two specific parts - register distance, onset density, who masks
whom. when a question outruns the kit, `check --events` is the raw material: the whole score
as json with sample-exact times, a dozen lines of anything away from the meter you need. and
for "what does this lane own," measure directly: delete the lane in a copy, re-render,
compare levels.

log each round: what was measured, what changed because of it, what the re-measure said.

## revising under ear feedback

a human with ears will hand you notes as timestamps.

1. **freeze the timeline.** keep every pattern length and the play order fixed across
   revision rounds, so the piece stays the same duration and every timestamp in their notes
   keeps pointing at the same music. restructure only when the note explicitly asks for it.
2. **map timestamps to your own table.** check's `sections` carry `start_seconds` per play:
   resolve each complaint to a pattern and bar range before touching anything.
3. **measure the complaint.** "these two parts fight" becomes register distance, shared
   slots, and relative level, measured. the fix then targets the real cause: two voices a
   third apart at equal volume is a register problem, and dropping one an octave beats
   turning it down.
4. **keep a protected list.** the parts that won stay byte-identical, and you prove it with
   a diff after revising. nothing erodes feedback like a fix that quietly moved the chorus.
5. **land rounds as diffable folders.** each round gets its own copy and a changes file:
   what moved and why, what was protected and how that was verified, the measure/re-measure
   receipts, and watch items phrased as "if X still bothers you, the lever is Y."
6. **check for overshoot inside the round.** after the fix, re-measure the thing the
   complaint was about. a knife that stepped back so far the main voice went quiet is a new
   bug, not a fix.

learning from a reference score is allowed the way musicians do it: steal technique,
quarantine melodies. the shape of a transpose ladder travels; the tune does not.

## full-length craft

- **repetition discipline.** a section played twice earns one audible difference: the echo
  following the melody this time, the riff moved down an octave with a wail over it, a
  walk-down at the phrase turn. the roll read against the play order is how a carbon copy
  gets caught. the positive form: one hook cell, reused everywhere with its clothes changed,
  is what makes a piece cohere.
- **transpose ladders.** `play chorus +2` is the classic final-chorus lift, and a ladder of
  shifts can close a chase one semitone at a time. but pitch alone adds almost nothing to
  the meter: pair the lift with a volume or texture step or it won't read as one.
- **comping under a lead.** offbeat chord stabs, a 3-3-2 push, and a `fade` on every chord
  note so the pad decays like an electric piano and gets re-struck. a held `arp` chord under
  a busy melody reads as buzz at 60 steps a second; stabs breathe, drones grind.
- **echo canons.** a second pulse trailing the lead by two slots, quieter and thinner, is
  still the king of chip textures at length. keep the trail on eighth-note boundaries; a
  trail landing on 16th offbeats makes the whole passage lurch.
- **phrasing across the barline.** a constant eighth-note pulse with odd groupings (3+3+2,
  6+6+4) riding over the bar boxes, breaths only at group ends, is how long lines stay alive
  in fixed bars. split phrases where the drums already seam, not where arithmetic says.
- **written rubato.** a one-bar pattern at a different grid is a tuplet time-slip: the same
  four-beat box, suddenly subdivided a third slower or a quarter faster. a stretch bar before
  a drop, a compressed bar as a smash cut.

- **odd meter is native.** `pattern verse bars 4 beats 7 grid 14` is a true 7/8; the play
  block mixes meters freely. keep the grid a multiple of the beats so slots mean even
  subdivisions, and know two things about the ears in odd meters: the harmony advisory
  trusts only the downbeat (it won't guess whether your 7 is 2+2+3 or 3+2+2), and arp and
  slide pacing follow the real bar length, so an effect that fits in 4/4 can need a longer
  note in 7/8 or the reverse - the error names the length that fits, as always. the
  barline-phrasing technique above is not a meter workaround, it's still the music: a 7/8
  song where every phrase is exactly one bar is correct and dead.

## if you write a program that emits piro

sketches carry most pieces, anchors and all. a program earns its place only when motif math
outruns the notation: a riff transposed through a whole progression, voice-led harmony,
rule-driven variation at scale. if you get there:

- emit **sketches**, not positioned `.piro`: `piro sketch` owns the placing and the
  bar-fill check.
- **transposing in your own code must also rewrite `from` targets.** a slide's departure
  is a pitch like any other; shift the note and not the `from` and the scoop walks in from
  the wrong world.
