# sfx: from real units to sfxr's floats

> status: v1, implemented in `src/sfx.rs` against `sfxr 0.1.4`. every formula here was read out of the crate's source, not its doc comments (the two disagree in places, see the quirks at the bottom).

tl;dr: `piro sfx coin` makes a coin. flags speak Hz, milliseconds, and semitones. sfxr speaks twenty-odd floats between 0 and 1. this page is the exchange counter between the two, plus the house rules for when your flags and a preset's own shape disagree.

## the clock everything hangs off

sfxr renders at 44,100 samples a second and runs its oscillator 8 times per output sample, so the oscillator ticks **352,800** times a second. every pitch below is a period counted in those ticks, and every duration is a count of output samples.

## the presets

seven names on our side. the crate ships seven constructors, two of them under different names:

| `piro sfx` | sfxr 0.1.4 | what it rolls |
|---|---|---|
| `coin` | `Sample::pickup` | short bright tone, usually with a second note jumping up (the arpeggio) |
| `jump` | `Sample::jump` | square wave gliding upward |
| `hurt` | `Sample::hit` | square, sine, or noise gliding downward fast |
| `explosion` | `Sample::explosion` | low noise with punch, sometimes a phaser |
| `powerup` | `Sample::powerup` | rising tone, sometimes stuttering (the repeat) |
| `blip` | `Sample::blip` | tiny square or sine menu tick |
| `laser` | `Sample::laser` | fast downward glide that bottoms out on a floor pitch |
| `custom` | `Sample::new`, reshaped | no preset at all, see custom mode |

each constructor rolls its own dice from a seed. ours defaults to **seed 9**, so the same command writes the same wav, byte for byte, on the same build. `--seed N` (any whole number from 0 up) rolls a different variation; the seed used is always echoed back in the stats. nine is not arbitrary: it's the first coin this project ever made.

the seed only rolls the preset's knobs. the crate seeds its noise generator at a constant, so the grain of the hiss in `explosion` is the same for every seed.

## the knobs

| flag | unit | accepts | bounds | sfxr field |
|---|---|---|---|---|
| `--wave` | name | `square`, `saw`, `sine`, `noise` | the menu | `wave_type` |
| `--pitch` | Hz | `880`, `880hz` | 4 to 3500 | `base_freq` (and `freq_limit`, see transposing) |
| `--length` | ms | `300`, `300ms`, `0.3s` | 1 to 6801 | sets all three envelope stages |
| `--attack` | ms | same as length | 0 to 2267 | `env_attack` |
| `--decay` | ms | same as length | 0 to 2267 | `env_decay` |
| `--sweep` | semitones | `+12`, `-7`, `12st` | -72 to +72 | `freq_ramp` (and clears `freq_limit`) |
| `--seed` | dice | `0` and up | any u64 | the preset constructor's seed |

anything that doesn't parse, or parses to NaN or infinity, is a `bad-value` error. anything finite but past its bounds is an `out-of-range` error that names the bounds. nothing gets clamped quietly, and every error is collected in one pass so a bad command reports all its problems at once.

### the wave menu, honestly

| `--wave` | sfxr | what the crate actually computes |
|---|---|---|
| `square` | `WaveType::Square` | ±0.5, duty from the preset (50% in custom) |
| `saw` | `WaveType::Triangle` | `1.0 - phase * 2.0`, a falling sawtooth |
| `sine` | `WaveType::Sine` | `sin(phase * 2π)` |
| `noise` | `WaveType::Noise` | 32 random steps per cycle, redrawn every cycle |

the crate's `Triangle` is a sawtooth wearing a name tag, so there is no `tri` here. asking for `triangle` gets an error that points at `saw`. a real triangle would be new synthesis, not a rename.

for `noise`, pitch means grain: each cycle holds 32 random steps, so `--pitch 100` redraws the noise 3,200 times a second. low pitches rumble, high ones hiss.

### pitch

```
period    = round(352800 / hz)                  oscillator ticks per cycle
base_freq = sqrt(100 / (period + 0.5) - 0.001)
sounds at   352800 / period  Hz
```

sfxr stores `fperiod = 100 / (base_freq² + 0.001)` and truncates it to a whole number of ticks. we pick the nearest whole period and aim at its middle (`+ 0.5`), so truncation lands exactly where we meant. the price is that only whole periods exist: the error is under 9 cents at 3500 Hz and shrinks as the pitch drops. `pitch_hz` in the stats is the pitch that really sounds, not the one you typed.

the crate's own range is about 3.53 to 3531 Hz (`base_freq` 0 to 1); we accept 4 to 3500.

### envelope stages

```
samples = round(ms * 44.1)
float   = sqrt((samples + 0.5) / 100000)        capped at 1.0
```

sfxr turns each stage float back into `float² * 100000` samples, truncated. the `+ 0.5` makes that round trip exact for every count from 0 to 100,000 (a test walks all of them). two adjustments on top:

- **attack is stored as samples + 1.** sfxr spends the first count of its attack stage on entry, so a stored attack of n plays n - 1 samples. we add one so `--attack 20ms` is 20 ms.
- **sustain and decay never go below one sample.** sfxr divides by the stage length, so an empty sustain or decay is 0/0, and the NaN gets into its filters and pins the rest of the sound to full scale. `--decay 0` is honoured as one sample (0.02 ms), which is as close to a hard cut as the crate allows without breaking.

one stage tops out at 100,000 samples (2267.6 ms), which is where the 2267 ms bounds come from. the three stages back to back give the 6801 ms ceiling on `--length`.

the wav is exactly attack + sustain + decay samples long. sfxr itself would keep generating silence forever; we stop where the envelope ends, like the original sfxr did.

### sweep

```
N         = samples in one cycle (the whole sound, or one repeat if the sound repeats)
slide     = 2 ^ (-semitones / (12 * N))         sfxr multiplies the period by this every sample
freq_ramp = cbrt((1 - slide) / 0.01)
```

`freq_ramp` lives in -1 to 1, so the fastest glide sfxr can do is about 7.6 semitones per millisecond. a sweep that asks for more than that in a very short sound, or that runs into sfxr's floor (3.53 Hz) or ceiling (44.1 kHz), still renders, with a `sweep-clamped` warning saying how far the glide actually got. `sweep_semitones` in the stats is always the travel that really happens, measured from the start pitch to wherever the glide ends up (arpeggio jumps and vibrato wobble aren't counted, they're not travel).

## when your flags and the preset disagree

every preset arrives as a full shape: three envelope stages, a pitch glide, sometimes an arpeggio (a single jump to a second note) and a repeat (the whole pitch motion restarting on a timer). flags reshape it by these rules.

### the envelope

sustain is never set directly; it's whatever's left.

| you give | what happens |
|---|---|
| nothing | the preset's stages, untouched |
| `--attack` and/or `--decay` | those stages become exactly what you said. sustain keeps the preset's value. the total length changes as a consequence |
| `--length` alone | **scale.** all three stages stretch or shrink by the same factor, so the shape survives at the new size |
| `--length` with `--attack` and/or `--decay` | your pinned stages are exact. the stages you didn't pin share the remaining time in the same proportions they had in the preset. sustain absorbs the rounding |

and the conflicts:

- **attack + decay longer than length** is an `envelope-overflow` error naming all three numbers. you asked for something that can't exist, so nothing gets written.
- **a stretched stage past 2267 ms** is a `stage-too-long` error that names the longest length the preset's shape can reach, and suggests pinning `--attack` / `--decay` to go further. (pinning works because pinned stages stop taking a proportional share.)
- if every stage you left free was zero in the preset, sustain takes the whole remainder.

### the pitch motion follows the length

whenever the envelope changes the sound's total length, by any of the routes above, the preset's motion in time is stretched by the same factor, so a coin squeezed to 60 ms still reaches its second note:

- **glide:** the preset's travel in semitones is kept, and `freq_ramp` is re-solved for the new cycle length (`slide_new = slide_old ^ (N_old / N_new)`).
- **arpeggio:** the moment of the jump moves by the factor. sfxr can only schedule it between 33 and 20,032 samples (0.7 to 454 ms); past that it lands on the nearest edge with an `arp-clamped` warning.
- **repeat:** the repeat period moves by the factor.

not stretched: pitch, wave, vibrato rate, and the slow drifts of duty, filters, and phaser. ※ those keep their per-sample rates, so a heavily stretched laser drifts its timbre further than the original did. if that ever matters, they can be re-solved the same way the glide is.

### sweep replaces, it doesn't stack

`--sweep` throws out the preset's glide and installs yours, from the start pitch, over one cycle. it also removes the preset's floor (`freq_limit`, the pitch a laser bottoms out on), because a floor would quietly stop your sweep short. the arpeggio and vibrato stay: they're the preset's character, not its travel.

### pitch transposes

`--pitch` sets the start pitch. if the preset has a floor, the floor moves with it by the same ratio, so a laser played lower still falls the same number of semitones before it bottoms out. the arpeggio is a ratio already, so it moves along for free.

### wave just swaps the oscillator

everything else about the preset stays. a `jump --wave noise` is a rising whoosh.

## custom mode

`piro sfx custom --wave sine --pitch 440` builds from a blank sample instead of a preset.

- **required:** `--wave` and `--pitch`. a missing one is a `missing-flag` error. there's no preset to inherit them from, and guessing would just be a preset nobody asked for.
- **the blank shape:** attack 0 ms, sustain 100 ms, decay 200 ms (300 ms total), no glide, no arpeggio, no repeat, no vibrato, no filters, square at 50% duty. `--length`, `--attack`, `--decay`, and `--sweep` reshape it by exactly the rules above.
- **`--seed` is an error** (`seed-in-custom`): there are no dice to roll, and a flag that does nothing should say so instead of being ignored.

## what comes back

the output is a 16-bit mono wav at 44.1 kHz, at sfxr's own export level (the crate's default gain of 0.2 matches the original sfxr's wav export). it lands at `<preset>.wav` in the current directory unless `-o` says otherwise, published atomically: written whole to a hidden sibling file, synced, then renamed over the target, so a failed run never leaves half a wav or eats the previous one.

stats in the json envelope:

| field | meaning |
|---|---|
| `preset`, `seed`, `wave` | what was rolled (`seed` is null in custom) |
| `pitch_hz` | the start pitch that really sounds |
| `sweep_semitones` | glide travel per cycle, as rendered |
| `attack_ms`, `sustain_ms`, `decay_ms` | the envelope as rendered |
| `duration_seconds`, `samples` | length of the wav |
| `peak_dbfs` | loudest sample against full scale (null if silent) |
| `saturated_samples` | samples sitting on full scale |

warnings: `sweep-clamped`, `arp-clamped`, `saturation` (some samples hit full scale), `silent` (the whole thing is zeros). errors: `unknown-preset`, `bad-value`, `out-of-range`, `missing-flag`, `seed-in-custom`, `envelope-overflow`, `stage-too-long`, `write-failed`.

## crate quirks we sand off

found by reading `sfxr 0.1.4` against the original sfxr. the fixes live at our boundary; the crate is untouched.

- **zero-length stages** produce 0/0 and poison the filters with NaN. floored at one sample (above).
- **the repeat timer** is computed as `(1 - r)² * 20000 * 32` where the original sfxr has `* 20000 + 32`, so repeats in 0.1.4 almost never fire inside a sound's lifetime and `powerup` loses its stutter. we read the preset's repeat with the original formula and feed the crate the value that gets it there.
- **negative arpeggios** (only `explosion` rolls them) should drop the pitch. 0.1.4 computes `1 - m² * 10` where the original has `1 + m² * 10`, which jumps the pitch up instead, and past |m| = 0.32 inverts the period and pins the oscillator at 44.1 kHz. the crate's formula can't express a downward jump at all, so negative arpeggios are dropped.
- **the triangle** is a saw. renamed, not fixed.
- **the floor holds instead of ending.** in the original sfxr, a glide that reaches `freq_limit` ends the sound. 0.1.4 holds the floor pitch until the envelope finishes. we keep the crate's behaviour; it's why `laser` ends on a steady low tone.
