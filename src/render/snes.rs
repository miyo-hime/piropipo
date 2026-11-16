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
