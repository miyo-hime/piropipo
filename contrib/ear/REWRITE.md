# Rewriting but we endure

The tunes are yours; what changed is how they're played. Every change traces back to a rule, and
every rule came from listening (see LESSONS.md). Listen to it in
[media/but-we-endure-rewrite.mp4](media/but-we-endure-rewrite.mp4), or regenerate it with
`rewrite.py` and `master.py`.

## The rules, and the idea behind each

1. **An answering line keeps its call's rhythm.** Line 2 of service shaped itself after line 1,
   then moved two note starts. A rhythm that's *almost* the same reads as a mistake, and a clearly
   different one reads as a choice. So vary the notes or the rhythm, not both. Every answer in
   the rewrite takes its call's exact rhythm and lets the pitches carry the difference.
2. **Line endings resolve.** Line 2 ended on a held E over B7. The E is home, but under B7 it rubs
   half a step against the D# and never steps down to it. Now it does: E, then D#, a 4–3
   suspension.
3. **No late copies.** pulse2 often played the melody an octave down one 16th late (mask, binding,
   endure, stretch). At 83 ms apart two copies of a line don't fuse into one voice, and they're too
   close to hear as an echo, so the ear hears a flam. Doublings now start *with* the melody and
   get their drive from repeated notes instead: 32nds articulated "ta · ta ka", like a trumpet's
   double tongue.
4. **The accompaniment plays chord tones and gets out of the melody's way.** It sits an octave or
   more below the tune. Any tone that would land a half step or a tritone from the melody note
   sounding at that moment is swapped for another chord tone, unless both notes belong to the
   chord (B over C stays, because that's Cmaj7).
5. **A meter change announces itself.** The bar before it swells with white noise from nothing
   into a snare roll. The downbeat crashes. The first bar of the new meter is heavy: a kick on
   every beat group, snares between, the last group in 16ths, the bass pounding the root, and a
   chord stab on every group. A listener can't follow 7/8 → 4/4 → 5/4 if nothing marks it.
6. **Meter changes only between sections.** The original flipped meter for a bar or two at a time.
   crack's tail went 7/8, 7/8, 5/4, and bow went 7/8, 7/8, 5/4, 4/4, 4/4. Those are now written in
   their section's meter.
7. **A fragmented mind is a sound, not a broken melody.** Chopping the tune into pieces just
   sounded broken. fragments now keeps one whole melody, and the fracture lives in the sound:
   piro's own effects (a different duty on every note, deep vibrato, slides), plus effects piro
   can't do, added after the render (a low-pass closing, a deepening bitcrush, a tremolo chopping
   in 16ths). On a chip pulse wave, overdrive and a light bitcrush barely register, because the
   wave is already a hard square. The filter and the tremolo do the work.
8. **Busy rhythm wants a calm melody.** crack's tail was 16th-note leaps under full drums and a
   doubling, all at once. It's now the opening theme in eighths: crack winds down by quoting
   where the song began.

## Section by section

| section | meter | what changed |
|---|---|---|
| mask | 7/8, 10 bars | The door chime now rings over two 7/8 bars instead of one 4/4 bar, so the song starts in its home meter. Then the broken-chord theme twice. The second time, pulse2 doubles it in time with the double tongue instead of one 16th late. The bass enters for the last two bars, then a snare fill into service. |
| service | 7/8, 16 bars | Line 2 takes line 1's rhythm (with a re-struck A), and its E steps down to D#. Lines 3–4 follow the same answer shape. Under the melody is a 16th-note roll of chord tones; your chromatic "arguing" pulse2 lines are gone, which is a taste call more than a fix. The loud repeat (lines 5–6) gets the double tongue instead of a 250 ms echo. The runs in bars 13–15 are answered by pulse2 in their rests. Bar 16 takes bar 14's rhythm, and its rest is the riser into crack. |
| crack | 4/4, 11 bars | A heavy first bar. Your melody stays, with bar 6 re-rhythmed to match bar 2. The 7/8, 7/8, 5/4 tail is now three bars of 4/4 quoting the opening theme in eighths (Em, Am, B7). |
| endure | 4/4, 2 bars | The melody over a soft held chord, instead of an octave-up echo two slots behind. Under it, a two-bar white-noise swell into fragments, the longest breath in the song. |
| fragments | 5/4, 8 bars | One whole melody in one rhythm (4 2 2 2 2 8 sixteenths), new notes every bar over Em F Em F Am F B7 B7. Over the F chords the line steps through D, never B: B over the F bass is the tritone that kept rubbing. Duty flips, vibrato, a half-step slide into each F bar, and the effects grow across the section. Heavy first bar, riser at the end. |
| stretch, compress | 4/4 in triplets, then quintuplets | Kept as the gear change, now with drums that play the subdivision: triplet snares, then a quintuplet roll. pulse2 doubles in time. The quintuplet roll is the riser into tamagoe. |
| tamagoe | 4/4, 6 bars | A heavy first bar, then hats only. Bar 3 re-rhythmed to match bar 1. The high pad is struck with each bar instead of a beat late. |
| binding | 7/8, 8 bars | A heavy first bar, then the theme with the double tongue throughout. The last bar is the theme's own B7 bar, carrying the riser into crack. |
| crack2 | 4/4, 11 bars | crack again, driven by the double tongue instead of the roll: the one audible difference for the repeat. |
| shatter | 4/4, 8 bars | Your climax melody. Bar 4 comes down in quarters (B A G D) instead of almost repeating bar 2's rhythm. In bar 7 the harmony line takes thirds and sixths instead of hitting B7's tritone twice. The section is pushed into overdrive, then its last two bars are bitcrushed and chopped before bow puts it back together. |
| bow | 7/8, 6 bars | One heavy bar to say we're home in 7/8, then the theme alone over a held bass. The door chime from the opening closes the song: G over E minor. |

## Things your version did on purpose that we changed

Your README says the two pulse lines argue on purpose, and the ending stays unresolved because
she does too. We heard the arguing as clashes, and swapped it for chord tones. We also ended on
the chime over E minor: on the third, open, but home. Both are taste calls, not corrections.

## Measured

- **The ear's findings on the whole song: 34 → 0.** The original's 34 were 3 flams, 16 short
  clashes, 9 near-miss rhythms, 3 unresolved endings and 3 echoes.
- **piro's harmony advisory: 4 → 1.** The one left is B6 over Cmaj7 at the start of shatter,
  kept on purpose.
- **Every meter change (7 of them):** the noise lane climbs about 13 dB across the bar before it,
  and a crash lands on the downbeat.
- **The effects:** sections without effects are bit-identical to piro's own render. In fragments
  the effects grow from 12 dB under the music to 6 dB under it. In shatter's last two bars the
  effect is louder than the music.
- **Length:** 113 s (the original is 112).

These are measurements, not ears. Whether the transitions land and whether fragments reads as a
mind breaking (or just a bad speaker) hasn't been judged by anyone listening yet.

## How the generator works

`rewrite.py` builds each section from the original's tunes plus a handful of arrangement moves:
`tongue`, `double`, `roll`, `stab`, `pad`, `answer`, `bass`, `groove`, `heavy`, `riser` and
`fill`. It writes positioned `.piro`, with the meter set up per section:
- `beats 7 grid 28` for 7/8 (one box holds two bars), or `grid 56` where the double tongue needs
  32nds;
- `beats 5 grid 20` for 5/4;
- plus the meter comments the ear reads.

Before writing, it runs its clash check and reports anything left. `master.py` then renders each
lane with piro, applies the `.fx.json` effects to the named sections, lanes and bars, and mixes.
