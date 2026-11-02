# sfx: presets and flags

`piro sfx coin` makes a coin. flags speak Hz, milliseconds, and semitones; the synth's
internals are not your problem. the stats that come back always describe what really
rendered, so trust them over what you asked for.

## the presets

| `piro sfx` | what it rolls |
|---|---|
| `coin` | short bright tone, usually with a second note jumping up (the arpeggio) |
| `jump` | square wave gliding upward |
| `hurt` | square, sine, or noise gliding downward fast |
| `explosion` | low noise with punch, sometimes a phaser |
| `powerup` | rising tone, sometimes stuttering (the repeat) |
| `blip` | tiny square or sine menu tick |
| `laser` | fast downward glide that bottoms out on a floor pitch and holds it |
| `custom` | no preset at all, see custom mode |

each preset rolls its variations from a seed, default **9**, so the same command writes
the same wav byte for byte. `--seed N` (any whole number) rolls a different variation;
the seed used is echoed in the stats. the grain of the noise itself is the same for every
seed - only the preset's knobs roll.

## the flags

| flag | unit | accepts | bounds |
|---|---|---|---|
| `--wave` | name | `square`, `saw`, `sine`, `noise` | the menu |
| `--pitch` | Hz | `880`, `880hz` | 4 to 3500 |
| `--length` | ms | `300`, `300ms`, `0.3s` | 1 to 6801 |
| `--attack` | ms | same as length | 0 to 2267 |
| `--decay` | ms | same as length | 0 to 2267 |
| `--sweep` | semitones | `+12`, `-7`, `12st` | -72 to +72 |
| `--seed` | dice | `0` and up | any u64 |

anything that doesn't parse is a `bad-value` error; anything finite but past its bounds
is an `out-of-range` error naming the bounds. nothing gets clamped quietly, and a bad
command reports all its problems at once.

notes on the units:

- **pitch**: the synth only has whole oscillator periods, so the sounding pitch can
  differ from the ask by up to ~9 cents at the very top of the range. `pitch_hz` in the
  stats is the pitch that really sounds.
- **wave**: there is no triangle; asking for one gets an error pointing at `saw`. for
  `noise`, pitch means grain - each cycle holds 32 random steps, so low pitches rumble
  and high ones hiss.
- **envelope**: a sound is attack + sustain + decay, and the wav is exactly that long.
  one stage tops out at 2267 ms, which is where the 6801 ms ceiling on `--length` comes
  from. `--decay 0` is honoured as the shortest cut the synth allows (0.02 ms).
- **sweep**: the fastest glide possible is about 7.6 semitones per millisecond; a sweep
  that asks for more in a very short sound, or that runs into the synth's pitch floor or
  ceiling, still renders, with a `sweep-clamped` warning saying how far the glide really
  got. `sweep_semitones` in the stats is always the travel that actually happens.

## when your flags and the preset disagree

every preset arrives as a full shape: three envelope stages, a pitch glide, sometimes an
arpeggio (a single jump to a second note) and a repeat (the pitch motion restarting on a
timer). flags reshape it by these rules.

### the envelope

sustain is never set directly; it's whatever's left.

| you give | what happens |
|---|---|
| nothing | the preset's stages, untouched |
| `--attack` and/or `--decay` | those stages become exactly what you said. sustain keeps the preset's value. the total length changes as a consequence |
| `--length` alone | **scale.** all three stages stretch or shrink by the same factor, so the shape survives at the new size |
| `--length` with `--attack` and/or `--decay` | your pinned stages are exact. the stages you didn't pin share the remaining time in the same proportions they had in the preset |

and the conflicts:

- **attack + decay longer than length** is an `envelope-overflow` error naming all three
  numbers.
- **a stretched stage past 2267 ms** is a `stage-too-long` error that names the longest
  length the preset's shape can reach, and suggests pinning `--attack` / `--decay` to go
  further.

### the pitch motion follows the length

whenever the envelope changes the sound's total length, the preset's motion in time
stretches by the same factor, so a coin squeezed to 60 ms still reaches its second note:
the glide keeps its travel in semitones, the arpeggio's jump moves with the length (its
schedulable window is 0.7 to 454 ms; past that it lands on the nearest edge with an
`arp-clamped` warning), and the repeat period moves too. pitch, wave, and the slower
timbre drifts keep their own rates.

### sweep replaces, it doesn't stack

`--sweep` throws out the preset's glide and installs yours, from the start pitch. it also
removes the preset's pitch floor (the pitch a laser bottoms out on), because a floor
would quietly stop your sweep short. the arpeggio and vibrato stay: they're the preset's
character, not its travel.

### pitch transposes

`--pitch` sets the start pitch. a preset's floor moves with it by the same ratio, so a
laser played lower still falls the same number of semitones before it bottoms out.

### wave just swaps the oscillator

everything else about the preset stays. a `jump --wave noise` is a rising whoosh.

## custom mode

`piro sfx custom --wave sine --pitch 440` builds from a blank sample instead of a preset.

- **required:** `--wave` and `--pitch`. a missing one is a `missing-flag` error.
- **the blank shape:** attack 0 ms, sustain 100 ms, decay 200 ms (300 ms total), no
  glide, no arpeggio, no repeat, square at 50% duty. `--length`, `--attack`, `--decay`,
  and `--sweep` reshape it by exactly the rules above.
- **`--seed` is an error** (`seed-in-custom`): there are no dice to roll.

## what comes back

a 16-bit mono wav at 44.1 kHz, landing at `<preset>.wav` in the current directory unless
`-o` says otherwise. a failed run never leaves a half-written file.

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

warnings: `sweep-clamped`, `arp-clamped`, `saturation`, `silent`. errors:
`unknown-preset`, `bad-value`, `out-of-range`, `missing-flag`, `seed-in-custom`,
`envelope-overflow`, `stage-too-long`, `write-failed`.
