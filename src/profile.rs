use crate::bank::{self, BANK, Instrument, Kind};
use crate::dsp::{self, Adsr};
use crate::score::{Bend, Lane, Note, Ornaments, Pattern, Position, Song};
use std::f64::consts::LN_2;
use std::fmt;

const SCIENTIFIC: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub const DUTIES: [&[&str]; 4] = [&["12.5", "12"], &["25"], &["50"], &["75"]];

const KNOBS: [&str; 2] = ["vol", "volume"];

const VOLUME: u8 = 10;

const CPU_HZ: f64 = 21_477_272.727_2 / 12.0;

// the chip's 64 kb of sound ram, less a flat 8 kb for the driver that plays the song and the song itself
pub const ARAM: usize = 0x10000;

pub const DRIVER: usize = 0x2000;

const FEEDBACK: i32 = 30;

const WET: i32 = 35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice { Pulse, Triangle, Noise, Sample, Kit, Hiss }

impl Voice {
    pub fn takes(self) -> &'static [&'static str] {
        match self { Voice::Pulse => &["duty", "vol"], Voice::Triangle => &[], Voice::Noise => &["vol"], Voice::Sample | Voice::Kit | Voice::Hiss => &["inst", "vol", "pan", "echo", "adsr"] }
    }

    pub fn marks(self) -> &'static [&'static str] {
        match self {
            Voice::Pulse => &["vib", "arp", "from", "duty", "vol", "fade"],
            Voice::Triangle => &["vib", "arp", "from"],
            Voice::Noise | Voice::Kit | Voice::Hiss => &["vol", "fade"],
            Voice::Sample => &["vib", "arp", "from", "vol", "fade"],
        }
    }

    // noise and drums answer to a letter, not a pitch: transposes pass them by and the harmony advisory doesn't hear them
    pub fn pitched(self) -> bool { !matches!(self, Voice::Noise | Voice::Kit | Voice::Hiss) }
}

pub fn width(value: &str) -> Option<u8> { (0..).zip(DUTIES).find(|(_, names)| names.contains(&value)).map(|(register, _)| register) }

pub fn level(value: &str) -> Option<u8> { value.bytes().all(|b| b.is_ascii_digit()).then(|| value.parse().ok()).flatten().filter(|v| *v <= 15) }

fn pan(value: &str) -> Option<i8> {
    let digits = value.strip_prefix('-').or_else(|| value.strip_prefix('+')).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) { return None; }
    value.parse().ok().filter(|p: &i8| (-8..=8).contains(p))
}

fn envelope(value: &str) -> Option<Adsr> {
    let parts: Vec<u8> = value.split(',').map(|p| p.bytes().all(|b| b.is_ascii_digit()).then(|| p.parse().ok()).flatten()).collect::<Option<_>>()?;
    let [attack, decay, sustain, release] = parts[..] else { return None };
    Some(Adsr { attack, decay, sustain, release }).filter(|a| a.valid())
}

// ffmml's lfo bends the period register, not the pitch, so a tick climbs further than it falls
pub fn register(pitch: u8) -> f64 { CPU_HZ / 16.0 / (440.0 * 2f64.powf((f64::from(pitch) - 69.0) / 12.0)) }

pub fn ticks(pitch: u8, cents: f64) -> f64 { (cents * register(pitch) * LN_2 / 1200.0).round() }

pub fn cents(pitch: u8, ticks: f64) -> f64 { ticks * 1200.0 / (register(pitch) * LN_2) }

pub fn swing(pitch: u8, ticks: f64) -> (f64, f64) {
    let r = register(pitch);
    (1200.0 * (r / (r - ticks)).log2(), 1200.0 * ((r + ticks) / r).log2())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part { pub name: &'static str, pub channel: char, pub voice: Voice, pub low: u8, pub high: u8, pub playable: &'static str }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine { Famicom, Sdsp }

#[derive(Debug, PartialEq, Eq)]
pub struct Card { pub name: &'static str, pub parts: &'static [Part], pub engine: Engine, pub rate: u32 }

pub static NES: Card = Card { name: "nes", engine: Engine::Famicom, rate: 44_100, parts: &[
    Part { name: "pulse1", channel: 'A', voice: Voice::Pulse, low: 36, high: 107, playable: "C2 through B7" },
    Part { name: "pulse2", channel: 'B', voice: Voice::Pulse, low: 36, high: 107, playable: "C2 through B7" },
    Part { name: "triangle", channel: 'C', voice: Voice::Triangle, low: 24, high: 95, playable: "C1 through B6" },
    Part { name: "noise", channel: 'D', voice: Voice::Noise, low: 0, high: u8::MAX, playable: "any pitch; its letter picks one of 12 noise periods" },
] };

pub static NES_FREE: Card = Card { name: "nes-free", engine: Engine::Famicom, rate: 44_100, parts: &[
    Part { name: "pulse1", channel: 'A', voice: Voice::Pulse, low: 33, high: 116, playable: "A1 through G#8" },
    Part { name: "pulse2", channel: 'B', voice: Voice::Pulse, low: 33, high: 116, playable: "A1 through G#8" },
    Part { name: "triangle", channel: 'C', voice: Voice::Triangle, low: 21, high: 104, playable: "A0 through G#7" },
    Part { name: "noise", channel: 'D', voice: Voice::Noise, low: 0, high: u8::MAX, playable: "any pitch; its letter picks one of 12 noise periods" },
] };

const SEAT: &str = "whatever its `inst` plays";

// eight voices, all alike: what a voice sounds like is the instrument its lane loads
pub static SNES: Card = Card { name: "snes", engine: Engine::Sdsp, rate: dsp::RATE, parts: &[
    Part { name: "voice1", channel: '1', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice2", channel: '2', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice3", channel: '3', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice4", channel: '4', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice5", channel: '5', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice6", channel: '6', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice7", channel: '7', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
    Part { name: "voice8", channel: '8', voice: Voice::Sample, low: 0, high: 127, playable: SEAT },
] };

pub static CARDS: [&Card; 3] = [&NES, &NES_FREE, &SNES];

pub fn card(name: Option<&str>) -> Result<&'static Card, Breach> {
    let name = name.unwrap_or(NES.name);
    CARDS.iter().copied().find(|c| c.name == name).ok_or_else(|| Breach::Profile(name.into()))
}

// the echo as the chip holds it: a delay in 16 ms steps, and feedback and wet as signed 8-bit registers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room { pub delay: u8, pub feedback: i8, pub wet: i8 }

impl Room {
    pub fn ms(self) -> u32 { u32::from(self.delay) * 16 }

    // the percents the song line wrote, read back off the registers
    pub fn feedback_percent(self) -> i32 { unscaled(self.feedback) }

    pub fn wet_percent(self) -> i32 { unscaled(self.wet) }

    pub fn bytes(self) -> usize { usize::from(self.delay) * dsp::ECHO_STEP_BYTES }
}

fn percent(value: &str, low: i32, high: i32) -> Option<i8> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) { return None; }
    value.parse().ok().filter(|p| (low..=high).contains(p)).map(scaled)
}

fn unscaled(register: i8) -> i32 { (i32::from(register) * 100 + i32::from(register.signum()) * 63) / 127 }

// a percent onto the chip's signed 8-bit scale, rounded half away from zero
fn scaled(p: i32) -> i8 { i8::try_from((p * 127 + p.signum() * 50) / 100).unwrap_or(if p < 0 { -127 } else { 127 }) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spend { pub items: Vec<(String, usize)>, pub total: usize }

impl Card {
    pub fn part(&self, lane: &str) -> Result<&Part, Breach> { self.parts.iter().find(|p| p.name == lane).ok_or_else(|| Breach::Lane { lane: lane.into(), card: self.name }) }

    pub fn audit<'p>(&self, pattern: &'p Pattern) -> Vec<Charge<'p>> {
        let mut charges = Vec::new();
        let mut clocks: Vec<(&'p Lane, &'p Note, u64, u64, u8)> = Vec::new();
        for lane in &pattern.lanes {
            let part = match self.part(&lane.name) {
                Ok(part) => part,
                Err(breach) => { charges.push(Charge { lane, note: None, breach }); continue }
            };
            let (good, bad): (Vec<_>, Vec<_>) = lane.params.iter().map(|p| (p, part.setup(std::slice::from_ref(p)).err())).partition(|(_, e)| e.is_none());
            charges.extend(bad.into_iter().filter_map(|(_, e)| e).map(|breach| Charge { lane, note: None, breach }));
            let good: Vec<(String, String)> = good.into_iter().map(|(p, _)| p.clone()).collect();
            let seat = match part.seat(&good) {
                Ok(seat) => seat,
                Err(breach) => { charges.push(Charge { lane, note: None, breach }); continue }
            };
            charges.extend(lane.notes.iter().filter_map(|note| seat.plays(note).err().map(|breach| Charge { lane, note: Some(note), breach })));
            if seat.voice == Voice::Hiss {
                for note in lane.notes.iter().filter(|n| seat.plays(n).is_ok()) {
                    let Some(pitch) = note.pitch else { continue };
                    let from = note.at.tick(pattern.grid);
                    clocks.push((lane, note, from, from + u64::from(note.len), clock(pitch)));
                }
            }
        }
        // the chip has one noise clock, so two noises that sound at once have to agree on it
        for (i, &(lane, note, from, to, clock)) in clocks.iter().enumerate() {
            let held = clocks[..i].iter().find(|&&(other, _, f, t, c)| other.name != lane.name && f < to && from < t && c != clock);
            if let Some(&(other, held, _, _, holds)) = held { charges.push(Charge { lane, note: Some(note), breach: Breach::Clock { at: note.at, clock, lane: other.name.clone(), held: held.at, holds } }); }
        }
        charges
    }

    pub fn room(&self, song: &Song) -> Result<Option<Room>, Breach> {
        let get = |key: &str| song.echo.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        if self.engine != Engine::Sdsp {
            return match song.echo.first() { Some((key, _)) => Err(Breach::Echoless { key: key.clone(), card: self.name }), None => Ok(None) };
        }
        let Some(delay) = get("echo") else {
            return match song.echo.first() { Some((key, _)) => Err(Breach::Dry(key.clone())), None => Ok(None) };
        };
        let ms = delay.len().checked_sub(2).and_then(|cut| delay.get(cut..).filter(|u| u.eq_ignore_ascii_case("ms")).and(delay.get(..cut)))
            .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).and_then(|n| n.parse::<u32>().ok());
        let delay = match ms {
            Some(ms) if ms % 16 == 0 && (16..=240).contains(&ms) => u8::try_from(ms / 16).unwrap_or(dsp::ECHO_STEPS),
            _ => return Err(Breach::Delay { value: delay.into(), ms }),
        };
        let feedback = match get("feedback") { Some(v) => percent(v, -100, 100).ok_or_else(|| Breach::Feedback(v.into()))?, None => scaled(FEEDBACK) };
        let wet = match get("wet") { Some(v) => percent(v, 0, 100).ok_or_else(|| Breach::Wet(v.into()))?, None => scaled(WET) };
        Ok(Some(Room { delay, feedback, wet }))
    }

    // what the played song asks the sound ram to hold: each sample once however many lanes load it, each drum the kit really strikes,
    // the echo buffer, and the driver. nes cards have no sound ram, so they spend nothing
    pub fn spend(&self, song: &Song, room: Option<Room>) -> Result<Option<Spend>, Breach> {
        if self.engine != Engine::Sdsp { return Ok(None); }
        let mut loaded: Vec<&'static bank::Sample> = Vec::new();
        for pattern in song.setlist.iter().filter_map(|c| song.pattern(&c.pattern)) {
            for lane in &pattern.lanes {
                let Some(seat) = self.part(&lane.name).ok().and_then(|p| p.seat(&lane.params).ok()) else { continue };
                let Some(inst) = seat.inst else { continue };
                let samples: Vec<&'static bank::Sample> = match inst.kind {
                    Kind::Tone(sample) => if lane.notes.iter().any(|n| n.pitch.is_some()) { vec![sample] } else { Vec::new() },
                    Kind::Kit(_) => lane.notes.iter().filter_map(|n| inst.drum(n.pitch?)).map(|d| d.sample).collect(),
                    Kind::Noise => Vec::new(),
                };
                for sample in samples { if !loaded.iter().any(|s| std::ptr::eq(*s, sample)) { loaded.push(sample); } }
            }
        }
        let mut items: Vec<(String, usize)> = loaded.iter().map(|s| (s.name.to_owned(), s.bytes())).collect();
        if let Some(room) = room { items.push((format!("echo {}ms", room.ms()), room.bytes())); }
        items.push(("driver".to_owned(), DRIVER));
        let total = items.iter().map(|(_, b)| b).sum();
        if total > ARAM { return Err(Breach::Aram { items, total }); }
        Ok(Some(Spend { items, total }))
    }
}

pub fn clock(pitch: u8) -> u8 { dsp::nearest_clock(440.0 * 2f64.powf((f64::from(pitch) - 69.0) / 12.0)) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patch { pub inst: Option<&'static Instrument>, pub pan: i8, pub echo: bool, pub adsr: Option<Adsr> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setup { pub duty: Option<u8>, pub volume: u8, pub patch: Option<Patch> }

impl Setup {
    fn patched(self, change: impl FnOnce(&mut Patch)) -> Setup {
        let mut patch = self.patch.unwrap_or(Patch { inst: None, pan: 0, echo: false, adsr: None });
        change(&mut patch);
        Setup { patch: Some(patch), ..self }
    }
}

// a lane as it will really sound: its part, and on the snes the instrument that decides its voice and range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seat { pub voice: Voice, pub low: u8, pub high: u8, pub playable: &'static str, pub setup: Setup, pub inst: Option<&'static Instrument> }

impl Part {
    pub fn setup(&self, params: &[(String, String)]) -> Result<Setup, Breach> {
        let home = Setup { duty: (self.voice == Voice::Pulse).then_some(2), volume: if self.voice == Voice::Triangle { 15 } else { VOLUME }, patch: None };
        params.iter().try_fold(home, |setup, (key, value)| match (self.voice, key.as_str()) {
            (Voice::Pulse, "duty") => width(value).map(|w| Setup { duty: Some(w), ..setup }).ok_or_else(|| Breach::Duty(value.clone())),
            (Voice::Pulse | Voice::Noise | Voice::Sample, "vol") => level(value).map(|volume| Setup { volume, ..setup }).ok_or_else(|| Breach::Loudness(value.clone())),
            (Voice::Triangle, knob) if KNOBS.contains(&knob) => Err(Breach::Volume(key.clone())),
            (Voice::Sample, "inst") => bank::instrument(value).map(|inst| setup.patched(|p| p.inst = Some(inst))).ok_or_else(|| Breach::Instrument(value.clone())),
            (Voice::Sample, "pan") => pan(value).map(|pan| setup.patched(|p| p.pan = pan)).ok_or_else(|| Breach::Pan(value.clone())),
            (Voice::Sample, "echo") => match value.as_str() { "on" => Ok(setup.patched(|p| p.echo = true)), "off" => Ok(setup.patched(|p| p.echo = false)), _ => Err(Breach::Echo(value.clone())) },
            (Voice::Sample, "adsr") => envelope(value).map(|adsr| setup.patched(|p| p.adsr = Some(adsr))).ok_or_else(|| Breach::Adsr(value.clone())),
            _ => Err(Breach::Param { key: key.clone(), takes: self.voice.takes() }),
        })
    }

    pub fn seat(&self, params: &[(String, String)]) -> Result<Seat, Breach> {
        let setup = self.setup(params)?;
        if self.voice != Voice::Sample { return Ok(Seat { voice: self.voice, low: self.low, high: self.high, playable: self.playable, setup, inst: None }); }
        let inst = setup.patch.and_then(|p| p.inst).ok_or(Breach::Unvoiced)?;
        let voice = match inst.kind { Kind::Tone(_) => Voice::Sample, Kind::Kit(_) => Voice::Kit, Kind::Noise => Voice::Hiss };
        Ok(Seat { voice, low: inst.low, high: inst.high, playable: inst.playable, setup, inst: Some(inst) })
    }

    // the nes four play the same whatever their params say, so their notes can be judged without them
    pub fn plays(&self, note: &Note) -> Result<(), Breach> { self.seat(&[])?.plays(note) }
}

impl Seat {
    pub fn adsr(&self) -> Option<Adsr> { self.setup.patch.and_then(|p| p.adsr).or(self.inst.map(|i| i.adsr)) }

    pub fn plays(&self, note: &Note) -> Result<(), Breach> {
        let Some(pitch) = note.pitch else { return Ok(()) };
        if !(self.low..=self.high).contains(&pitch) { return Err(Breach::Range { at: note.at, pitch, playable: self.playable }); }
        if self.voice == Voice::Kit && self.inst.and_then(|i| i.drum(pitch)).is_none() { return Err(Breach::Drumless { at: note.at, pitch }); }
        self.adorns(pitch, &note.ornaments).map_err(|flaw| Breach::Ornament { at: note.at, flaw })
    }

    fn adorns(&self, pitch: u8, ornaments: &Ornaments) -> Result<(), Flaw> {
        let Ornaments { bend, duty, vol, fade } = ornaments;
        let unheard = match self.voice {
            Voice::Noise | Voice::Hiss => bend.as_ref().map(|b| Flaw::Hiss(b.word())),
            Voice::Kit => bend.as_ref().map(|b| Flaw::Drummed(b.word())),
            Voice::Triangle if vol.is_some() => Some(Flaw::Mute("vol")),
            Voice::Triangle if *fade => Some(Flaw::Mute("fade")),
            _ => None,
        };
        if let Some(flaw) = unheard { return Err(flaw); }
        if duty.is_some() && self.voice != Voice::Pulse { return Err(Flaw::Unfit { word: "duty", takes: self.voice.marks() }); }
        let reach = |landed: i16| (i16::from(self.low)..=i16::from(self.high)).contains(&landed);
        match bend {
            Some(Bend::Vibrato(vib)) if self.voice == Voice::Sample => self.sways(pitch, vib.cents),
            Some(Bend::Vibrato(vib)) => self.shakes(pitch, vib.cents),
            Some(Bend::Arpeggio(offsets)) => match offsets.iter().find(|o| !reach(i16::from(pitch).saturating_add(**o))) {
                Some(&offset) => Err(Flaw::Reach { offset: Some(offset), pitch, landed: i16::from(pitch).saturating_add(offset), playable: self.playable }),
                None => Ok(()),
            },
            Some(Bend::Slide(from)) if !reach(i16::from(*from)) => Err(Flaw::Reach { offset: None, pitch, landed: i16::from(*from), playable: self.playable }),
            _ => Ok(()),
        }
    }

    fn shakes(&self, pitch: u8, asked: f64) -> Result<(), Flaw> {
        let ticks = ticks(pitch, asked);
        let r = register(pitch);
        if ticks == 0.0 { return Err(Flaw::Shallow { pitch, asked, least: cents(pitch, 0.5) }); }
        if ticks >= r { return Err(Flaw::Headroom { pitch, ticks, register: r }); }
        if ticks > 255.0 { return Err(Flaw::Deep { pitch, ticks }); }
        let (up, down) = swing(pitch, ticks);
        let crest = f64::from(pitch) + up / 100.0;
        let trough = f64::from(pitch) - down / 100.0;
        let edge = if crest > f64::from(self.high) { Some(self.high) } else if trough < f64::from(self.low) { Some(self.low) } else { None };
        match edge {
            Some(edge) => Err(Flaw::Swing { pitch, ticks, up, down, edge, playable: self.playable }),
            None => Ok(()),
        }
    }

    // the snes pitch register has 14 bits, so a vibrato swings the cents it asks for, both ways alike
    fn sways(&self, pitch: u8, asked: f64) -> Result<(), Flaw> {
        let edge = if f64::from(pitch) + asked / 100.0 > f64::from(self.high) { Some(self.high) } else if f64::from(pitch) - asked / 100.0 < f64::from(self.low) { Some(self.low) } else { None };
        edge.map_or(Ok(()), |edge| Err(Flaw::Sway { pitch, cents: asked, edge, playable: self.playable }))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Charge<'p> { pub lane: &'p Lane, pub note: Option<&'p Note>, pub breach: Breach }

#[derive(Debug, Clone, PartialEq)]
pub enum Breach {
    Profile(String),
    Lane { lane: String, card: &'static str },
    Param { key: String, takes: &'static [&'static str] },
    Volume(String),
    Duty(String),
    Loudness(String),
    Range { at: Position, pitch: u8, playable: &'static str },
    Ornament { at: Position, flaw: Flaw },
    Instrument(String),
    Unvoiced,
    Pan(String),
    Echo(String),
    Adsr(String),
    Drumless { at: Position, pitch: u8 },
    Clock { at: Position, clock: u8, lane: String, held: Position, holds: u8 },
    Echoless { key: String, card: &'static str },
    Dry(String),
    Delay { value: String, ms: Option<u32> },
    Feedback(String),
    Wet(String),
    Aram { items: Vec<(String, usize)>, total: usize },
}

impl Breach {
    pub fn code(&self) -> &'static str {
        match self {
            Breach::Profile(_) => "unknown-profile",
            Breach::Lane { .. } => "unknown-lane",
            Breach::Param { .. } => "unknown-param",
            Breach::Volume(_) => "no-volume-knob",
            Breach::Duty(_) => "bad-duty",
            Breach::Loudness(_) => "bad-volume",
            Breach::Range { .. } => "out-of-range",
            Breach::Ornament { flaw, .. } => flaw.code(),
            Breach::Instrument(_) => "unknown-instrument",
            Breach::Unvoiced => "no-instrument",
            Breach::Pan(_) => "bad-pan",
            Breach::Echo(_) => "bad-echo-switch",
            Breach::Adsr(_) => "bad-adsr",
            Breach::Drumless { .. } => "no-drum",
            Breach::Clock { .. } => "noise-clock-clash",
            Breach::Echoless { .. } => "no-echo",
            Breach::Dry(_) => "echo-missing",
            Breach::Delay { .. } => "bad-echo",
            Breach::Feedback(_) => "bad-feedback",
            Breach::Wet(_) => "bad-wet",
            Breach::Aram { .. } => "aram-overflow",
        }
    }

    pub fn at(&self) -> Option<Position> {
        match self { Breach::Range { at, .. } | Breach::Ornament { at, .. } | Breach::Drumless { at, .. } | Breach::Clock { at, .. } => Some(*at), _ => None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Flaw {
    Hiss(&'static str),
    Drummed(&'static str),
    Mute(&'static str),
    Unfit { word: &'static str, takes: &'static [&'static str] },
    Shallow { pitch: u8, asked: f64, least: f64 },
    Headroom { pitch: u8, ticks: f64, register: f64 },
    Deep { pitch: u8, ticks: f64 },
    Swing { pitch: u8, ticks: f64, up: f64, down: f64, edge: u8, playable: &'static str },
    Sway { pitch: u8, cents: f64, edge: u8, playable: &'static str },
    Reach { offset: Option<i16>, pitch: u8, landed: i16, playable: &'static str },
    Late { frames: u8, note_ms: f64 },
    Crammed { offsets: u64, steps: u64, fits: Option<u32> },
    Pace { rate: f64, fastest: f64, fits: Option<u32> },
    Cramped { semitones: u16, dwells: u64, fits: Option<u32> },
    Blink { frames: u64, fits: Option<u32> },
    Crowded(&'static str),
}

impl Flaw {
    pub fn code(&self) -> &'static str {
        match self {
            Flaw::Hiss(_) => "noise-pitch-effect",
            Flaw::Drummed(_) => "kit-pitch-effect",
            Flaw::Mute(_) => "no-volume-knob",
            Flaw::Unfit { .. } => "effect-off-lane",
            Flaw::Shallow { .. } => "vibrato-too-shallow",
            Flaw::Headroom { .. } => "vibrato-headroom",
            Flaw::Deep { .. } => "vibrato-too-deep",
            Flaw::Swing { .. } | Flaw::Sway { .. } => "vibrato-out-of-range",
            Flaw::Reach { offset: Some(_), .. } => "arp-out-of-range",
            Flaw::Reach { offset: None, .. } => "slide-out-of-range",
            Flaw::Late { .. } => "vibrato-never-starts",
            Flaw::Crammed { .. } => "arp-too-short",
            Flaw::Pace { .. } => "arp-rate",
            Flaw::Cramped { .. } => "slide-too-short",
            Flaw::Blink { .. } => "fade-too-short",
            Flaw::Crowded(_) => "too-many-macros",
        }
    }
}

impl fmt::Display for Flaw {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let fits = |fits: &Option<u32>| fits.map_or(String::new(), |l| format!("; x{l} would fit"));
        match self {
            Flaw::Hiss("vib") => write!(f, "`vib` doesn't play on the noise lane; noise pitch is a timer setting, and wobbling it makes a different instrument, not vibrato"),
            Flaw::Hiss("arp") => write!(f, "`arp` doesn't play on the noise lane; noise pitch is a timer setting, so stepping it changes the hiss, not the chord"),
            Flaw::Hiss(word) => write!(f, "`{word}` doesn't play on the noise lane; noise pitch is a timer setting, so there's no pitch to slide between"),
            Flaw::Drummed(word) => write!(f, "`{word}` doesn't play on a kit lane; its letters pick drums, and every drum plays at the rate it was stored"),
            Flaw::Mute(word) => write!(f, "{}", Breach::Volume((*word).into())),
            Flaw::Unfit { word, takes } => write!(f, "doesn't take `{word}` on its notes; this lane's notes take {}", roster(takes.iter().copied(), "and")),
            Flaw::Shallow { pitch, asked, least } => write!(f, "`vib {asked}ct` on {} rounds to 0 register ticks and would never move; ask for at least {:.1}ct there", named(*pitch), (least * 10.0).ceil() / 10.0),
            Flaw::Headroom { pitch, ticks, register } => write!(f, "a {ticks}-tick vibrato on {} would drive the chip's period register (sitting at {register:.1}) through zero, where the pitch runs off to infinity; keep it at {} ticks or fewer", named(*pitch), register.ceil() - 1.0),
            Flaw::Deep { pitch, ticks } => write!(f, "a vibrato this deep needs {ticks} register ticks on {}, past the chip's 255; the deepest there is about {:.0}ct", named(*pitch), cents(*pitch, 255.0)),
            Flaw::Swing { pitch, ticks, up, down, edge, playable } => write!(f, "the vibrato on {} rounds to {ticks} register tick{} and really swings +{up:.1}/-{down:.1} cents, past {}; this lane plays {playable}", named(*pitch), if *ticks == 1.0 { "" } else { "s" }, named(*edge)),
            Flaw::Sway { pitch, cents, edge, playable } => write!(f, "the vibrato on {} swings {cents}ct each way, past {}; this lane plays {playable}", named(*pitch), named(*edge)),
            Flaw::Reach { offset: Some(offset), pitch, landed, playable } => write!(f, "the arp's {offset:+} lands on {} from {}, out of range; this lane plays {playable}", named(*landed), named(*pitch)),
            Flaw::Reach { offset: None, landed, playable, .. } => write!(f, "the slide departs from {}, out of range; this lane plays {playable}", named(*landed)),
            Flaw::Late { frames, note_ms } => write!(f, "the vibrato's delay ({frames} frames, {:.0} ms) meets or outlasts the note's {note_ms:.1} ms, so it would never start; shorten the delay or lengthen the note", f64::from(*frames) * 1000.0 / 60.0),
            Flaw::Crammed { offsets, steps, fits: l } => write!(f, "this note fits {steps} arp step{} at about 60 a second, too few to sound all {offsets} offsets{}", if *steps == 1 { "" } else { "s" }, fits(l)),
            Flaw::Pace { rate, fastest, .. } if *fastest < 45.0 => write!(f, "the arp steps {rate:.1} times a second, outside 45 to 75 (60, give or take 25%); at this tempo and grid the fastest step is {fastest:.1} a second, so it writes no arps"),
            Flaw::Pace { rate, fits: l, .. } => write!(f, "the arp steps {rate:.1} times a second, outside 45 to 75 (60, give or take 25%){}", fits(l)),
            Flaw::Cramped { semitones, dwells, fits: l } => write!(f, "a {semitones}-semitone slide needs {} dwells at about 60 a second, the target's hold included, and this note only fits {dwells}{}", semitones + 1, fits(l)),
            Flaw::Blink { frames, fits: l } => write!(f, "a fade needs at least 2 frames (a frame is 1/60 s) to reach 0, and this note spans {frames}{}", fits(l)),
            Flaw::Crowded(kind) => write!(f, "this note needs a 129th distinct {kind} macro, past the 128 a song can hold; reuse the settings of an earlier {kind}"),
        }
    }
}

fn kb(bytes: usize) -> String { format!("{:.1} kb", bytes as f64 / 1024.0) }

impl fmt::Display for Breach {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let instruments = || roster(BANK.iter().map(|i| i.name), "and");
        match self {
            Breach::Profile(name) => write!(f, "`{name}` isn't a profile; the profiles are {}", roster(CARDS.iter().map(|c| c.name), "and")),
            Breach::Lane { lane, card: name } => {
                let lanes = card(Some(name)).map_or(String::new(), |c| roster(c.parts.iter().map(|p| p.name), "and"));
                write!(f, "`{lane}` isn't an {name} lane; the lanes are {lanes}")
            }
            Breach::Param { key, takes: [] } => write!(f, "doesn't take `{key}`; it takes no parameters"),
            Breach::Param { key, takes } => write!(f, "doesn't take `{key}`; it takes {}", roster(takes.iter().copied(), "and")),
            Breach::Volume(key) => write!(f, "doesn't take `{key}`; the triangle has no volume, it's either on or off"),
            Breach::Duty(value) => write!(f, "duty `{value}` isn't a pulse width the chip has; pick {}", roster(DUTIES.iter().map(|names| names[0]), "or")),
            Breach::Loudness(value) => write!(f, "vol `{value}` isn't a volume the chip has; pick a whole number from 0 to 15"),
            Breach::Range { pitch, playable, .. } => write!(f, "{} is out of range; this lane plays {playable}", named(*pitch)),
            Breach::Ornament { flaw, .. } => write!(f, "{flaw}"),
            Breach::Instrument(name) => write!(f, "`{name}` isn't in the snes bank; it holds {}", instruments()),
            Breach::Unvoiced => write!(f, "a snes lane plays nothing until it loads an instrument; add `inst` and one of {}", instruments()),
            Breach::Pan(value) => write!(f, "pan `{value}` isn't a place the voice can sit; pick a whole number from -8 (left) through 0 (centre) to 8 (right)"),
            Breach::Echo(value) => write!(f, "echo `{value}` is a switch; it's either `on` or `off`"),
            Breach::Adsr(value) => write!(f, "adsr `{value}` isn't an envelope the chip has; write attack,decay,sustain,release as whole numbers up to 15,7,7,31, like `15,7,7,0`"),
            Breach::Drumless { pitch, .. } => write!(f, "the kit has no drum on {}; its letters are {}", named(*pitch).trim_end_matches(|c: char| c.is_ascii_digit() || c == '-'), bank::letters()),
            Breach::Clock { clock, lane, held, holds, .. } => write!(f, "this noise wants the {} clock while lane {lane}'s noise at {held} holds the {} one; the chip has one noise clock, so noises that sound at once have to share it", hz(dsp::clock_hz(*clock)), hz(dsp::clock_hz(*holds))),
            Breach::Echoless { key, card } => write!(f, "the {card} card has no echo, so `{key}` has nothing to set; the echo belongs to `profile snes`"),
            Breach::Dry(key) => write!(f, "`{key}` sets up an echo the song never asks for; add `echo` with a delay, like `echo 160ms`"),
            Breach::Delay { value, ms: Some(ms) } if (1..=255).contains(ms) && ms % 16 != 0 => write!(f, "echo `{value}` isn't a delay the chip has; it steps in 16 ms, so {}ms or {}ms", (ms / 16 * 16).max(16), (ms / 16 * 16 + 16).min(240)),
            Breach::Delay { value, .. } => write!(f, "echo `{value}` isn't a delay the chip has; write 16ms to 240ms in steps of 16, like `echo 160ms`"),
            Breach::Feedback(value) => write!(f, "feedback `{value}` isn't one the chip has; pick a whole percent from -100 to 100"),
            Breach::Wet(value) => write!(f, "wet `{value}` isn't one the chip has; pick a whole percent from 0 to 100"),
            Breach::Aram { items, total } => write!(f, "the song needs {} of the chip's 64 kb of sound ram: {}; drop an instrument or a drum, or shorten the echo (each 16 ms of it costs 2 kb)", kb(*total), items.iter().map(|(name, bytes)| format!("{name} {}", kb(*bytes))).collect::<Vec<_>>().join(", ")),
        }
    }
}

fn hz(hz: f64) -> String { if hz >= 1000.0 { format!("{:.1} khz", hz / 1000.0) } else if hz.fract() == 0.0 { format!("{hz:.0} hz") } else { format!("{hz:.1} hz") } }

impl std::error::Error for Breach {}

pub fn named(pitch: impl Into<i16>) -> String {
    let pitch = pitch.into();
    format!("{}{}", SCIENTIFIC[usize::from(pitch.rem_euclid(12).unsigned_abs())], pitch.div_euclid(12) - 1)
}

pub(crate) fn roster<'a>(names: impl Iterator<Item = &'a str>, conjunction: &str) -> String {
    let names: Vec<String> = names.map(|n| format!("`{n}`")).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} {conjunction} {second}"),
        [rest @ .., last] => format!("{}, {conjunction} {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Song;

    fn audit(text: &str) -> Vec<Breach> {
        let song = Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}"));
        let card = card(song.profile.as_deref()).unwrap_or_else(|breach| panic!("{breach}"));
        song.patterns.iter().flat_map(|p| card.audit(p)).map(|c| c.breach).collect()
    }

    fn one_note(profile: &str, lane: &str, pitch: &str) -> String { format!("song \"s\" tempo 90 profile {profile}\npattern p bars 1 grid 4\nlane {lane}\n  1: {pitch} x1\n") }

    #[test]
    fn nes_refuses_what_the_chip_leash_forbids_by_name() {
        let [breach] = &audit(&one_note("nes", "pulse1", "B1"))[..] else { panic!("expected one breach") };
        assert_eq!(*breach, Breach::Range { at: Position { bar: 1, slot: 1 }, pitch: 35, playable: "C2 through B7" });
        assert_eq!(breach.to_string(), "B1 is out of range; this lane plays C2 through B7");
        let charges = audit(&format!("{}  2: C8 x1\nlane triangle\n  1: C7 x1\n  2: B6 x1\n", one_note("nes", "pulse2", "C2")));
        assert_eq!(charges.iter().map(ToString::to_string).collect::<Vec<_>>(), ["C8 is out of range; this lane plays C2 through B7", "C7 is out of range; this lane plays C1 through B6"]);
    }

    #[test]
    fn nes_free_takes_everything_the_engine_plays_and_nothing_past_it() {
        for (lane, low, high, under, over) in [("pulse1", "A1", "G#8", "G#1", "A8"), ("triangle", "A0", "G#7", "G#0", "A7")] {
            assert_eq!(audit(&format!("{}  2: {high} x1\n", one_note("nes-free", lane, low))), Vec::new());
            assert_eq!(audit(&format!("{}  2: {low} x1\n", one_note("nes", lane, high))).len(), 2, "nes should refuse both ends of nes-free's {lane}");
            assert!(matches!(&audit(&format!("{}  2: {over} x1\n", one_note("nes-free", lane, under)))[..], [Breach::Range { .. }, Breach::Range { .. }]));
        }
        assert_eq!(audit(&one_note("nes-free", "noise", "B9")), Vec::new());
        assert_eq!(audit(&one_note("nes", "noise", "C0")), Vec::new());
    }

    #[test]
    fn every_card_says_the_range_it_enforces() {
        for card in CARDS.iter().filter(|c| c.engine == Engine::Famicom) {
            for part in card.parts.iter().filter(|p| p.voice != Voice::Noise) { assert_eq!(part.playable, format!("{} through {}", named(part.low), named(part.high)), "{} {}", card.name, part.name); }
            assert_eq!(card.parts.iter().map(|p| (p.name, p.channel, p.voice)).collect::<Vec<_>>(), NES.parts.iter().map(|p| (p.name, p.channel, p.voice)).collect::<Vec<_>>(), "{} strays from the nes voice bank", card.name);
            for (nes, part) in NES.parts.iter().zip(card.parts) { assert!(part.low <= nes.low && nes.high <= part.high, "{} {} is narrower than nes", card.name, part.name); }
        }
    }

    #[test]
    fn an_unknown_profile_names_the_ones_that_exist() {
        assert_eq!(card(None), Ok(&NES));
        assert_eq!(card(Some("nes-free")), Ok(&NES_FREE));
        let breach = card(Some("gameboy")).unwrap_err();
        assert_eq!(breach, Breach::Profile("gameboy".into()));
        assert_eq!(breach.to_string(), "`gameboy` isn't a profile; the profiles are `nes`, `nes-free`, and `snes`");
    }

    #[test]
    fn duty_reads_the_chip_register_and_twelve_means_twelve_and_a_half() {
        let duty = |lane: &str, params: &[(&str, &str)]| NES.part(lane).unwrap().setup(&params.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>()).map(|s| s.duty);
        assert_eq!(duty("pulse1", &[("duty", "12")]), Ok(Some(0)));
        assert_eq!(duty("pulse1", &[("duty", "12.5")]), Ok(Some(0)));
        assert_eq!(duty("pulse2", &[("duty", "75")]), Ok(Some(3)));
        assert_eq!(duty("pulse2", &[]), Ok(Some(2)));
        assert_eq!(duty("triangle", &[]), Ok(None));
        let odd = duty("pulse1", &[("duty", "33")]).unwrap_err();
        assert_eq!(odd.to_string(), "duty `33` isn't a pulse width the chip has; pick `12.5`, `25`, `50`, or `75`");
        assert_eq!(NES_FREE.part("pulse1").unwrap().setup(&[("duty".into(), "33".into())]), Err(odd));
    }

    #[test]
    fn the_triangle_has_no_volume_knob() {
        let breaches = audit(&format!("{}lane noise vol 8\nlane pulse1 vol 8\n", one_note("nes", "triangle volume 8 vol 15", "C3")));
        assert_eq!(breaches, [Breach::Volume("volume".into()), Breach::Volume("vol".into())]);
        assert_eq!(breaches[0].to_string(), "doesn't take `volume`; the triangle has no volume, it's either on or off");
        assert_eq!(audit(&one_note("nes-free", "triangle vol 15", "C3")), [Breach::Volume("vol".into())]);
        let notes = audit("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane triangle\n  1: C3 x1 vol 4\n  2: C3 x1 fade\n");
        assert_eq!(notes.iter().map(ToString::to_string).collect::<Vec<_>>(), ["doesn't take `vol`; the triangle has no volume, it's either on or off", "doesn't take `fade`; the triangle has no volume, it's either on or off"]);
    }

    #[test]
    fn pulses_and_noise_take_a_lane_volume() {
        let setup = |lane: &str, params: &[(&str, &str)]| NES.part(lane).unwrap().setup(&params.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>());
        assert_eq!(setup("pulse1", &[]), Ok(Setup { duty: Some(2), volume: 10, patch: None }));
        assert_eq!(setup("pulse1", &[("vol", "6"), ("duty", "25")]), Ok(Setup { duty: Some(1), volume: 6, patch: None }));
        assert_eq!(setup("noise", &[("vol", "0")]), Ok(Setup { duty: None, volume: 0, patch: None }));
        assert_eq!(setup("triangle", &[]), Ok(Setup { duty: None, volume: 15, patch: None }));
        for junk in ["16", "-1", "+3", "3.5", "loud", ""] { assert_eq!(setup("pulse2", &[("vol", junk)]), Err(Breach::Loudness(junk.into())), "{junk:?} should not read"); }
        assert_eq!(Breach::Loudness("16".into()).to_string(), "vol `16` isn't a volume the chip has; pick a whole number from 0 to 15");
        assert_eq!(setup("noise", &[("duty", "50")]).unwrap_err().to_string(), "doesn't take `duty`; it takes `vol`");
        assert_eq!(setup("pulse1", &[("volume", "5")]).unwrap_err().to_string(), "doesn't take `volume`; it takes `duty` and `vol`");
    }

    #[test]
    fn an_arp_offset_past_the_integers_is_refused_not_overflowed() {
        let charges = audit("song \"s\" tempo 120 profile nes\npattern p bars 1 grid 16\nlane pulse1\n  1: C4 x16 arp 0,32767\n");
        assert!(charges.iter().any(|b| b.to_string().starts_with("the arp's +32767 lands on")), "{charges:?}");
    }

    #[test]
    fn strange_lanes_are_charged_once_without_their_notes_piling_on() {
        let song = Song::parse(&one_note("nes", "bass duty 50", "C9")).unwrap();
        let charges = NES.audit(&song.patterns[0]);
        assert_eq!(charges.iter().map(|c| (c.lane.name.as_str(), c.note, &c.breach)).collect::<Vec<_>>(), [("bass", None, &Breach::Lane { lane: "bass".into(), card: "nes" })]);
        assert_eq!(charges[0].breach.to_string(), "`bass` isn't an nes lane; the lanes are `pulse1`, `pulse2`, `triangle`, and `noise`");
    }

    fn snes(lanes: &str) -> Vec<String> { audit(&format!("song \"s\" tempo 90 profile snes\npattern p bars 1 grid 4\n{lanes}")).iter().map(ToString::to_string).collect() }

    #[test]
    fn a_snes_lane_loads_an_instrument_from_the_bank() {
        assert_eq!(snes("lane voice1 inst flute\n  1: C5 x4\n"), Vec::<String>::new());
        assert_eq!(snes("lane voice1 vol 8\n  1: C5 x4\n"), ["a snes lane plays nothing until it loads an instrument; add `inst` and one of `flute`, `ocarina`, `harp`, `pizz`, `musicbox`, `epiano`, `strings`, `accordion`, `choir`, `organ`, `bass`, `bell`, `kit`, and `noise`"]);
        assert!(snes("lane voice2 inst kazoo\n  1: C5 x4\n")[0].starts_with("`kazoo` isn't in the snes bank; it holds `flute`, "));
        assert_eq!(snes("lane voice9 inst flute\n"), ["`voice9` isn't an snes lane; the lanes are `voice1`, `voice2`, `voice3`, `voice4`, `voice5`, `voice6`, `voice7`, and `voice8`"]);
        assert_eq!(snes("lane pulse1 duty 50\n"), ["`pulse1` isn't an snes lane; the lanes are `voice1`, `voice2`, `voice3`, `voice4`, `voice5`, `voice6`, `voice7`, and `voice8`"]);
    }

    #[test]
    fn the_instrument_draws_the_range() {
        assert_eq!(snes("lane voice1 inst flute\n  1: B3 x1\n  2: C7 x1\n  3: C#7 x1\n"), ["B3 is out of range; this lane plays C4 through C7 on the flute", "C#7 is out of range; this lane plays C4 through C7 on the flute"]);
        assert_eq!(snes("lane voice1 inst bass\n  1: C1 x1\n  2: C4 x1\n"), Vec::<String>::new());
        assert_eq!(snes("lane voice1 inst flute\n  1: C5 x4 arp 0,4,7,24\n"), Vec::<String>::new());
        assert_eq!(snes("lane voice1 inst flute\n  1: C5 x4 arp 0,4,7,25\n"), ["the arp's +25 lands on C#7 from C5, out of range; this lane plays C4 through C7 on the flute"]);
    }

    #[test]
    fn a_snes_vibrato_swings_the_cents_it_asks_for() {
        assert_eq!(snes("lane voice1 inst flute\n  1: B6 x4 vib 90ct 5hz\n"), Vec::<String>::new());
        assert_eq!(snes("lane voice1 inst flute\n  1: C7 x4 vib 10ct 5hz\n"), ["the vibrato on C7 swings 10ct each way, past C7; this lane plays C4 through C7 on the flute"]);
        assert_eq!(snes("lane voice1 inst flute\n  1: C4 x4 vib 1ct 5hz\n"), ["the vibrato on C4 swings 1ct each way, past C4; this lane plays C4 through C7 on the flute"]);
    }

    #[test]
    fn the_kit_reads_letters_and_refuses_to_bend() {
        assert_eq!(snes("lane voice8 inst kit\n  1: C2 x1\n  2: D9 x1\n  3: G#4 x1 vol 4 fade\n"), Vec::<String>::new());
        assert_eq!(snes("lane voice8 inst kit\n  1: A#4 x1\n"), ["the kit has no drum on A#; its letters are C kick, C# rim, D snare, D# brush, E frame, F tom, F# hat, G shaker, G# bells, A ting"]);
        let kit = "`{}` doesn't play on a kit lane; its letters pick drums, and every drum plays at the rate it was stored";
        assert_eq!(snes("lane voice8 inst kit\n  1: D4 x2 vib 20ct 5hz\n  3: E4 x2 from D4\n"), [kit.replace("{}", "vib"), kit.replace("{}", "from")]);
        assert_eq!(snes("lane voice7 inst noise\n  1: D4 x2 arp 0,7\n"), ["`arp` doesn't play on the noise lane; noise pitch is a timer setting, so stepping it changes the hiss, not the chord"]);
    }

    #[test]
    fn duty_has_no_place_on_a_sample_voice() {
        assert_eq!(snes("lane voice1 inst flute duty 50\n  1: C5 x4 duty 25\n"), ["doesn't take `duty`; it takes `inst`, `vol`, `pan`, `echo`, and `adsr`", "doesn't take `duty` on its notes; this lane's notes take `vib`, `arp`, `from`, `vol`, and `fade`"]);
    }

    #[test]
    fn lane_knobs_are_read_or_refused_by_name() {
        let seat = |params: &[(&str, &str)]| SNES.parts[0].seat(&params.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>());
        let harp = seat(&[("inst", "harp"), ("pan", "-3"), ("echo", "on"), ("adsr", "15,4,5,12"), ("vol", "9")]).unwrap();
        assert_eq!((harp.voice, harp.setup.volume, harp.setup.patch.map(|p| (p.pan, p.echo))), (Voice::Sample, 9, Some((-3, true))));
        assert_eq!(harp.adsr(), Some(Adsr { attack: 15, decay: 4, sustain: 5, release: 12 }));
        assert_eq!(seat(&[("inst", "harp")]).unwrap().adsr(), Some(bank::I_HARP.adsr), "without `adsr` the instrument keeps its own envelope");
        assert_eq!(seat(&[("inst", "kit")]).unwrap().voice, Voice::Kit);
        assert_eq!(seat(&[("inst", "noise")]).unwrap().voice, Voice::Hiss);
        for (key, value) in [("pan", "9"), ("pan", "-9"), ("pan", "left"), ("echo", "yes"), ("adsr", "16,0,0,0"), ("adsr", "15,8,7,0"), ("adsr", "15,7,7"), ("vol", "16")] {
            assert!(seat(&[("inst", "harp"), (key, value)]).is_err(), "{key} {value} should not read");
        }
        assert_eq!(Breach::Pan("9".into()).to_string(), "pan `9` isn't a place the voice can sit; pick a whole number from -8 (left) through 0 (centre) to 8 (right)");
        assert_eq!(Breach::Adsr("16,0,0,0".into()).to_string(), "adsr `16,0,0,0` isn't an envelope the chip has; write attack,decay,sustain,release as whole numbers up to 15,7,7,31, like `15,7,7,0`");
    }

    fn room(head: &str) -> Result<Option<Room>, Breach> {
        let song = Song::parse(&format!("{head}\npattern p bars 1 grid 4\n")).unwrap_or_else(|e| panic!("{e:?}"));
        card(song.profile.as_deref()).unwrap().room(&song)
    }

    #[test]
    fn the_echo_steps_in_sixteen_milliseconds() {
        assert_eq!(room("song \"s\" tempo 90 profile snes"), Ok(None));
        assert_eq!(room("song \"s\" tempo 90 profile snes echo 160ms"), Ok(Some(Room { delay: 10, feedback: 38, wet: 44 })));
        assert_eq!(room("song \"s\" tempo 90 profile snes echo 240MS feedback -100 wet 0"), Ok(Some(Room { delay: 15, feedback: -127, wet: 0 })));
        assert_eq!(room("song \"s\" tempo 90 profile snes echo 150ms").unwrap_err().to_string(), "echo `150ms` isn't a delay the chip has; it steps in 16 ms, so 144ms or 160ms");
        assert_eq!(room("song \"s\" tempo 90 profile snes echo 300ms").unwrap_err().to_string(), "echo `300ms` isn't a delay the chip has; write 16ms to 240ms in steps of 16, like `echo 160ms`");
        assert_eq!(room("song \"s\" tempo 90 profile snes echo 0ms").unwrap_err().code(), "bad-echo");
        assert_eq!(room("song \"s\" tempo 90 profile snes feedback 40").unwrap_err().to_string(), "`feedback` sets up an echo the song never asks for; add `echo` with a delay, like `echo 160ms`");
        assert_eq!(room("song \"s\" tempo 90 profile snes echo 32ms wet 101").unwrap_err(), Breach::Wet("101".into()));
        assert_eq!(room("song \"s\" tempo 90 echo 160ms").unwrap_err().to_string(), "the nes card has no echo, so `echo` has nothing to set; the echo belongs to `profile snes`");
    }

    fn spend(text: &str) -> Result<Option<Spend>, Breach> {
        let song = Song::parse(text).unwrap_or_else(|e| panic!("{e:?}"));
        let card = card(song.profile.as_deref()).unwrap();
        card.spend(&song, card.room(&song).unwrap())
    }

    #[test]
    fn the_sound_ram_holds_what_the_song_plays() {
        let text = "song \"s\" tempo 90 profile snes echo 160ms\npattern a bars 1 grid 4\nlane voice1 inst harp\n  1: C5 x4\nlane voice2 inst harp\n  1: E5 x4\nlane voice8 inst kit\n  1: C3 x1\n  2: F#3 x1\n  3: C4 x1\nlane voice7 inst noise\n  1: C5 x4\npattern unplayed bars 1 grid 4\nlane voice1 inst strings\n  1: C4 x4\nplay\n  a\n";
        let Spend { items, total } = spend(text).unwrap().unwrap();
        assert_eq!(items, [("harp".to_owned(), bank::HARP.bytes()), ("kick".to_owned(), bank::KICK.bytes()), ("hat".to_owned(), bank::HAT.bytes()), ("echo 160ms".to_owned(), 20_480), ("driver".to_owned(), DRIVER)]);
        assert_eq!(total, items.iter().map(|i| i.1).sum::<usize>());
        assert_eq!(spend("song \"s\" tempo 90\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x4\n"), Ok(None));
        let greedy = "song \"s\" tempo 90 profile snes echo 240ms\npattern a bars 1 grid 4\nlane voice1 inst strings\n  1: C4 x4\nlane voice2 inst choir\n  1: C4 x4\nlane voice3 inst accordion\n  1: C4 x4\nlane voice4 inst bell\n  1: C4 x4\nlane voice5 inst pizz\n  1: C3 x4\nlane voice6 inst harp\n  1: C4 x4\n";
        let refused = spend(greedy).unwrap_err();
        assert_eq!(refused.code(), "aram-overflow");
        assert!(refused.to_string().starts_with("the song needs 65.6 kb of the chip's 64 kb of sound ram: strings 6.8 kb, choir 4.5 kb, accordion 4.5 kb, bell 4.7 kb, pizz 3.7 kb, harp 3.5 kb, echo 240ms 30.0 kb, driver 8.0 kb;"), "{refused}");
    }

    #[test]
    fn noises_that_sound_together_share_one_clock() {
        assert_eq!(snes("lane voice6 inst noise\n  1: C6 x2\nlane voice7 inst noise\n  2: C6 x2\n"), Vec::<String>::new());
        assert_eq!(snes("lane voice6 inst noise\n  1: C6 x2\nlane voice7 inst noise\n  3: C2 x2\n"), Vec::<String>::new(), "back to back is fine");
        assert_eq!(snes("lane voice6 inst noise\n  1: C6 x2\nlane voice7 inst noise\n  2: C2 x2\n"), ["this noise wants the 62.5 hz clock while lane voice6's noise at 1.1 holds the 1.0 khz one; the chip has one noise clock, so noises that sound at once have to share it"]);
    }
}
