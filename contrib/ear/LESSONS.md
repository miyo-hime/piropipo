# What the listener's ear taught the ear

One entry for every time something in a song sounded wrong to the listener (or the ear got something
wrong): where it was, what they heard, what the composer was going for, why it didn't work, what we
changed, and whether it sounded better. Each lesson ends with what the ear now checks, so the next
composer gets told before anyone has to hear it.

Read this before composing, and before changing a measure in `theory.py`. Bars are named
`section |bar` like everywhere else. The song is "but we endure" (`songs/but-we-endure.piro`)
unless a lesson says otherwise.

## 1. The near-miss answer (service |1–|4)

- **Heard:** on Reddit, before any of this existed. Line 2 echoes line 1's shape, but its rhythm
  is slightly off: "adding rhythmic permutations between the lines is complex on complex. The
  note changes should probably line up."
- **Going for:** variation. The composer's skill says a repeated section "earns one audible
  difference", and warns that "a 7/8 song where every phrase is exactly one bar is correct and
  dead".
- **Why it didn't work:** the composer spent the difference twice. Line 2 moves the tune down a
  step *and* moves 2 of its 10 note starts. A rhythm that's almost the same reads as a mistake;
  a rhythm that's clearly different reads as a choice.
- **Changed:** line 2 takes line 1's exact rhythm. The held A becomes A for an eighth, then a
  re-struck A. The chip plays two same-pitch notes in a row as one held note, so the second A is
  louder (`vol`) to make it re-strike.
- **Verdict:** better (the listener). Hear it in `songs/but-we-endure-fix-Dsharp.piro`.
- **The ear now checks:** every line against the line before it. Same rhythm with new notes is
  an *answer* (good). A rhythm that's up to 35% different, with new notes too, is a *near miss*
  (warning, amber cells in the letters row). More than 35% different is *contrast* (fine).

## 2. The held E over B7 (service |4)

- **Heard:** "you end on the dominant, the E. I don't know why that doesn't sound right."
- **What it is:** the E is home (the key is E minor), but the chord under it is B7 (B D# F# A),
  the dominant. A held E over B7 is a suspension: it rubs half a step against the chord's D# and
  wants to step down to it. Instead the line jumps back to E and the rub never resolves.
- **Changed:** E for an eighth, then D# (the 4–3 suspension resolving). The listener's first instinct was
  "E then D", but a plain D would clash with the D# in pulse2's arpeggio. Both versions are kept to
  compare: `fix-D` and `fix-Dsharp`.
- **Verdict:** D# is better (the listener: "Yeah, better").
- **The ear now checks:** the last note of every line. It warns when the note rubs against
  anything sounding (a minor 2nd, major 7th or tritone) or sits outside the key, and the next
  note doesn't step to a chord tone. It names the step that would resolve it ("step it down to
  D#5 or up to F#5").

## 3. A broken chord with no bass under it (mask |2–|5)

- **Heard:** the ear labeled these bars Em7 / Gmaj7 / Bm and so on, changing chord every beat.
  the listener: "I think it's just E, C, A, B ... if I was the bass player, I would be playing a C, a C
  major 7, and then a B7."
- **The ear was wrong:** the melody plays alone here, as a broken chord that keeps returning to
  B5. With no bass, the ear treated each beat's lowest note as a bass and re-guessed the chord
  every beat, and it counted the returning B as a chord tone (which turned A minor into B7).
- **Changed (in the ear):** a note struck over and over in one bar is a pedal and is discounted.
  With no bass playing, a bar keeps one chord, and its lowest note stands in for the bass, the
  way an organist reads the first note of a broken chord. mask now reads Em7, Cmaj7, Am, B7,
  twice over.

## 4. A ninth at the top of a broken chord is not a problem (mask |3, |7)

- **The ear was wrong:** it warned "line ends on D6, not in the Cmaj7 chord". D6 over C is a
  ninth, a color note at the top of the figure, and the listener called the melody here fine.
- **Changed (in the ear):** a line ending on a scale tone that isn't in the chord is fine on its
  own. It only warns when that note actually rubs against something, or is outside the key.

## 5. The late copy (mask |6–|9)

- **Heard:** "there is now a delayed and overlapping copy of the melody underneath that's offset
  1 to the right ... It's going so fast that you don't notice that it sounds bad, but it does
  sound bad."
- **Going for:** the NES echo. The composer's skill teaches it as "the single most recognizable
  NES texture: repeat a phrase two slots late on the other pulse, quieter and thinner", here to
  build intensity into `service`. But this composer trailed by *one* slot, not two. The skill's
  own long-form guide warns against exactly that: "keep the trail on eighth-note boundaries; a
  trail landing on 16th offbeats makes the whole passage lurch."
- **Why it sounded bad:** mostly timing, not pitch. pulse2 plays the melody an octave down 83 ms
  late. A voice that doubles another fuses with it only if they start together (Huron's
  onset-synchrony principle, from *Tone and Voice*, 2001). At 83 ms apart the two copies don't
  fuse, and they're too close to hear as an echo, so the ear hears a flam: one line struck twice,
  sloppily. The moment the listener pointed at (B5 over E4) is actually a perfect fifth, the smoothest
  interval there is, so no table of clashing intervals would have caught it. The lag does also
  make real clashes: 7 of 27 overlaps are harsh, including B5 over C4 (major 7th, |7) and C5
  over B4 (minor 2nd, |8).
- **Changed:** pulse2 doubles the melody an octave down *in time*, and gets its drive from
  repeated notes instead of lag. That's a trumpet player's double tongue, **ta · ta ka** in 32nd
  notes under every melody eighth:
  ```
  melody:  E5 E5 E5 E5 | B5 B5 B5 B5     (32nds; each melody note is an eighth)
  before:  -- -- E4 E4 | E4 E4 B4 B4     one 16th late
  after:   E4 -- E4 E4 | B4 -- B4 B4     ta (rest) ta ka, ka a little softer
  ```
  mask went from 16 to 32 slots per bar to fit the 32nds. The "ka" is softer (`vol 3` after
  `vol 5`), so the chip re-strikes it instead of merging it into the "ta" before it. The
  rendered audio differs from the original only in pulse2, from 6.0 s to 10.7 s; every other
  sample is identical. Made by a script (applied to |6–|9: the listener named 6, 8 and 9, and |7 has the
  same echo).
- **Verdict:** *waiting for an ear.* Compare `songs/but-we-endure-tongued.piro` against
  the original.
- **The ear now checks:** any voice that plays the melody's notes a fixed few slots behind, four
  or more notes in a row. Under 150 ms behind is a **flam** (warning). An eighth or more is an
  **echo** (a note, not a warning). Echoes are still in service |9–|12, endure and fragments
  |1–|2, waiting for the listener to judge whether they bother them too.

## 6. A meter change has to announce itself

- **Direction:** "When you transition between a 7/8 and a 4/4, there needs to be
  something that clearly communicates you've transitioned ... growing white noise that then hits,
  and very, very heavy drums and very, very heavy rhythm to establish the new beat. You can't just
  go back and forth. The first measure of a new time change needs to establish that there has
  been a time change. Otherwise, people are going to get confused."
- **The original:** switched meter for a bar or two at a time with nothing marking the change.
  crack's tail went 7/8, 7/8, 5/4, and bow went 7/8, 7/8, 5/4, 4/4, 4/4.
- **The rewrite:** meter changes only at section boundaries, 7 times in all.
  - **The bar before** swells with white noise from nothing into a snare roll. Measured: about
    13 dB of climb across the bar, every time.
  - **The downbeat** is a crash.
  - **The first bar of the new meter** is heavy: a kick on every beat group, snares between, the
    last group in 16ths, the bass pounding the root, and a chord stab on every group.
- **Verdict:** *waiting for an ear.*
- **The ear doesn't check this yet.** Next would be a detector that flags a meter change with no
  rise in the noise lane before it and no hit on it.

## 7. A fragmented mind is a sound, not a broken melody

- **Direction:** "you shouldn't do that by making the melody literally fragment, because
  it sounds bad ... adding some sort of distortion or filter on the thing itself."
- **The original fragments:** the melody literally broken into bits by rests, with a late copy
  under it.
- **The rewrite:** one whole melody in one steady rhythm (4 2 2 2 2 8, new notes every bar). The
  fracture is all in the sound:
  - piro's own effects: a different tone (duty) on every note, deep vibrato on the long notes, a
    half-step slide into each F bar;
  - effects piro can't do, added by `master.py`: a low-pass closing from 9 kHz to
    1.5 kHz, a bitcrush deepening, and a 16th-note tremolo growing to half depth.
- **Learned:** on a chip pulse wave, which is already a hard square, overdrive and a light
  bitcrush barely change anything (brightness moved 3%). A filter and a tremolo do the work.
  Measured: fragments' effects go from 12 dB under the music to 6 dB under it by the last bars,
  and in shatter's last two bars the effect is louder than the music.
- **Verdict:** *waiting for an ear.*

## 8. A major 7th over its own root is a chord, not a clash

- **The case:** B over C, as in the theme's Cmaj7 bar and shatter |1. piro's harmony advisory
  flags it as a sustained major 7th. the listener called that chord "C major 7", so it stays.
- **The rule:** the rewrite's clash check, and the generator's swaps, only object to a half step
  or a tritone when the two notes aren't both in the chord.

## How the rewrite is made

`rewrite.py` writes `songs/but-we-endure-rewrite.piro` (and its `.fx.json`) from the
original's tunes and the rules above. Edit the generator, not the score. `master.py`
renders any song with its effects to `out/<song>.wav` and `.mp3`. The ear applies the same
effects when it builds, so the page plays what the master sounds like.
