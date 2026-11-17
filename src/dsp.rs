// an s-dsp of our own: eight sample voices at 32 khz, four-point gaussian interpolation, adsr envelopes, one noise clock for the
// whole chip, and an echo with feedback through an 8-tap fir. modelled on how the chip behaves, written from scratch: the gaussian
// table comes from a formula below, not from the chip's rom, and the fir coefficients are ours

pub const RATE: u32 = 32_000;

pub const VOICES: usize = 8;

// one echo step is 16 ms: 512 stereo frames, 2 kb of the chip's 64 kb of sound ram
pub const ECHO_STEP: usize = 512;

pub const ECHO_STEPS: u8 = 15;

pub const ECHO_STEP_BYTES: usize = 2048;

// how many samples apart the chip's 32 envelope and noise rates tick; rate 0 never does
pub const PERIODS: [u32; 32] = [0, 2048, 1536, 1280, 1024, 768, 640, 512, 384, 320, 256, 192, 160, 128, 96, 80, 64, 48, 40, 32, 24, 20, 16, 12, 10, 8, 6, 5, 4, 3, 2, 1];

// the key-off release drops the envelope by 8 every sample, so from full it lasts this long
pub const RELEASE: u64 = 0x800 / 8;

pub const FULL_PITCH: u16 = 0x3FFF;

pub const UNITY_PITCH: u16 = 0x1000;

// a one-sided lowpass summing to 127: every trip around the echo comes back a little darker
pub const FIR: [i32; 8] = [48, 32, 20, 12, 8, 4, 2, 1];

const MASTER: i32 = 0x60;

// K(d) = exp(-d^2 / (2 * 0.63^2)) at d = (512 - j) / 256, scaled so the four taps sum to at most 2048 (they land between 2043 and 2048).
// sigma 0.63 puts the centre tap near 64% of the sum, the soft shape the chip is known for
pub const GAUSS: [i32; 512] = [
    8, 9, 9, 9, 9, 9, 9, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13, 14, 14, 14, 14, 15, 15, 15,
    16, 16, 16, 16, 17, 17, 17, 18, 18, 18, 19, 19, 19, 20, 20, 20, 21, 21, 21, 22, 22, 23, 23, 23, 24, 24, 25, 25, 26, 26, 27, 27,
    27, 28, 28, 29, 29, 30, 30, 31, 31, 32, 33, 33, 34, 34, 35, 35, 36, 37, 37, 38, 38, 39, 40, 40, 41, 42, 42, 43, 44, 45, 45, 46,
    47, 47, 48, 49, 50, 51, 51, 52, 53, 54, 55, 56, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75,
    76, 78, 79, 80, 81, 82, 83, 85, 86, 87, 88, 90, 91, 92, 94, 95, 96, 98, 99, 100, 102, 103, 105, 106, 108, 109, 111, 112, 114, 115, 117, 119,
    120, 122, 123, 125, 127, 129, 130, 132, 134, 136, 137, 139, 141, 143, 145, 147, 149, 150, 152, 154, 156, 158, 160, 162, 164, 167, 169, 171, 173, 175, 177, 179,
    182, 184, 186, 189, 191, 193, 196, 198, 200, 203, 205, 208, 210, 213, 215, 218, 220, 223, 225, 228, 231, 233, 236, 239, 241, 244, 247, 250, 253, 255, 258, 261,
    264, 267, 270, 273, 276, 279, 282, 285, 288, 291, 294, 298, 301, 304, 307, 311, 314, 317, 320, 324, 327, 330, 334, 337, 341, 344, 348, 351, 355, 358, 362, 365,
    369, 373, 376, 380, 384, 388, 391, 395, 399, 403, 407, 410, 414, 418, 422, 426, 430, 434, 438, 442, 446, 450, 454, 458, 462, 466, 471, 475, 479, 483, 487, 492,
    496, 500, 504, 509, 513, 517, 522, 526, 531, 535, 539, 544, 548, 553, 557, 562, 566, 571, 575, 580, 585, 589, 594, 598, 603, 608, 612, 617, 622, 626, 631, 636,
    640, 645, 650, 655, 659, 664, 669, 674, 679, 683, 688, 693, 698, 703, 708, 712, 717, 722, 727, 732, 737, 742, 746, 751, 756, 761, 766, 771, 776, 781, 786, 790,
    795, 800, 805, 810, 815, 820, 825, 830, 834, 839, 844, 849, 854, 859, 864, 868, 873, 878, 883, 888, 893, 897, 902, 907, 912, 916, 921, 926, 931, 935, 940, 945,
    949, 954, 959, 963, 968, 973, 977, 982, 986, 991, 995, 1000, 1004, 1009, 1013, 1018, 1022, 1027, 1031, 1035, 1040, 1044, 1048, 1053, 1057, 1061, 1065, 1069, 1073, 1078, 1082, 1086,
    1090, 1094, 1098, 1102, 1106, 1110, 1113, 1117, 1121, 1125, 1129, 1132, 1136, 1140, 1143, 1147, 1150, 1154, 1157, 1161, 1164, 1168, 1171, 1174, 1178, 1181, 1184, 1187, 1190, 1193, 1196, 1199,
    1202, 1205, 1208, 1211, 1214, 1217, 1219, 1222, 1225, 1227, 1230, 1233, 1235, 1238, 1240, 1242, 1245, 1247, 1249, 1251, 1253, 1256, 1258, 1260, 1262, 1263, 1265, 1267, 1269, 1271, 1272, 1274,
    1276, 1277, 1279, 1280, 1281, 1283, 1284, 1285, 1287, 1288, 1289, 1290, 1291, 1292, 1293, 1294, 1295, 1295, 1296, 1297, 1297, 1298, 1298, 1299, 1299, 1300, 1300, 1300, 1301, 1301, 1301, 1301,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adsr { pub attack: u8, pub decay: u8, pub sustain: u8, pub release: u8 }

impl Adsr {
    pub fn valid(self) -> bool { self.attack <= 15 && self.decay <= 7 && self.sustain <= 7 && self.release <= 31 }

    // seconds to full: attack rates climb by 32 a tick over 64 ticks, except the fastest, which jumps 1024 at a time
    pub fn attack_seconds(self) -> f64 {
        let rate = usize::from(self.attack) * 2 + 1;
        let ticks = if rate == 31 { 2.0 } else { 63.0 };
        ticks * f64::from(PERIODS[rate]) / f64::from(RATE)
    }

    // seconds from full down into the sustain level, and from there to silence while the key is held; None is forever.
    // both walk the envelope the way the voice does, step by step, so they agree with it exactly
    pub fn decay_seconds(self) -> f64 { f64::from(decay(self.sustain).0) * f64::from(PERIODS[usize::from(self.decay) * 2 + 16]) / f64::from(RATE) }

    pub fn sustain_seconds(self) -> Option<f64> {
        let period = PERIODS[usize::from(self.release)];
        (period > 0).then(|| f64::from(drain(decay(self.sustain).1)) * f64::from(period) / f64::from(RATE))
    }

    pub fn level(self) -> f64 { f64::from(self.sustain + 1) / 8.0 }
}

fn step(env: u16) -> u16 { env - (((env - 1) >> 8) + 1) }

// ticks of the decay from full until the envelope enters the sustain level's band, and the level it enters at
fn decay(sustain: u8) -> (u32, u16) {
    let (mut env, mut ticks) = (0x7FF, 0);
    while env >> 8 > u16::from(sustain.min(7)) { env = step(env); ticks += 1; }
    (ticks, env)
}

// ticks of the sustain's fall from a level to silence
fn drain(from: u16) -> u32 {
    let (mut env, mut ticks) = (from, 0);
    while env > 0 { env = step(env); ticks += 1; }
    ticks
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode { Attack, Decay, Sustain, Release, Off }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Sample { pcm: &'static [i16], restart: Option<usize> }, Noise }

impl Source {
    fn at(self, n: usize) -> i32 {
        let Source::Sample { pcm, restart } = self else { return 0 };
        if let Some(&s) = pcm.get(n) { return i32::from(s); }
        match restart {
            Some(r) if r < pcm.len() => i32::from(pcm[r + (n - pcm.len()) % (pcm.len() - r)]),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Voice { source: Source, phase: u64, pitch: u16, env: u16, mode: Mode, adsr: Adsr, tick: u32, volume: [i32; 2], echo: bool }

impl Default for Voice {
    fn default() -> Self { Voice { source: Source::Noise, phase: 0, pitch: UNITY_PITCH, env: 0, mode: Mode::Off, adsr: Adsr { attack: 15, decay: 7, sustain: 7, release: 0 }, tick: 0, volume: [0; 2], echo: false } }
}

impl Voice {
    fn rings(&self) -> bool { self.mode != Mode::Off }

    fn envelope(&mut self) {
        let Adsr { attack, decay, sustain, release } = self.adsr;
        if self.mode == Mode::Release {
            self.env = self.env.saturating_sub(8);
            if self.env == 0 { self.mode = Mode::Off; }
            return;
        }
        // at or under the band, not only in it: a tie can raise the sustain level while the decay is already below it
        if self.mode == Mode::Decay && self.env >> 8 <= u16::from(sustain) { self.mode = Mode::Sustain; }
        let rate = match self.mode { Mode::Attack => usize::from(attack) * 2 + 1, Mode::Decay => usize::from(decay) * 2 + 16, Mode::Sustain => usize::from(release), _ => return };
        let period = PERIODS[rate];
        if period == 0 { return; }
        self.tick += 1;
        if self.tick < period { return; }
        self.tick = 0;
        match self.mode {
            Mode::Attack => {
                self.env += if rate == 31 { 1024 } else { 32 };
                if self.env >= 0x7E0 { self.env = self.env.min(0x7FF); self.mode = Mode::Decay; }
            }
            _ => self.env = self.env.saturating_sub(((self.env.max(1) - 1) >> 8) + 1),
        }
    }

    fn sample(&mut self, noise: i32) -> i32 {
        if !self.rings() { return 0; }
        let raw = match self.source {
            Source::Noise => noise,
            source @ Source::Sample { pcm, restart } => {
                let i = (self.phase >> 12) as usize;
                let f = ((self.phase >> 4) & 0xFF) as usize;
                let taps = [source.at(i), source.at(i + 1), source.at(i + 2), source.at(i + 3)];
                let mixed = (GAUSS[255 - f] * taps[0] >> 11) + (GAUSS[511 - f] * taps[1] >> 11) + (GAUSS[256 + f] * taps[2] >> 11) + (GAUSS[f] * taps[3] >> 11);
                self.phase += u64::from(self.pitch);
                let len = pcm.len() as u64;
                match restart {
                    Some(r) => { let span = (len - r as u64) << 12; while self.phase >> 12 >= len { self.phase -= span; } }
                    None if self.phase >> 12 >= len => { self.mode = Mode::Off; self.env = 0; }
                    None => {}
                }
                clamp(mixed)
            }
        };
        let out = raw * i32::from(self.env) >> 11;
        self.envelope();
        out
    }
}

#[derive(Debug, Clone)]
struct Echo { buffer: Vec<[i32; 2]>, at: usize, history: [[i32; 2]; 8], feedback: i32, wet: i32 }

impl Echo {
    fn step(&mut self, input: [i32; 2]) -> [i32; 2] {
        let Some(&heard) = self.buffer.get(self.at) else { return [0; 2] };
        self.history.rotate_right(1);
        self.history[0] = heard;
        let filtered: [i32; 2] = std::array::from_fn(|c| clamp(self.history.iter().zip(FIR).map(|(h, k)| h[c] * k).sum::<i32>() >> 7));
        self.buffer[self.at] = std::array::from_fn(|c| clamp(input[c] + (filtered[c] * self.feedback >> 7)));
        self.at = (self.at + 1) % self.buffer.len();
        filtered.map(|f| f * self.wet >> 7)
    }
}

fn clamp(s: i32) -> i32 { s.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) }

#[derive(Debug, Clone)]
pub struct Chip { voices: [Voice; VOICES], echo: Echo, lfsr: u16, clock: u8, noise_tick: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key { pub source: Source, pub adsr: Adsr, pub pitch: u16, pub volume: [i32; 2], pub echo: bool }

impl Chip {
    // `delay` in 16 ms steps (0 is no echo), feedback and wet as the chip's signed 8-bit registers
    pub fn new(delay: u8, feedback: i8, wet: i8) -> Chip {
        let echo = Echo { buffer: vec![[0; 2]; usize::from(delay.min(ECHO_STEPS)) * ECHO_STEP], at: 0, history: [[0; 2]; 8], feedback: feedback.into(), wet: wet.into() };
        Chip { voices: [Voice::default(); VOICES], echo, lfsr: 0x4000, clock: 0, noise_tick: 0 }
    }

    pub fn key_on(&mut self, voice: usize, key: Key) {
        let Key { source, adsr, pitch, volume, echo } = key;
        self.voices[voice] = Voice { source, phase: 0, pitch: pitch.min(FULL_PITCH), env: 0, mode: Mode::Attack, adsr, tick: 0, volume, echo };
    }

    pub fn key_off(&mut self, voice: usize) {
        let v = &mut self.voices[voice];
        if v.rings() { v.mode = Mode::Release; }
    }

    pub fn pitch(&mut self, voice: usize, pitch: u16) { self.voices[voice].pitch = pitch.min(FULL_PITCH); }

    pub fn route(&mut self, voice: usize, echo: bool, adsr: Adsr) { let v = &mut self.voices[voice]; v.echo = echo; v.adsr = adsr; }

    pub fn volume(&mut self, voice: usize, volume: [i32; 2]) { self.voices[voice].volume = volume; }

    pub fn noise_clock(&mut self, clock: u8) { self.clock = clock.min(31); }

    pub fn ringing(&self, voice: usize) -> bool { self.voices[voice].rings() }

    pub fn level(&self, voice: usize) -> u16 { self.voices[voice].env }

    pub fn sample(&mut self) -> [i16; 2] {
        let period = PERIODS[usize::from(self.clock)];
        if period > 0 {
            self.noise_tick += 1;
            if self.noise_tick >= period {
                self.noise_tick = 0;
                self.lfsr = (self.lfsr >> 1) | (((self.lfsr << 14) ^ (self.lfsr << 13)) & 0x4000);
            }
        }
        let noise = i32::from((self.lfsr << 1) as i16);
        let (mut main, mut send) = ([0; 2], [0; 2]);
        for v in &mut self.voices {
            let out = v.sample(noise);
            for c in 0..2 {
                let side = out * v.volume[c] >> 7;
                main[c] += side;
                if v.echo { send[c] += side; }
            }
        }
        let back = self.echo.step(send.map(clamp));
        std::array::from_fn(|c| clamp((clamp(main[c]) * MASTER >> 7) + back[c]) as i16)
    }
}

// the noise clock nearest a frequency, among the 31 that tick
pub fn nearest_clock(hz: f64) -> u8 {
    (1..32u8).min_by(|a, b| {
        let off = |r: u8| (f64::from(RATE) / f64::from(PERIODS[usize::from(r)]) / hz).log2().abs();
        off(*a).total_cmp(&off(*b))
    }).unwrap_or(1)
}

pub fn clock_hz(clock: u8) -> f64 { f64::from(RATE) / f64::from(PERIODS[usize::from(clock.clamp(1, 31))]) }

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel(j: usize) -> f64 { let d = (512.0 - j as f64) / 256.0; (-(d * d) / (2.0 * 0.63 * 0.63)).exp() }

    #[test]
    fn the_gaussian_table_is_its_formula() {
        let taps = |g: &dyn Fn(usize) -> f64, i: usize| g(255 - i) + g(511 - i) + g(256 + i) + g(i);
        let scale = 2047.0 / (0..256).map(|i| taps(&kernel, i)).fold(0.0, f64::max);
        for (j, &g) in GAUSS.iter().enumerate() { assert!((f64::from(g) - scale * kernel(j)).abs() <= 0.5, "entry {j} is {g}, the formula says {}", scale * kernel(j)); }
        for i in 0..256 {
            let sum = GAUSS[255 - i] + GAUSS[511 - i] + GAUSS[256 + i] + GAUSS[i];
            assert!((2043..=2048).contains(&sum), "fraction {i} sums to {sum}");
        }
        assert!(GAUSS.windows(2).all(|w| w[0] <= w[1]), "the table climbs to its centre");
    }

    #[test]
    fn the_envelope_rates_give_the_chip_times() {
        let adsr = |attack, decay, sustain, release| Adsr { attack, decay, sustain, release };
        assert!((adsr(0, 0, 7, 0).attack_seconds() - 4.032).abs() < 0.001);
        assert!((adsr(15, 0, 7, 0).attack_seconds() - 0.0000625).abs() < 1e-9);
        assert!((adsr(10, 0, 7, 0).attack_seconds() - 0.039375).abs() < 1e-9, "{}", adsr(10, 0, 7, 0).attack_seconds());
        assert_eq!(adsr(15, 7, 7, 0).sustain_seconds(), None);
        assert_eq!(adsr(15, 7, 7, 0).decay_seconds(), 0.0, "sustain level 7 is full, so there is nothing to decay");
        assert!(adsr(15, 0, 0, 0).decay_seconds() > adsr(15, 7, 0, 0).decay_seconds());
    }

    fn held(chip: &mut Chip, n: usize) -> Vec<[i16; 2]> { (0..n).map(|_| chip.sample()).collect() }

    #[test]
    fn a_sustain_raised_mid_decay_holds_instead_of_draining() {
        let mut chip = Chip::new(0, 0, 0);
        chip.key_on(0, Key { adsr: Adsr { attack: 15, decay: 0, sustain: 0, release: 0 }, ..key(0x1000) });
        held(&mut chip, 40_000);
        assert!(chip.level(0) >> 8 < 7, "the decay has passed band 7");
        chip.route(0, false, Adsr { attack: 15, decay: 0, sustain: 7, release: 0 });
        held(&mut chip, 64_000);
        assert!(chip.level(0) > 0, "with the new sustain at full and no sustain release, the voice holds where it is");
    }

    #[test]
    fn the_envelope_times_are_the_voice_s_own() {
        for adsr in [Adsr { attack: 15, decay: 7, sustain: 0, release: 31 }, Adsr { attack: 13, decay: 4, sustain: 3, release: 22 }, Adsr { attack: 15, decay: 2, sustain: 5, release: 11 }] {
            let mut chip = Chip::new(0, 0, 0);
            chip.key_on(0, Key { adsr, ..key(0x1000) });
            // a sustain that drains to zero leaves the voice keyed on and silent, as the chip does: count to the silence
            let (mut samples, mut risen) = (0u32, false);
            while (!risen || chip.level(0) > 0) && samples < 32_000 * 30 { chip.sample(); samples += 1; risen |= chip.level(0) > 0; }
            let predicted = (adsr.attack_seconds() + adsr.decay_seconds() + adsr.sustain_seconds().unwrap()) * f64::from(RATE);
            let slack = f64::from(PERIODS[usize::from(adsr.attack) * 2 + 1] + PERIODS[usize::from(adsr.decay) * 2 + 16] + PERIODS[usize::from(adsr.release)]) * 2.0;
            assert!((f64::from(samples) - predicted).abs() <= slack, "{adsr:?}: the voice lasts {samples} samples, the estimate says {predicted:.0}");
        }
    }

    static SQUARE: [i16; 32] = [12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, 12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000, -12_000];

    fn square() -> Source { Source::Sample { pcm: &SQUARE, restart: Some(0) } }

    fn key(pitch: u16) -> Key { Key { source: square(), adsr: Adsr { attack: 15, decay: 7, sustain: 7, release: 0 }, pitch, volume: [100, 100], echo: false } }

    #[test]
    fn a_looped_cycle_plays_at_its_pitch_register() {
        for pitch in [0x0800, 0x1000, 0x1800, 0x3FFF] {
            let mut chip = Chip::new(0, 0, 0);
            chip.key_on(0, key(pitch));
            let out = held(&mut chip, 32_000);
            let rises = out.windows(2).filter(|w| w[0][0] < 0 && w[1][0] >= 0).count();
            let want = f64::from(pitch) / 4096.0 * 1000.0;
            assert!((rises as f64 - want).abs() <= 1.0, "pitch {pitch:#x} crosses {rises} times a second, wants {want}");
        }
    }

    #[test]
    fn a_released_voice_is_silent_within_eight_milliseconds() {
        let mut chip = Chip::new(0, 0, 0);
        chip.key_on(0, key(0x1000));
        held(&mut chip, 1000);
        chip.key_off(0);
        let tail = held(&mut chip, 300);
        let last = tail.iter().rposition(|s| s[0] != 0).unwrap();
        assert!(last < RELEASE as usize, "the release still sounds at sample {last}");
        assert!(!chip.ringing(0));
    }

    #[test]
    fn an_unlooped_sample_ends_itself() {
        static BLIP: [i16; 16] = [9_000; 16];
        let mut chip = Chip::new(0, 0, 0);
        chip.key_on(0, Key { source: Source::Sample { pcm: &BLIP, restart: None }, ..key(0x1000) });
        let out = held(&mut chip, 64);
        assert!(out[..8].iter().any(|s| s[0] != 0));
        assert!(out[20..].iter().all(|s| s[0] == 0));
        assert!(!chip.ringing(0));
    }

    #[test]
    fn the_echo_comes_back_after_exactly_its_delay() {
        for steps in [1, 5, 15] {
            let mut chip = Chip::new(steps, 0, 127);
            static CLICK: [i16; 16] = [0, 30_000, 30_000, 30_000, 30_000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            chip.key_on(0, Key { source: Source::Sample { pcm: &CLICK, restart: None }, echo: true, ..key(0x1000) });
            let out = held(&mut chip, usize::from(steps) * ECHO_STEP + 64);
            let dry = out.iter().position(|s| s[0] != 0).unwrap();
            let wet = out.iter().skip(20).position(|s| s[0] != 0).unwrap() + 20;
            assert_eq!(wet - dry, usize::from(steps) * ECHO_STEP, "{steps} steps of echo");
        }
    }

    #[test]
    fn feedback_rings_down_and_never_runs_away() {
        let mut chip = Chip::new(2, 127, 127);
        chip.key_on(0, Key { echo: true, ..key(0x1000) });
        held(&mut chip, 4_000);
        chip.key_off(0);
        let tail = held(&mut chip, 32_000 * 8);
        let late = tail[tail.len() - 1_000..].iter().map(|s| s[0].unsigned_abs()).max().unwrap();
        let early = tail[..2_000].iter().map(|s| s[0].unsigned_abs()).max().unwrap();
        assert!(late < early / 4, "full feedback should still fade through the fir: {early} then {late}");
    }

    #[test]
    fn eight_voices_flat_out_stay_on_the_rails() {
        let mut chip = Chip::new(15, 127, 127);
        for v in 0..VOICES { chip.key_on(v, Key { volume: [127, 127], echo: true, ..key(0x1000 + v as u16 * 0x100) }); }
        let out = held(&mut chip, 32_000);
        assert!(out.iter().any(|s| s[0] == i16::MAX || s[0] == i16::MIN), "this mix should be hitting the rails, clamped");
    }

    #[test]
    fn the_noise_clock_snaps_to_the_nearest_rate() {
        assert_eq!(clock_hz(nearest_clock(32_000.0)), 32_000.0);
        assert_eq!(clock_hz(nearest_clock(440.0)), 400.0);
        assert_eq!(clock_hz(nearest_clock(1.0)), 15.625);
        let mut chip = Chip::new(0, 0, 0);
        chip.noise_clock(nearest_clock(4_000.0));
        chip.key_on(0, Key { source: Source::Noise, ..key(0x1000) });
        let out = held(&mut chip, 3_200);
        let changes = out.windows(2).filter(|w| w[0][0] != w[1][0]).count();
        assert!((350..=410).contains(&changes), "a 4 khz noise clock changes about 400 times in 0.1 s, saw {changes}");
    }

    #[test]
    fn the_same_keys_give_the_same_bytes() {
        let run = || { let mut chip = Chip::new(4, 40, 50); chip.key_on(3, Key { echo: true, ..key(0x1234) }); held(&mut chip, 10_000) };
        assert_eq!(run(), run());
    }
}
