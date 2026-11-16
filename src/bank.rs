// the snes card's instruments. every sample here is what src/recipes.rs makes, brr-coded by `cargo run --release --example bank`;
// the test at the bottom makes them all again and refuses a file that has drifted from its recipe

use crate::brr;
use crate::dsp::{Adsr, Source};
pub use crate::recipes::Tune;
use std::fmt;
use std::sync::OnceLock;

pub struct Sample { pub name: &'static str, pub brr: &'static [u8], pub restart: Option<usize>, pub tune: Tune, pcm: OnceLock<Vec<i16>> }

impl Sample {
    pub fn pcm(&'static self) -> &'static [i16] { self.pcm.get_or_init(|| brr::decode(self.brr)) }

    pub fn source(&'static self) -> Source { Source::Sample { pcm: self.pcm(), restart: self.restart } }

    pub fn bytes(&self) -> usize { self.brr.len() }
}

impl fmt::Debug for Sample {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "Sample({})", self.name) }
}

macro_rules! sample {
    ($id:ident, $name:literal, $restart:expr, $tune:expr) => {
        pub static $id: Sample = Sample { name: $name, brr: include_bytes!(concat!("bank/", $name, ".brr")), restart: $restart, tune: $tune, pcm: OnceLock::new() };
    };
}

sample!(FLUTE, "flute", Some(3_840), Tune::Root(1_062.5));
sample!(OCARINA, "ocarina", Some(1_920), Tune::Root(875.0));
sample!(HARP, "harp", Some(6_400), Tune::Root(32_000.0 / 48.0));
sample!(PIZZ, "pizz", Some(6_400), Tune::Root(32_000.0 / 288.0));
sample!(MUSICBOX, "musicbox", Some(3_200), Tune::Root(1_125.0));
sample!(EPIANO, "epiano", Some(4_800), Tune::Root(500.0));
sample!(STRINGS, "strings", Some(0), Tune::Root(375.0));
sample!(ACCORDION, "accordion", Some(0), Tune::Root(32_000.0 * 129.0 / 8_192.0));
sample!(CHOIR, "choir", Some(0), Tune::Root(375.0));
sample!(ORGAN, "organ", Some(640), Tune::Root(500.0));
sample!(BASS, "bass", Some(1_280), Tune::Root(125.0));
sample!(BELL, "bell", Some(8_000), Tune::Root(500.0));
sample!(KICK, "kick", None, Tune::Stored(16_000));
sample!(RIM, "rim", None, Tune::Stored(32_000));
sample!(SNARE, "snare", None, Tune::Stored(24_000));
sample!(BRUSH, "brush", None, Tune::Stored(32_000));
sample!(FRAME, "frame", None, Tune::Stored(16_000));
sample!(TOM, "tom", None, Tune::Stored(16_000));
sample!(HAT, "hat", None, Tune::Stored(32_000));
sample!(SHAKER, "shaker", None, Tune::Stored(32_000));
sample!(BELLS, "bells", None, Tune::Stored(32_000));
sample!(TING, "ting", Some(2_560), Tune::Stored(32_000));

pub static SAMPLES: [&Sample; 22] = [&FLUTE, &OCARINA, &HARP, &PIZZ, &MUSICBOX, &EPIANO, &STRINGS, &ACCORDION, &CHOIR, &ORGAN, &BASS, &BELL, &KICK, &RIM, &SNARE, &BRUSH, &FRAME, &TOM, &HAT, &SHAKER, &BELLS, &TING];

#[derive(Debug)]
pub struct Drum { pub name: &'static str, pub sample: &'static Sample, pub adsr: Adsr }

#[derive(Debug)]
pub enum Kind { Tone(&'static Sample), Kit(&'static [Option<Drum>; 12]), Noise }

#[derive(Debug)]
pub struct Instrument { pub name: &'static str, pub kind: Kind, pub adsr: Adsr, pub low: u8, pub high: u8, pub playable: &'static str }

impl PartialEq for Instrument {
    fn eq(&self, other: &Self) -> bool { self.name == other.name }
}

impl Eq for Instrument {}

const fn adsr(attack: u8, decay: u8, sustain: u8, release: u8) -> Adsr { Adsr { attack, decay, sustain, release } }

const STRUCK: Adsr = adsr(15, 7, 7, 0);

pub static KIT: [Option<Drum>; 12] = [
    Some(Drum { name: "kick", sample: &KICK, adsr: STRUCK }),
    Some(Drum { name: "rim", sample: &RIM, adsr: STRUCK }),
    Some(Drum { name: "snare", sample: &SNARE, adsr: STRUCK }),
    Some(Drum { name: "brush", sample: &BRUSH, adsr: STRUCK }),
    Some(Drum { name: "frame", sample: &FRAME, adsr: STRUCK }),
    Some(Drum { name: "tom", sample: &TOM, adsr: STRUCK }),
    Some(Drum { name: "hat", sample: &HAT, adsr: STRUCK }),
    Some(Drum { name: "shaker", sample: &SHAKER, adsr: STRUCK }),
    Some(Drum { name: "bells", sample: &BELLS, adsr: STRUCK }),
    Some(Drum { name: "ting", sample: &TING, adsr: adsr(15, 7, 7, 15) }),
    None,
    None,
];

macro_rules! tone {
    ($id:ident, $name:literal, $sample:ident, $adsr:expr, $low:literal, $high:literal, $playable:literal) => {
        pub static $id: Instrument = Instrument { name: $name, kind: Kind::Tone(&$sample), adsr: $adsr, low: $low, high: $high, playable: $playable };
    };
}

tone!(I_FLUTE, "flute", FLUTE, adsr(12, 7, 7, 0), 60, 96, "C4 through C7 on the flute");
tone!(I_OCARINA, "ocarina", OCARINA, adsr(11, 7, 7, 0), 60, 96, "C4 through C7 on the ocarina");
tone!(I_HARP, "harp", HARP, adsr(15, 7, 7, 14), 48, 96, "C3 through C7 on the harp");
tone!(I_PIZZ, "pizz", PIZZ, adsr(15, 7, 7, 19), 28, 64, "E1 through E4 on the pizz");
tone!(I_MUSICBOX, "musicbox", MUSICBOX, adsr(15, 7, 7, 13), 60, 108, "C4 through C8 on the musicbox");
tone!(I_EPIANO, "epiano", EPIANO, adsr(15, 3, 5, 13), 48, 95, "C3 through B6 on the epiano");
tone!(I_STRINGS, "strings", STRINGS, adsr(7, 7, 7, 0), 43, 90, "G2 through F#6 on the strings");
tone!(I_ACCORDION, "accordion", ACCORDION, adsr(10, 7, 7, 0), 48, 95, "C3 through B6 on the accordion");
tone!(I_CHOIR, "choir", CHOIR, adsr(6, 7, 7, 0), 48, 84, "C3 through C6 on the choir");
tone!(I_ORGAN, "organ", ORGAN, adsr(12, 7, 7, 0), 36, 95, "C2 through B6 on the organ");
tone!(I_BASS, "bass", BASS, adsr(15, 2, 5, 11), 24, 60, "C1 through C4 on the bass");
tone!(I_BELL, "bell", BELL, adsr(15, 7, 7, 10), 43, 84, "G2 through C6 on the bell");

pub static I_KIT: Instrument = Instrument { name: "kit", kind: Kind::Kit(&KIT), adsr: STRUCK, low: 0, high: 127, playable: "any pitch; its letter picks the drum" };

pub static I_NOISE: Instrument = Instrument { name: "noise", kind: Kind::Noise, adsr: adsr(15, 7, 7, 0), low: 0, high: 127, playable: "any pitch; it picks the nearest of the chip's 31 noise clocks" };

pub static BANK: [&Instrument; 14] = [&I_FLUTE, &I_OCARINA, &I_HARP, &I_PIZZ, &I_MUSICBOX, &I_EPIANO, &I_STRINGS, &I_ACCORDION, &I_CHOIR, &I_ORGAN, &I_BASS, &I_BELL, &I_KIT, &I_NOISE];

pub fn instrument(name: &str) -> Option<&'static Instrument> { BANK.iter().copied().find(|i| i.name == name) }

impl Instrument {
    pub fn drum(&self, pitch: u8) -> Option<&'static Drum> {
        match self.kind { Kind::Kit(kit) => kit[usize::from(pitch % 12)].as_ref(), _ => None }
    }

    // the pitch register that sounds `pitch`, rounded to the chip's 14 bits; None past the register's top
    pub fn register(&self, pitch: f64) -> Option<u16> {
        let Kind::Tone(sample) = self.kind else { return None };
        let Tune::Root(root) = sample.tune else { return None };
        let reg = (4096.0 * 440.0 * 2f64.powf((pitch - 69.0) / 12.0) / root).round();
        (reg >= 1.0 && reg <= f64::from(crate::dsp::FULL_PITCH)).then_some(reg as u16)
    }
}

pub fn stored_register(sample: &Sample) -> u16 {
    match sample.tune { Tune::Stored(rate) => u16::try_from(u64::from(rate) * 4096 / u64::from(crate::dsp::RATE)).unwrap_or(crate::dsp::UNITY_PITCH), Tune::Root(_) => crate::dsp::UNITY_PITCH }
}

pub fn letters() -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    KIT.iter().zip(NAMES).filter_map(|(d, n)| d.as_ref().map(|d| format!("{n} {}", d.name))).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::named;
    use crate::recipes;

    #[test]
    fn the_bank_is_exactly_what_its_recipes_make() {
        let made = recipes::all();
        assert_eq!(made.len(), SAMPLES.len());
        for (made, sample) in made.iter().zip(SAMPLES) {
            assert_eq!(made.name, sample.name);
            assert_eq!(made.restart, sample.restart, "{}", made.name);
            assert_eq!(made.tune, sample.tune, "{}", made.name);
            assert!(brr::encode(&made.pcm, made.restart.map(|r| r / brr::BLOCK)) == sample.brr, "src/bank/{}.brr isn't what its recipe makes; run `cargo run --release --example bank`", made.name);
        }
    }

    #[test]
    fn every_sample_survives_its_brr_coding() {
        for made in recipes::all() {
            let sample = SAMPLES.iter().find(|s| s.name == made.name).unwrap();
            let back = sample.pcm();
            let signal: f64 = made.pcm.iter().map(|s| f64::from(*s).powi(2)).sum();
            let noise: f64 = made.pcm.iter().zip(back).map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2)).sum();
            let snr = 10.0 * (signal / noise.max(1.0)).log10();
            assert!(snr > 18.0, "{} comes back from brr at {snr:.1} db snr", made.name);
        }
    }

    #[test]
    fn loops_join_without_a_click_and_nothing_sits_off_centre() {
        for sample in SAMPLES {
            let pcm = sample.pcm();
            let mean = pcm.iter().map(|s| f64::from(*s)).sum::<f64>() / pcm.len() as f64;
            assert!(mean.abs() < 400.0, "{} sits {mean:.0} off centre", sample.name);
            let Some(restart) = sample.restart else {
                assert!(pcm.last().unwrap().unsigned_abs() < 300, "{} ends on a step of {}", sample.name, pcm.last().unwrap());
                continue;
            };
            let steps: Vec<i32> = pcm[restart..].windows(2).map(|w| (i32::from(w[1]) - i32::from(w[0])).abs()).collect();
            let typical = steps.iter().copied().max().unwrap();
            let seam = (i32::from(pcm[restart]) - i32::from(*pcm.last().unwrap())).abs();
            assert!(seam <= typical + typical / 4, "{}'s loop seam steps {seam}, its loop never steps more than {typical}", sample.name);
        }
    }

    #[test]
    fn ranges_say_what_they_enforce_and_fit_the_register() {
        for inst in BANK {
            if let Kind::Tone(_) = inst.kind {
                assert_eq!(inst.playable, format!("{} through {} on the {}", named(inst.low), named(inst.high), inst.name));
                assert!(inst.register(f64::from(inst.high)).is_some(), "{}'s top note is past the pitch register", inst.name);
                assert!(inst.register(f64::from(inst.low)).is_some_and(|r| r >= 256), "{}'s bottom note is too slow to sound", inst.name);
            }
            assert!(inst.adsr.valid());
        }
        assert_eq!(letters(), "C kick, C# rim, D snare, D# brush, E frame, F tom, F# hat, G shaker, G# bells, A ting");
    }

    #[test]
    fn a_root_plays_at_unity() {
        assert_eq!(I_EPIANO.register(71.0 + 12.0 * (500.0f64 / 493.883_301_256_124_1).log2()), Some(0x1000));
        assert_eq!(stored_register(&KICK), 0x800);
        assert_eq!(stored_register(&SNARE), 0xC00);
        assert_eq!(stored_register(&HAT), 0x1000);
    }
}
