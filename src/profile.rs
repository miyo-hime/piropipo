use crate::score::{Bend, Lane, Note, Ornaments, Pattern, Position};
use std::f64::consts::LN_2;
use std::fmt;

const SCIENTIFIC: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub const DUTIES: [&[&str]; 4] = [&["12.5", "12"], &["25"], &["50"], &["75"]];

const KNOBS: [&str; 2] = ["vol", "volume"];

const VOLUME: u8 = 10;

const CPU_HZ: f64 = 21_477_272.727_2 / 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice { Pulse, Triangle, Noise }

impl Voice {
    pub fn takes(self) -> &'static [&'static str] {
        match self { Voice::Pulse => &["duty", "vol"], Voice::Triangle => &[], Voice::Noise => &["vol"] }
    }

    pub fn marks(self) -> &'static [&'static str] {
        match self { Voice::Pulse => &["vib", "arp", "from", "duty", "vol", "fade"], Voice::Triangle => &["vib", "arp", "from"], Voice::Noise => &["vol", "fade"] }
    }
}

pub fn width(value: &str) -> Option<u8> { (0..).zip(DUTIES).find(|(_, names)| names.contains(&value)).map(|(register, _)| register) }

pub fn level(value: &str) -> Option<u8> { value.bytes().all(|b| b.is_ascii_digit()).then(|| value.parse().ok()).flatten().filter(|v| *v <= 15) }

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

#[derive(Debug, PartialEq, Eq)]
pub struct Card { pub name: &'static str, pub parts: &'static [Part] }

pub static NES: Card = Card { name: "nes", parts: &[
    Part { name: "pulse1", channel: 'A', voice: Voice::Pulse, low: 36, high: 107, playable: "C2 through B7" },
    Part { name: "pulse2", channel: 'B', voice: Voice::Pulse, low: 36, high: 107, playable: "C2 through B7" },
    Part { name: "triangle", channel: 'C', voice: Voice::Triangle, low: 24, high: 95, playable: "C1 through B6" },
    Part { name: "noise", channel: 'D', voice: Voice::Noise, low: 0, high: u8::MAX, playable: "any pitch; its letter picks one of 12 noise periods" },
] };

pub static NES_FREE: Card = Card { name: "nes-free", parts: &[
    Part { name: "pulse1", channel: 'A', voice: Voice::Pulse, low: 33, high: 116, playable: "A1 through G#8" },
    Part { name: "pulse2", channel: 'B', voice: Voice::Pulse, low: 33, high: 116, playable: "A1 through G#8" },
    Part { name: "triangle", channel: 'C', voice: Voice::Triangle, low: 21, high: 104, playable: "A0 through G#7" },
    Part { name: "noise", channel: 'D', voice: Voice::Noise, low: 0, high: u8::MAX, playable: "any pitch; its letter picks one of 12 noise periods" },
] };

pub static CARDS: [&Card; 2] = [&NES, &NES_FREE];

pub fn card(name: Option<&str>) -> Result<&'static Card, Breach> {
    let name = name.unwrap_or(NES.name);
    CARDS.iter().copied().find(|c| c.name == name).ok_or_else(|| Breach::Profile(name.into()))
}

impl Card {
    pub fn part(&self, lane: &str) -> Result<&Part, Breach> { self.parts.iter().find(|p| p.name == lane).ok_or_else(|| Breach::Lane(lane.into())) }

    pub fn audit<'p>(&self, pattern: &'p Pattern) -> Vec<Charge<'p>> {
        let mut charges = Vec::new();
        for lane in &pattern.lanes {
            let part = match self.part(&lane.name) {
                Ok(part) => part,
                Err(breach) => { charges.push(Charge { lane, note: None, breach }); continue }
            };
            charges.extend(lane.params.iter().filter_map(|p| part.setup(std::slice::from_ref(p)).err()).map(|breach| Charge { lane, note: None, breach }));
            charges.extend(lane.notes.iter().filter_map(|note| part.plays(note).err().map(|breach| Charge { lane, note: Some(note), breach })));
        }
        charges
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setup { pub duty: Option<u8>, pub volume: u8 }

impl Part {
    pub fn setup(&self, params: &[(String, String)]) -> Result<Setup, Breach> {
        let home = Setup { duty: (self.voice == Voice::Pulse).then_some(2), volume: if self.voice == Voice::Triangle { 15 } else { VOLUME } };
        params.iter().try_fold(home, |setup, (key, value)| match (self.voice, key.as_str()) {
            (Voice::Pulse, "duty") => width(value).map(|w| Setup { duty: Some(w), ..setup }).ok_or_else(|| Breach::Duty(value.clone())),
            (Voice::Pulse | Voice::Noise, "vol") => level(value).map(|volume| Setup { volume, ..setup }).ok_or_else(|| Breach::Loudness(value.clone())),
            (Voice::Triangle, knob) if KNOBS.contains(&knob) => Err(Breach::Volume(key.clone())),
            _ => Err(Breach::Param { key: key.clone(), takes: self.voice.takes() }),
        })
    }

    pub fn plays(&self, note: &Note) -> Result<(), Breach> {
        let Some(pitch) = note.pitch else { return Ok(()) };
        if !(self.low..=self.high).contains(&pitch) { return Err(Breach::Range { at: note.at, pitch, playable: self.playable }); }
        self.adorns(pitch, &note.ornaments).map_err(|flaw| Breach::Ornament { at: note.at, flaw })
    }

    fn adorns(&self, pitch: u8, ornaments: &Ornaments) -> Result<(), Flaw> {
        let Ornaments { bend, duty, vol, fade } = ornaments;
        let unheard = match self.voice {
            Voice::Noise => bend.as_ref().map(|b| Flaw::Hiss(b.word())),
            Voice::Triangle if vol.is_some() => Some(Flaw::Mute("vol")),
            Voice::Triangle if *fade => Some(Flaw::Mute("fade")),
            _ => None,
        };
        if let Some(flaw) = unheard { return Err(flaw); }
        if duty.is_some() && self.voice != Voice::Pulse { return Err(Flaw::Unfit { word: "duty", takes: self.voice.marks() }); }
        let reach = |landed: i16| (i16::from(self.low)..=i16::from(self.high)).contains(&landed);
        match bend {
            Some(Bend::Vibrato(vib)) => self.shakes(pitch, vib.cents),
            Some(Bend::Arpeggio(offsets)) => match offsets.iter().find(|o| !reach(i16::from(pitch) + **o)) {
                Some(&offset) => Err(Flaw::Reach { offset: Some(offset), pitch, landed: i16::from(pitch) + offset, playable: self.playable }),
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Charge<'p> { pub lane: &'p Lane, pub note: Option<&'p Note>, pub breach: Breach }

#[derive(Debug, Clone, PartialEq)]
pub enum Breach {
    Profile(String),
    Lane(String),
    Param { key: String, takes: &'static [&'static str] },
    Volume(String),
    Duty(String),
    Loudness(String),
    Range { at: Position, pitch: u8, playable: &'static str },
    Ornament { at: Position, flaw: Flaw },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Flaw {
    Hiss(&'static str),
    Mute(&'static str),
    Unfit { word: &'static str, takes: &'static [&'static str] },
    Shallow { pitch: u8, asked: f64, least: f64 },
    Headroom { pitch: u8, ticks: f64, register: f64 },
    Deep { pitch: u8, ticks: f64 },
    Swing { pitch: u8, ticks: f64, up: f64, down: f64, edge: u8, playable: &'static str },
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
            Flaw::Mute(_) => "no-volume-knob",
            Flaw::Unfit { .. } => "effect-off-lane",
            Flaw::Shallow { .. } => "vibrato-too-shallow",
            Flaw::Headroom { .. } => "vibrato-headroom",
            Flaw::Deep { .. } => "vibrato-too-deep",
            Flaw::Swing { .. } => "vibrato-out-of-range",
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
            Flaw::Mute(word) => write!(f, "{}", Breach::Volume((*word).into())),
            Flaw::Unfit { word, takes } => write!(f, "doesn't take `{word}` on its notes; this lane's notes take {}", roster(takes.iter().copied(), "and")),
            Flaw::Shallow { pitch, asked, least } => write!(f, "`vib {asked}ct` on {} rounds to 0 register ticks and would never move; ask for at least {:.1}ct there", named(*pitch), (least * 10.0).ceil() / 10.0),
            Flaw::Headroom { pitch, ticks, register } => write!(f, "a {ticks}-tick vibrato on {} would drive the chip's period register (sitting at {register:.1}) through zero, where the pitch runs off to infinity; keep it at {} ticks or fewer", named(*pitch), register.ceil() - 1.0),
            Flaw::Deep { pitch, ticks } => write!(f, "a vibrato this deep needs {ticks} register ticks on {}, past the chip's 255; the deepest there is about {:.0}ct", named(*pitch), cents(*pitch, 255.0)),
            Flaw::Swing { pitch, ticks, up, down, edge, playable } => write!(f, "the vibrato on {} rounds to {ticks} register tick{} and really swings +{up:.1}/-{down:.1} cents, past {}; this lane plays {playable}", named(*pitch), if *ticks == 1.0 { "" } else { "s" }, named(*edge)),
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

impl fmt::Display for Breach {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Breach::Profile(name) => write!(f, "`{name}` isn't a profile; the profiles are {}", roster(CARDS.iter().map(|c| c.name), "and")),
            // ※ every card seats the nes four, so the nes card names them; a card with lanes of its own needs its name carried in here
            Breach::Lane(lane) => write!(f, "`{lane}` isn't an nes lane; the lanes are {}", roster(NES.parts.iter().map(|p| p.name), "and")),
            Breach::Param { key, takes: [] } => write!(f, "doesn't take `{key}`; it takes no parameters"),
            Breach::Param { key, takes } => write!(f, "doesn't take `{key}`; it takes {}", roster(takes.iter().copied(), "and")),
            Breach::Volume(key) => write!(f, "doesn't take `{key}`; the triangle has no volume, it's either on or off"),
            Breach::Duty(value) => write!(f, "duty `{value}` isn't a pulse width the chip has; pick {}", roster(DUTIES.iter().map(|names| names[0]), "or")),
            Breach::Loudness(value) => write!(f, "vol `{value}` isn't a volume the chip has; pick a whole number from 0 to 15"),
            Breach::Range { pitch, playable, .. } => write!(f, "{} is out of range; this lane plays {playable}", named(*pitch)),
            Breach::Ornament { flaw, .. } => write!(f, "{flaw}"),
        }
    }
}

impl std::error::Error for Breach {}

pub fn named(pitch: impl Into<i16>) -> String {
    let pitch = pitch.into();
    format!("{}{}", SCIENTIFIC[usize::from(pitch.rem_euclid(12).unsigned_abs())], pitch.div_euclid(12) - 1)
}

fn roster<'a>(names: impl Iterator<Item = &'a str>, conjunction: &str) -> String {
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
        for card in CARDS {
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
        assert_eq!(breach.to_string(), "`gameboy` isn't a profile; the profiles are `nes` and `nes-free`");
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
        assert_eq!(setup("pulse1", &[]), Ok(Setup { duty: Some(2), volume: 10 }));
        assert_eq!(setup("pulse1", &[("vol", "6"), ("duty", "25")]), Ok(Setup { duty: Some(1), volume: 6 }));
        assert_eq!(setup("noise", &[("vol", "0")]), Ok(Setup { duty: None, volume: 0 }));
        assert_eq!(setup("triangle", &[]), Ok(Setup { duty: None, volume: 15 }));
        for junk in ["16", "-1", "+3", "3.5", "loud", ""] { assert_eq!(setup("pulse2", &[("vol", junk)]), Err(Breach::Loudness(junk.into())), "{junk:?} should not read"); }
        assert_eq!(Breach::Loudness("16".into()).to_string(), "vol `16` isn't a volume the chip has; pick a whole number from 0 to 15");
        assert_eq!(setup("noise", &[("duty", "50")]).unwrap_err().to_string(), "doesn't take `duty`; it takes `vol`");
        assert_eq!(setup("pulse1", &[("volume", "5")]).unwrap_err().to_string(), "doesn't take `volume`; it takes `duty` and `vol`");
    }

    #[test]
    fn strange_lanes_are_charged_once_without_their_notes_piling_on() {
        let song = Song::parse(&one_note("nes", "bass duty 50", "C9")).unwrap();
        let charges = NES.audit(&song.patterns[0]);
        assert_eq!(charges.iter().map(|c| (c.lane.name.as_str(), c.note, &c.breach)).collect::<Vec<_>>(), [("bass", None, &Breach::Lane("bass".into()))]);
        assert_eq!(charges[0].breach.to_string(), "`bass` isn't an nes lane; the lanes are `pulse1`, `pulse2`, `triangle`, and `noise`");
    }
}
