// the snes card's driver: it turns a pattern's lanes into the key-ons, key-offs, and pitch and volume writes a song engine makes,
// and runs them through our s-dsp at the chip's own 32 khz. arps and slides keep the nes card's step schedule, so check's pacing
// rules and reports mean the same thing on both

use super::{RenderError, Tail, charged, clock_at, lattice, staff, steps, timing};
use crate::bank::{self, Instrument, Kind};
use crate::dsp::{Adsr, Chip, FULL_PITCH, Key, RATE, RELEASE, Source, VOICES};
use crate::profile::{self, Flaw, SNES};
use crate::score::Position;
use crate::score::{Bend, Pattern, Song};
use std::f64::consts::TAU;

// the driver rewrites pitch and volume every 16 samples, half a millisecond
const TICK: u64 = 16;

enum Contour { Still(f64), Shake { pitch: f64, cents: f64, hz: f64, from: u64 }, Steps(Vec<(u64, f64)>), Glide { from: f64, to: f64, start: u64, land: u64 } }

impl Contour {
    fn at(&self, t: u64) -> f64 {
        match self {
            Contour::Still(pitch) => *pitch,
            Contour::Shake { pitch, cents, hz, from } if t >= *from => pitch + cents / 100.0 * (TAU * hz * (t - from) as f64 / f64::from(RATE)).sin(),
            Contour::Shake { pitch, .. } => *pitch,
            Contour::Steps(steps) => steps.iter().take_while(|(at, _)| *at <= t).last().map_or(steps[0].1, |s| s.1),
            Contour::Glide { to, land, .. } if t >= *land => *to,
            Contour::Glide { from, to, start, land } => from + (to - from) * t.saturating_sub(*start) as f64 / (land - start).max(1) as f64,
        }
    }

    fn turns(&self, t: u64) -> bool { matches!(self, Contour::Steps(steps) if steps.iter().any(|(at, _)| *at == t)) }
}

struct Strike { voice: usize, start: u64, off: u64, source: Source, adsr: Adsr, echo: bool, level: [i32; 2], fade: bool, contour: Contour, tone: Option<&'static Instrument>, fixed: u16, clock: Option<u8> }

impl Strike {
    fn pitch(&self, t: u64) -> u16 { self.tone.map_or(self.fixed, |inst| inst.register(self.contour.at(t)).unwrap_or(FULL_PITCH)) }

    // a fade runs the volume down to nothing by the key-off, so the release that follows is silent
    fn volume(&self, t: u64) -> [i32; 2] {
        if !self.fade { return self.level; }
        let left = self.off.saturating_sub(t) as f64 / (self.off - self.start).max(1) as f64;
        self.level.map(|v| (f64::from(v) * left).round() as i32)
    }
}

// the key-off lands one release before the written end, so the release is over when the next note or the rest begins;
// a note too short for that keys off halfway and lets its release spill
fn gate(start: u64, end: u64) -> u64 { if end - start > 2 * RELEASE { end - RELEASE } else { start + (end - start) / 2 } }

pub fn press(song: &Song, pattern: &Pattern, chip: &mut Option<Chip>) -> Result<Vec<[i16; 2]>, RenderError> {
    let card = &SNES;
    let room = card.room(song).map_err(|breach| charged("", breach))?;
    if !(1..=255).contains(&song.tempo) { return Err(RenderError::Tempo(song.tempo)); }
    let tempo = song.tempo;
    let staff = staff(pattern.beats, pattern.grid)?;
    if let Some(stray) = pattern.lanes.iter().find_map(|l| card.part(&l.name).err()) { return Err(charged("", stray)); }
    let at = |units: u64| u64::try_from(clock_at(RATE, tempo, staff.grid, units)).unwrap_or(u64::MAX);
    let span = u64::from(pattern.bars) * u64::from(pattern.grid) * staff.unit;
    let mut strikes = Vec::new();
    for (voice, part) in card.parts.iter().enumerate() {
        let Some(lane) = pattern.lanes.iter().find(|l| l.name == part.name) else { continue };
        let name = lane.name.as_str();
        let seat = part.seat(&lane.params).map_err(|breach| charged(name, breach))?;
        let Some(inst) = seat.inst else { continue };
        let (pan, echo) = seat.setup.patch.map_or((0, false), |p| (i32::from(p.pan), p.echo));
        let mut notes: Vec<_> = lane.notes.iter().collect();
        notes.sort_by_key(|n| n.at.tick(pattern.grid));
        let mut cursor = 0;
        for note in notes {
            let Position { bar, slot } = note.at;
            let first = note.at.tick(pattern.grid) * staff.unit;
            let last = first + staff.span(note.len);
            if bar == 0 || slot == 0 || slot > pattern.grid || note.len == 0 || first < cursor || last > span { return Err(RenderError::Misfit { lane: name.into(), at: note.at }); }
            cursor = last;
            let Some(pitch) = note.pitch else { continue };
            seat.plays(note).map_err(|breach| charged(name, breach))?;
            timing(tempo, staff, first, note, pitch).map_err(|flaw: Flaw| RenderError::Ornament { lane: name.into(), at: note.at, flaw, played: None })?;
            let (start, end) = (at(first), at(last));
            let reg = i32::from(note.ornaments.vol.unwrap_or(seat.setup.volume)) * 8;
            let level = [reg * (8 - pan).min(8) / 8, reg * (8 + pan).min(8) / 8];
            let written = f64::from(pitch);
            let (source, adsr, tone, fixed, clock) = match inst.kind {
                Kind::Tone(sample) => (sample.source(), seat.adsr().unwrap_or(inst.adsr), Some(inst), 0, None),
                Kind::Kit(_) => {
                    let drum = inst.drum(pitch).ok_or_else(|| charged(name, profile::Breach::Drumless { at: note.at, pitch }))?;
                    (drum.sample.source(), seat.setup.patch.and_then(|p| p.adsr).unwrap_or(drum.adsr), None, bank::stored_register(drum.sample), None)
                }
                Kind::Noise => (Source::Noise, seat.adsr().unwrap_or(inst.adsr), None, 0, Some(profile::clock(pitch))),
            };
            let walk = |units: u64| {
                let grain = u64::from(lattice(staff.grid) / staff.grid);
                let origin = first * grain;
                steps(tempo, staff.grid, last - first).into_iter().scan(origin, |cum, len| { let from = *cum; *cum += len; Some(from) }).take(usize::try_from(units).unwrap_or(usize::MAX)).map(|u| u64::try_from(clock_at(RATE, tempo, lattice(staff.grid), u)).unwrap_or(u64::MAX)).collect::<Vec<_>>()
            };
            let contour = match &note.ornaments.bend {
                _ if tone.is_none() => Contour::Still(written),
                None => Contour::Still(written),
                Some(Bend::Vibrato(vib)) => Contour::Shake { pitch: written, cents: vib.cents, hz: vib.hertz(), from: start + (u64::from(vib.delay) * u64::from(RATE)).div_ceil(60) },
                Some(Bend::Arpeggio(offsets)) => Contour::Steps(walk(u64::MAX).into_iter().zip(offsets.iter().cycle()).map(|(t, o)| (t, written + f64::from(*o))).collect()),
                Some(Bend::Slide(from)) => {
                    let semitones = u64::from(pitch.abs_diff(*from));
                    let land = walk(semitones + 1).get(usize::try_from(semitones).unwrap_or(usize::MAX)).copied().unwrap_or(end);
                    Contour::Glide { from: f64::from(*from), to: written, start, land }
                }
            };
            strikes.push(Strike { voice, start, off: gate(start, end), source, adsr, echo, level, fade: note.ornaments.fade, contour, tone, fixed, clock });
        }
    }
    strikes.sort_by_key(|s| (s.start, s.voice));
    let chip = chip.get_or_insert_with(|| room.map_or_else(|| Chip::new(0, 0, 0), |r| Chip::new(r.delay, r.feedback, r.wet)));
    let len = at(span);
    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    let mut live: [Option<usize>; VOICES] = [None; VOICES];
    let mut next = 0;
    for t in 0..len {
        for (voice, slot) in live.iter_mut().enumerate() {
            if slot.is_some_and(|i| strikes[i].off == t) { chip.key_off(voice); *slot = None; }
        }
        while let Some(strike) = strikes.get(next).filter(|s| s.start == t) {
            if let Some(clock) = strike.clock { chip.noise_clock(clock); }
            chip.key_on(strike.voice, Key { source: strike.source, adsr: strike.adsr, pitch: strike.pitch(t), volume: strike.volume(t), echo: strike.echo });
            live[strike.voice] = Some(next);
            next += 1;
        }
        for (voice, slot) in live.iter().enumerate() {
            let Some(strike) = slot.map(|i| &strikes[i]) else { continue };
            if t % TICK == 0 || strike.contour.turns(t) {
                chip.pitch(voice, strike.pitch(t));
                chip.volume(voice, strike.volume(t));
            }
        }
        out.push(chip.sample());
    }
    Ok(out)
}

// let the chip run on with nothing new to play, and measure how long it keeps sounding above -60 dbfs (ten seconds at most)
pub fn tail(chip: &Chip) -> Tail {
    let mut chip = chip.clone();
    let (mut heard, mut peak) = (0, 0i16);
    for i in 0..u64::from(RATE) * 10 {
        let loudest = chip.sample().iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        peak = peak.max(i16::try_from(loudest).unwrap_or(i16::MAX));
        if loudest > 33 { heard = i + 1; }
        if i > heard + u64::from(RATE) / 2 { break; }
    }
    Tail { ms: heard as f64 * 1000.0 / f64::from(RATE), peak }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::BANK;
    use crate::render::Stencils;

    fn song(text: &str) -> Song { Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}")) }

    fn heard(head: &str, lanes: &str) -> Vec<[i16; 2]> {
        let song = song(&format!("song \"s\" tempo 120 profile snes{head}\npattern p bars 1 grid 4\n{lanes}"));
        press(&song, &song.patterns[0], &mut None).unwrap_or_else(|e| panic!("{e}"))
    }

    fn rises(samples: &[[i16; 2]]) -> usize { samples.windows(2).filter(|w| w[0][0] < 0 && w[1][0] >= 0).count() }

    #[test]
    fn every_playable_pitch_lands_within_half_a_register_step_of_concert() {
        for inst in BANK {
            let Kind::Tone(sample) = inst.kind else { continue };
            let bank::Tune::Root(root) = sample.tune else { continue };
            for pitch in inst.low..=inst.high {
                let concert = 440.0 * 2f64.powf((f64::from(pitch) - 69.0) / 12.0);
                let reg = inst.register(f64::from(pitch)).unwrap_or_else(|| panic!("{} {} has no register", inst.name, profile::named(pitch)));
                let off = 1200.0 * (root * f64::from(reg) / 4096.0 / concert).log2();
                let step = 1200.0 * (1.0 + 0.5 / f64::from(reg)).log2();
                assert!(off.abs() <= step + 1e-9, "{} {} sounds {off:+.2} cents off, more than the register's half step of {step:.2}", inst.name, profile::named(pitch));
            }
        }
    }

    #[test]
    fn a_held_note_sounds_its_pitch_in_the_wav() {
        // two seconds of a4 on the ocarina, nearly a sine: 880 upward zero crossings, give or take the attack
        let out = heard("", "lane voice1 inst ocarina\n  1: A4 x4\n");
        assert_eq!(out.len(), 64_000);
        let crossings = rises(&out[2_000..62_000]);
        assert!((crossings as f64 - 60_000.0 / 32_000.0 * 440.0).abs() <= 2.0, "{crossings} crossings in 1.875 s");
    }

    #[test]
    fn a_rest_is_silent_to_the_sample_once_the_release_is_spent() {
        let out = heard("", "lane voice1 inst organ\n  1: C4 x1\n  2: - x1\n  3: E4 x1\n");
        let quarter = 16_000;
        assert!(out[..quarter].iter().any(|s| s[0] != 0));
        assert!(out[quarter..2 * quarter].iter().all(|s| *s == [0, 0]), "the key-off lands one release early, so the rest is clean from its first sample");
        assert!(out[2 * quarter..2 * quarter + 64].iter().any(|s| s[0] != 0), "the next note speaks at once");
        assert!(out[3 * quarter..].iter().all(|s| *s == [0, 0]));
    }

    #[test]
    fn pan_moves_the_voice_between_the_speakers() {
        let level = |pan: &str| { let out = heard("", &format!("lane voice1 inst organ{pan}\n  1: A4 x4\n")); [0, 1].map(|c| out.iter().map(|s| i64::from(s[c]).pow(2)).sum::<i64>()) };
        let [l, r] = level(" pan -8");
        assert!(l > 0 && r == 0, "hard left leaves the right speaker silent: {l} {r}");
        let [l, r] = level("");
        assert_eq!(l, r, "centre is the same on both sides");
        let [l, r] = level(" pan 4");
        assert!(r > l && l > 0);
    }

    #[test]
    fn the_echo_repeats_the_voices_that_send_to_it() {
        let wet = heard(" echo 64ms feedback 0 wet 100", "lane voice1 inst organ echo on\n  1: A4 x1\n");
        let dry = heard(" echo 64ms feedback 0 wet 100", "lane voice1 inst organ\n  1: A4 x1\n");
        let quarter = 16_000;
        assert!(dry[quarter..].iter().all(|s| *s == [0, 0]), "a voice that doesn't send is dry");
        let tail: Vec<usize> = wet[quarter..].iter().enumerate().filter(|(_, s)| **s != [0, 0]).map(|(i, _)| i).collect();
        let last = *tail.last().unwrap();
        assert!(!tail.is_empty() && last <= 4 * 512 + 16, "with no feedback the echo is one repeat, 64 ms behind: rings to {last}");
    }

    #[test]
    fn a_fade_lands_on_silence_by_the_key_off() {
        let out = heard("", "lane voice1 inst organ\n  1: A4 x4 fade\n");
        let loud = |range: std::ops::Range<usize>| out[range].iter().map(|s| s[0].unsigned_abs()).max().unwrap();
        assert!(loud(1_000..4_000) > 6 * loud(56_000..60_000));
        assert!(out[64_000 - RELEASE as usize..].iter().all(|s| *s == [0, 0]));
    }

    #[test]
    fn the_kit_hears_the_letter_and_not_the_octave() {
        let snare = |octave: u8| heard("", &format!("lane voice8 inst kit\n  1: D{octave} x1\n"));
        assert_eq!(snare(2), snare(6));
        assert_ne!(snare(2), heard("", "lane voice8 inst kit\n  1: C2 x1\n"));
    }

    #[test]
    fn the_chip_carries_from_play_to_play() {
        let song = song("song \"s\" tempo 120 profile snes echo 160ms\npattern p bars 1 grid 4\nlane voice1 inst organ echo on\n  1: A4 x1\n  2: - x3\npattern q bars 1 grid 4\nlane voice1 inst organ\n  1: - x4\n");
        let mut stencils = Stencils::default();
        crate::render::press(&song, &song.patterns[0], &mut stencils).unwrap();
        let after = crate::render::press(&song, &song.patterns[1], &mut stencils).unwrap().stereo.unwrap();
        let fresh = crate::render::press(&song, &song.patterns[1], &mut Stencils::default()).unwrap().stereo.unwrap();
        assert!(fresh.iter().all(|s| *s == [0, 0]));
        assert!(after[..8_000].iter().any(|s| *s != [0, 0]), "the echo from the first play rings on into the second");
    }

    #[test]
    fn the_same_score_presses_the_same_bytes() {
        let text = "lane voice1 inst flute echo on\n  1: C5 x2 vib 30ct 5hz\n  3: E5 x2 from C5\nlane voice2 inst harp pan -3\n  1: C4 x4 arp 0,4,7\nlane voice7 inst noise\n  1: C6 x1 fade\nlane voice8 inst kit\n  2: D3 x1\n";
        assert_eq!(heard(" echo 96ms", text), heard(" echo 96ms", text));
    }
}
