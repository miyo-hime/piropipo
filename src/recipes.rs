// the snes bank's recipes. every sample piro ships is made here from arithmetic alone: partials, plucked delay lines, filtered noise.
// `cargo run --release --example bank` writes what these make into src/bank/, and a test makes them again and holds the files to
// it byte for byte, so the samples carry the code's licence and nobody has to trust where a wav came from

use crate::brr::BLOCK;
use std::f64::consts::{PI, TAU};

pub const RATE: f64 = 32_000.0;

const PEAK: f64 = 15_500.0;

// a pitched sample's first quarter second (looping round its loop if it's shorter) is pressed to this rms, so `vol 10` sounds
// about as loud on any instrument; the peak cap wins where a spiky wave would pass it. drums are pressed by their peak
const LOUD: f64 = 4_500.0;

const BODY: usize = 8_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tune { Root(f64), Stored(u32) }

#[derive(Debug, Clone, PartialEq)]
pub struct Made { pub name: &'static str, pub pcm: Vec<i16>, pub restart: Option<usize>, pub tune: Tune }

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

// the audio eq cookbook's biquads, transposed direct form ii
#[derive(Clone, Copy)]
struct Biquad { b: [f64; 3], a: [f64; 2], z: [f64; 2] }

impl Biquad {
    fn new(kind: char, hz: f64, q: f64, rate: f64) -> Biquad {
        let w = TAU * hz / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b = match kind {
            'l' => [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
            'h' => [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
            _ => [alpha, 0.0, -alpha],
        };
        Biquad { b: b.map(|x| x / a0), a: [-2.0 * cos / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

fn noise(seed: u64, len: usize, filters: &[(char, f64, f64)], rate: f64) -> Vec<f64> {
    let mut rng = Rng(seed);
    let mut chain: Vec<Biquad> = filters.iter().map(|&(k, hz, q)| Biquad::new(k, hz, q, rate)).collect();
    (0..len).map(|_| chain.iter_mut().fold(rng.next(), |x, f| f.run(x))).collect()
}

// partials as (multiple of f0, amplitude, phase); a loop of `len` samples holds a whole number of cycles when f0 is RATE * k / len
fn partials(n: usize, f0: f64, list: &[(f64, f64, f64)]) -> f64 { list.iter().map(|&(h, a, p)| a * (TAU * h * f0 * n as f64 / RATE + p).sin()).sum() }

fn rise(n: usize, over: usize) -> f64 { if n >= over { 1.0 } else { 0.5 - 0.5 * (PI * n as f64 / over as f64).cos() } }

fn decay(n: usize, seconds: f64, rate: f64) -> f64 { (-(n as f64) / (seconds * rate)).exp() }

// an attack that turns, by its last sample, into a steady cycle the loop repeats; `steady` must be periodic over `cycle`
fn attack_then_loop(attack: usize, cycle: usize, evolving: impl Fn(usize) -> f64, steady: impl Fn(usize) -> f64, blend: usize) -> (Vec<f64>, usize) {
    assert!(attack.is_multiple_of(BLOCK) && cycle.is_multiple_of(BLOCK) && blend <= attack);
    let wave = (0..attack + cycle).map(|n| {
        if n >= attack { return steady(n); }
        let into = n + blend;
        if into < attack { return evolving(n); }
        let w = rise(into - attack, blend);
        evolving(n) * (1.0 - w) + steady(n) * w
    }).collect();
    (wave, attack)
}

fn press(name: &'static str, wave: &[f64], restart: Option<usize>, tune: Tune) -> Made {
    let mut wave = wave.to_vec();
    if restart.is_none() {
        let len = wave.len();
        for (i, s) in wave.iter_mut().enumerate().skip(len.saturating_sub(64)) { *s *= (len - i) as f64 / 64.0; }
        wave.resize(len.div_ceil(BLOCK) * BLOCK, 0.0);
    }
    assert!(wave.len().is_multiple_of(BLOCK), "{name} isn't whole blocks");
    let peak = wave.iter().fold(0.0f64, |m, s| m.max(s.abs())).max(1e-9);
    let gain = match tune {
        Tune::Root(_) => (LOUD / body_rms(&wave, restart).max(1e-9)).min(PEAK / peak),
        Tune::Stored(_) => PEAK / peak,
    };
    let pcm = wave.iter().map(|s| (s * gain).round() as i16).collect();
    Made { name, pcm, restart, tune }
}

pub fn body_rms<T: Copy + Into<f64>>(wave: &[T], restart: Option<usize>) -> f64 {
    let at = |n: usize| match restart {
        _ if n < wave.len() => wave[n].into(),
        Some(r) if r < wave.len() => wave[r + (n - wave.len()) % (wave.len() - r)].into(),
        _ => 0.0,
    };
    ((0..BODY).map(|n| at(n).powi(2)).sum::<f64>() / BODY as f64).sqrt()
}

fn breathy_lead(name: &'static str, attack: usize, cycle: usize, k: usize, list: &[(f64, f64, f64)], chiff: f64, chiff_hz: f64, seed: u64) -> Made {
    let f0 = RATE * k as f64 / cycle as f64;
    let hiss = noise(seed, attack, &[('b', chiff_hz, 1.2), ('l', 6_000.0, 0.7)], RATE);
    let hiss_peak = hiss.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    let evolving = |n: usize| {
        let swell = rise(n, 480);
        let overblow: Vec<(f64, f64, f64)> = list.iter().map(|&(h, a, p)| (h, if h == 2.0 { a * (1.0 + 1.5 * decay(n, 0.02, RATE)) } else { a }, p)).collect();
        swell * partials(n, f0, &overblow) + chiff * hiss[n] / hiss_peak * decay(n, 0.018, RATE) * rise(n, 64)
    };
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, |n| partials(n, f0, list), 256);
    press(name, &wave, Some(restart), Tune::Root(f0))
}

pub fn flute() -> Made { breathy_lead("flute", 3_840, 512, 17, &[(1.0, 1.0, 0.0), (2.0, 0.30, 0.3), (3.0, 0.12, 1.1), (4.0, 0.045, 2.0), (5.0, 0.02, 0.7)], 0.35, 2_200.0, 11) }

pub fn ocarina() -> Made { breathy_lead("ocarina", 1_920, 512, 14, &[(1.0, 1.0, 0.0), (2.0, 0.05, 0.9), (3.0, 0.035, 2.2)], 0.16, 1_500.0, 12) }

// a plucked string: a delay line of `period` samples, fed back through a gentle three-tap lowpass, so it rings at exactly RATE / period
fn plucked(name: &'static str, period: usize, seconds: f64, darkness: f64, brightness: f64, seed: u64, cycles: usize) -> Made {
    let attack = ((seconds * RATE) as usize).div_ceil(BLOCK) * BLOCK;
    let mut rng = Rng(seed);
    let mut smooth = Biquad::new('l', brightness, 0.6, RATE);
    let mut line: Vec<f64> = (0..period).map(|i| {
        let shape = 1.0 - 2.0 * ((i as f64 / period as f64) - 0.3).abs().min(0.7) / 0.7;
        smooth.run(0.7 * rng.next() + 0.6 * shape)
    }).collect();
    let mean = line.iter().sum::<f64>() / period as f64;
    line.iter_mut().for_each(|s| *s -= mean);
    let total = attack + period * cycles;
    let mut wave = line.clone();
    while wave.len() < total + period {
        let n = wave.len();
        let back = |k: usize| n.checked_sub(k).map_or(0.0, |i| wave[i]);
        wave.push(0.9993 * ((1.0 - darkness) * back(period) + darkness * 0.5 * (back(period - 1) + back(period + 1))));
    }
    let tail: Vec<f64> = wave[attack..attack + period * cycles].to_vec();
    wave.truncate(attack);
    wave.extend(tail);
    for (n, s) in wave.iter_mut().enumerate().take(16) { *s *= rise(n, 16); }
    press(name, &wave, Some(attack), Tune::Root(RATE / period as f64))
}

pub fn harp() -> Made { plucked("harp", 48, 0.2, 0.3, 4_500.0, 21, 1) }

pub fn pizz() -> Made { plucked("pizz", 288, 0.2, 0.55, 1_800.0, 22, 1) }

fn fm(n: usize, f0: f64, ratio: f64, index: f64) -> f64 { let t = n as f64 / RATE; (TAU * f0 * t + index * (TAU * ratio * f0 * t).sin()).sin() }

pub fn musicbox() -> Made {
    let (attack, cycle, k) = (3_200, 256, 9);
    let f0 = RATE * k as f64 / cycle as f64;
    let evolving = |n: usize| fm(n, f0, 4.0, 0.2 + 2.0 * decay(n, 0.019, RATE)) + 0.3 * decay(n, 0.009, RATE) * partials(n, f0, &[(5.23, 1.0, 0.0)]);
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, |n| fm(n, f0, 4.0, 0.2), 512);
    press("musicbox", &wave, Some(restart), Tune::Root(f0))
}

pub fn epiano() -> Made {
    let (attack, cycle, k) = (4_800, 512, 8);
    let f0 = RATE * k as f64 / cycle as f64;
    let evolving = |n: usize| fm(n, f0, 1.0, 0.35 + 1.3 * decay(n, 0.04, RATE)) + 0.22 * decay(n, 0.008, RATE) * partials(n, f0, &[(14.03, 1.0, 0.0)]);
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, |n| fm(n, f0, 1.0, 0.35), 512);
    press("epiano", &wave, Some(restart), Tune::Root(f0))
}

// detuned voices that each fit a whole number of cycles in one long loop, so the ensemble beats the same way every pass
fn ensemble(name: &'static str, len: usize, ks: &[(usize, f64)], spectrum: impl Fn(f64, f64) -> f64, top: f64, seed: u64) -> Made {
    let mut rng = Rng(seed);
    let centre = ks[ks.len() / 2].0;
    let f0 = RATE * centre as f64 / len as f64;
    let voices: Vec<Vec<(f64, f64, f64)>> = ks.iter().map(|&(k, level)| {
        let fk = RATE * k as f64 / len as f64;
        (1..).map(|h: u32| f64::from(h)).take_while(|h| h * fk <= top).map(|h| (h * fk / f0, level * spectrum(h, h * fk), PI * rng.next())).collect()
    }).collect();
    let wave: Vec<f64> = (0..len).map(|n| voices.iter().map(|v| partials(n, f0, v)).sum()).collect();
    press(name, &wave, Some(0), Tune::Root(f0))
}

pub fn strings() -> Made { ensemble("strings", 12_288, &[(143, 0.8), (144, 1.0), (145, 0.8)], |h, f| 1.0 / h / (1.0 + (f / 2_400.0).powi(2)), 6_000.0, 31) }

pub fn accordion() -> Made { ensemble("accordion", 8_192, &[(128, 1.0), (129, 0.85)], |h, f| (1.0 / h.powf(0.7)) * (1.0 + 0.8 / (1.0 + ((f - 1_300.0) / 500.0).powi(2))) / (1.0 + (f / 3_500.0).powi(2)), 6_000.0, 32) }

pub fn choir() -> Made {
    let formants = [(730.0, 1.0, 110.0), (1_090.0, 0.5, 120.0), (2_440.0, 0.25, 200.0), (3_400.0, 0.1, 300.0)];
    let shape = move |h: f64, f: f64| (0.04 + formants.iter().map(|&(hz, a, bw)| a / (1.0 + ((f - hz) / bw).powi(2))).sum::<f64>()) / h.sqrt();
    ensemble("choir", 8_192, &[(95, 0.75), (96, 1.0), (97, 0.75)], shape, 5_000.0, 33)
}

pub fn organ() -> Made {
    let (attack, cycle) = (640, 64);
    let f0 = RATE / cycle as f64;
    let stops = [(1.0, 1.0, 0.0), (2.0, 0.7, 0.4), (3.0, 0.35, 0.9), (4.0, 0.45, 1.3), (5.0, 0.12, 0.2), (6.0, 0.2, 2.1), (8.0, 0.25, 0.6)];
    let chiff = noise(41, attack, &[('b', 3_000.0, 1.5)], RATE);
    let evolving = |n: usize| partials(n, f0, &stops) * rise(n, 160) + 0.25 * chiff[n] * decay(n, 0.006, RATE);
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, |n| partials(n, f0, &stops), 128);
    press("organ", &wave, Some(restart), Tune::Root(f0))
}

pub fn bass() -> Made {
    let (attack, cycle) = (1_280, 256);
    let f0 = RATE / cycle as f64;
    let body = [(1.0, 1.0, 0.0), (2.0, 0.45, 0.5), (3.0, 0.2, 1.0), (4.0, 0.08, 1.7), (5.0, 0.04, 0.3)];
    let evolving = |n: usize| { let thump = 1.0 + 1.2 * decay(n, 0.012, RATE); rise(n, 96) * partials(n, f0, &body.map(|(h, a, p)| (h, if h > 1.0 { a * thump } else { a }, p))) };
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, |n| partials(n, f0, &body), 512);
    press("bass", &wave, Some(restart), Tune::Root(f0))
}

// a church bell's hum, prime, tierce, quint and nominal; the tierce and the upper partials die in the attack, and what loops is the
// part of the bell that is harmonic over the hum
pub fn bell() -> Made {
    let (attack, cycle) = (8_000, 512);
    let prime = RATE * 8.0 / cycle as f64;
    let ring = [(0.5, 0.7, 0.0, 9.0), (1.0, 0.6, 0.6, 3.0), (1.2, 0.5, 1.2, 0.5), (1.5, 0.3, 2.0, 0.8), (2.0, 0.55, 0.4, 1.2), (2.5, 0.2, 1.4, 0.25), (3.0, 0.16, 0.9, 0.18), (4.2, 0.12, 2.6, 0.08)];
    let harmonic = |h: f64| [0.5, 1.0, 1.5, 2.0].contains(&h);
    let held = |n: usize| ring.iter().filter(|r| harmonic(r.0)).map(|&(h, a, p, life)| a * decay(attack, life, RATE) * (TAU * h * prime * n as f64 / RATE + p).sin()).sum::<f64>();
    let evolving = |n: usize| {
        let strike = (1.0 + 2.0 * decay(n, 0.004, RATE)) * rise(n, 24);
        ring.iter().map(|&(h, a, p, life)| a * strike.min(if h > 2.0 { 3.0 } else { 1.5 }) * decay(n, life, RATE) * (TAU * h * prime * n as f64 / RATE + p).sin()).sum::<f64>()
    };
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, held, 2_048);
    press("bell", &wave, Some(restart), Tune::Root(prime))
}

// the kit: each drum stored at the rate it needs, and played at that rate whatever pitch is written
fn drum(name: &'static str, rate: u32, seconds: f64, voice: impl Fn(usize, f64) -> f64) -> Made {
    let len = (seconds * f64::from(rate)) as usize;
    let wave: Vec<f64> = (0..len).map(|n| voice(n, f64::from(rate))).collect();
    press(name, &wave, None, Tune::Stored(rate))
}

fn sweep(n: usize, rate: f64, from: f64, to: f64, fall: f64) -> f64 {
    // the phase of a pitch gliding exponentially from `from` to `to`, integrated in closed form
    let t = n as f64 / rate;
    TAU * (to * t + (from - to) * fall * (1.0 - (-t / fall).exp()))
}

pub fn kick() -> Made {
    let click = noise(51, 200, &[('l', 3_000.0, 0.7)], 16_000.0);
    drum("kick", 16_000, 0.32, |n, r| rise(n, 8) * decay(n, 0.17, r) * sweep(n, r, 160.0, 48.0, 0.035).sin() + click.get(n).map_or(0.0, |c| 0.35 * c * decay(n, 0.003, r)))
}

pub fn snare() -> Made {
    let hiss = noise(52, 6_000, &[('b', 3_500.0, 0.7), ('h', 900.0, 0.7)], 24_000.0);
    let peak = hiss.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    drum("snare", 24_000, 0.25, |n, r| rise(n, 6) * (0.55 * decay(n, 0.05, r) * ((TAU * 185.0 * n as f64 / r).sin() + 0.5 * (TAU * 330.0 * n as f64 / r).sin()) + 0.8 * decay(n, 0.11, r) * hiss[n] / peak))
}

// a swish, not a hit: stored at the full rate so it keeps its air, swelling in over 25 ms
pub fn brush() -> Made {
    let hiss = noise(53, 8_000, &[('b', 3_500.0, 0.5), ('h', 1_200.0, 0.7)], RATE);
    drum("brush", 32_000, 0.25, |n, r| rise(n, 800) * decay(n, 0.11, r) * hiss[n])
}

pub fn rim() -> Made {
    let click = noise(54, 1_600, &[('h', 2_000.0, 0.7)], RATE);
    drum("rim", 32_000, 0.05, |n, r| decay(n, 0.012, r) * (0.7 * (TAU * 1_700.0 * n as f64 / r).sin() + 0.45 * (TAU * 820.0 * n as f64 / r).sin()) + 0.5 * click[n] * decay(n, 0.002, r))
}

pub fn frame() -> Made {
    let slap = noise(55, 7_200, &[('l', 1_500.0, 0.7)], 16_000.0);
    drum("frame", 16_000, 0.45, |n, r| rise(n, 16) * decay(n, 0.25, r) * sweep(n, r, 100.0, 75.0, 0.05).sin() + 0.4 * slap[n] * decay(n, 0.02, r))
}

pub fn tom() -> Made { drum("tom", 16_000, 0.35, |n, r| rise(n, 12) * decay(n, 0.16, r) * sweep(n, r, 170.0, 130.0, 0.04).sin()) }

pub fn hat() -> Made {
    let hiss = noise(57, 2_240, &[('h', 7_000.0, 0.7), ('h', 7_000.0, 0.7), ('b', 10_000.0, 0.8)], RATE);
    drum("hat", 32_000, 0.07, |n, r| rise(n, 8) * decay(n, 0.018, r) * hiss[n])
}

pub fn shaker() -> Made {
    let hiss = noise(58, 3_840, &[('b', 6_000.0, 1.0), ('h', 2_500.0, 0.7)], RATE);
    drum("shaker", 32_000, 0.12, |n, r| rise(n, 384) * decay(n, 0.035, r) * hiss[n])
}

pub fn bells() -> Made {
    let mut rng = Rng(59);
    let jingles: Vec<(usize, f64, f64)> = (0..14).map(|i| {
        let at = (0.09 * (i as f64 / 14.0).powf(1.6) + 0.006 * rng.next().abs()) * RATE;
        (at as usize, 2_200.0 + 1_000.0 * rng.next().abs(), 0.05 + 0.04 * rng.next().abs())
    }).collect();
    let shimmer = noise(60, 9_600, &[('h', 6_000.0, 0.7)], RATE);
    drum("bells", 32_000, 0.3, move |n, r| {
        let ring: f64 = jingles.iter().filter(|j| n >= j.0).map(|&(at, hz, life)| {
            let k = n - at;
            decay(k, life, r) * rise(k, 6) * [(1.0, 1.0), (1.48, 0.6), (2.1, 0.4), (2.9, 0.25)].iter().map(|&(m, a)| a * (TAU * m * hz * k as f64 / r).sin()).sum::<f64>()
        }).sum();
        0.14 * ring + 0.25 * shimmer[n] * decay(n, 0.06, r)
    })
}

pub fn ting() -> Made {
    let (attack, cycle, k) = (2_560, 256, 13);
    let f0 = RATE * k as f64 / cycle as f64;
    let evolving = |n: usize| partials(n, f0, &[(1.0, 1.0, 0.0), (3.0, 0.35, 0.0), (5.0, 0.2, 0.0)]) + decay(n, 0.02, RATE) * partials(n, f0, &[(2.76, 0.6, 0.0), (5.4, 0.4, 0.0), (8.9, 0.2, 0.0)]);
    let (wave, restart) = attack_then_loop(attack, cycle, evolving, |n| partials(n, f0, &[(1.0, 1.0, 0.0), (3.0, 0.35, 0.0), (5.0, 0.2, 0.0)]), 512);
    press("ting", &wave, Some(restart), Tune::Stored(32_000))
}

pub fn all() -> Vec<Made> {
    vec![flute(), ocarina(), harp(), pizz(), musicbox(), epiano(), strings(), accordion(), choir(), organ(), bass(), bell(), kick(), rim(), snare(), brush(), frame(), tom(), hat(), shaker(), bells(), ting()]
}
