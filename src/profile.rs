use crate::score::{Lane, Note, Pattern, Position};
use std::fmt;

const SCIENTIFIC: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

pub const DUTIES: [&[&str]; 4] = [&["12.5", "12"], &["25"], &["50"], &["75"]];

const KNOBS: [&str; 2] = ["vol", "volume"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice { Pulse, Triangle, Noise }

impl Voice {
    pub fn takes(self) -> &'static [&'static str] { if self == Voice::Pulse { &["duty"] } else { &[] } }
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
            charges.extend(lane.params.iter().filter_map(|p| part.duty(std::slice::from_ref(p)).err()).map(|breach| Charge { lane, note: None, breach }));
            charges.extend(lane.notes.iter().filter_map(|note| part.plays(note).err().map(|breach| Charge { lane, note: Some(note), breach })));
        }
        charges
    }
}

impl Part {
    pub fn duty(&self, params: &[(String, String)]) -> Result<Option<u8>, Breach> {
        params.iter().try_fold((self.voice == Voice::Pulse).then_some(2), |_, (key, value)| match (self.voice, key.as_str()) {
            (Voice::Pulse, "duty") => (0..).zip(DUTIES).find(|(_, names)| names.contains(&value.as_str())).map(|(register, _)| Some(register)).ok_or_else(|| Breach::Duty(value.clone())),
            (Voice::Triangle, knob) if KNOBS.contains(&knob) => Err(Breach::Volume(key.clone())),
            _ => Err(Breach::Param { key: key.clone(), takes: self.voice.takes() }),
        })
    }

    pub fn plays(&self, note: &Note) -> Result<(), Breach> {
        match note.pitch {
            Some(pitch) if !(self.low..=self.high).contains(&pitch) => Err(Breach::Range { at: note.at, pitch, playable: self.playable }),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge<'p> { pub lane: &'p Lane, pub note: Option<&'p Note>, pub breach: Breach }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Breach {
    Profile(String),
    Lane(String),
    Param { key: String, takes: &'static [&'static str] },
    Volume(String),
    Duty(String),
    Range { at: Position, pitch: u8, playable: &'static str },
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
            Breach::Range { pitch, playable, .. } => write!(f, "{} is out of range; this lane plays {playable}", named(*pitch)),
        }
    }
}

impl std::error::Error for Breach {}

pub fn named(pitch: u8) -> String { format!("{}{}", SCIENTIFIC[usize::from(pitch % 12)], i32::from(pitch / 12) - 1) }

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
        let duty = |lane: &str, params: &[(&str, &str)]| NES.part(lane).unwrap().duty(&params.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>());
        assert_eq!(duty("pulse1", &[("duty", "12")]), Ok(Some(0)));
        assert_eq!(duty("pulse1", &[("duty", "12.5")]), Ok(Some(0)));
        assert_eq!(duty("pulse2", &[("duty", "75")]), Ok(Some(3)));
        assert_eq!(duty("pulse2", &[]), Ok(Some(2)));
        assert_eq!(duty("triangle", &[]), Ok(None));
        let odd = duty("pulse1", &[("duty", "33")]).unwrap_err();
        assert_eq!(odd.to_string(), "duty `33` isn't a pulse width the chip has; pick `12.5`, `25`, `50`, or `75`");
        assert_eq!(NES_FREE.part("pulse1").unwrap().duty(&[("duty".into(), "33".into())]), Err(odd));
    }

    #[test]
    fn the_triangle_has_no_volume_knob() {
        let breaches = audit(&format!("{}lane noise vol 8\nlane pulse1 vol 8\n", one_note("nes", "triangle volume 8 vol 15", "C3")));
        assert_eq!(breaches, [Breach::Volume("volume".into()), Breach::Volume("vol".into()), Breach::Param { key: "vol".into(), takes: &[] }, Breach::Param { key: "vol".into(), takes: &["duty"] }]);
        assert_eq!(breaches[0].to_string(), "doesn't take `volume`; the triangle has no volume, it's either on or off");
        assert_eq!(audit(&one_note("nes-free", "triangle vol 15", "C3")), [Breach::Volume("vol".into())]);
    }

    #[test]
    fn strange_lanes_are_charged_once_without_their_notes_piling_on() {
        let song = Song::parse(&one_note("nes", "bass duty 50", "C9")).unwrap();
        let charges = NES.audit(&song.patterns[0]);
        assert_eq!(charges.iter().map(|c| (c.lane.name.as_str(), c.note, &c.breach)).collect::<Vec<_>>(), [("bass", None, &Breach::Lane("bass".into()))]);
        assert_eq!(charges[0].breach.to_string(), "`bass` isn't an nes lane; the lanes are `pulse1`, `pulse2`, `triangle`, and `noise`");
    }
}
