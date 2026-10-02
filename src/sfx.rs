use crate::surface;
use serde_json::{Value, json};
use sfxr::{Generator, Sample, WaveType};
use std::path::{Path, PathBuf};

const RATE: u32 = 44_100;
const TICKS: f64 = 352_800.0;
const STAGE_CAP: u32 = 100_000;
const STAGE_MS: f64 = 2267.0;
const FIRST_COIN: u64 = 9;

type Roll = fn(Option<u64>) -> Sample;

const PRESETS: [(&str, Roll); 7] = [("coin", Sample::pickup), ("jump", Sample::jump), ("hurt", Sample::hit), ("explosion", Sample::explosion), ("powerup", Sample::powerup), ("blip", Sample::blip), ("laser", Sample::laser)];

const WAVES: [(&str, WaveType); 4] = [("square", WaveType::Square), ("saw", WaveType::Triangle), ("sine", WaveType::Sine), ("noise", WaveType::Noise)];

const STAGES: [&str; 3] = ["attack", "sustain", "decay"];

struct Dial { flag: &'static str, units: &'static [(&'static str, f64)], low: f64, high: f64, unit: &'static str, hint: &'static str }

const MILLIS: &[(&str, f64)] = &[("ms", 1.0), ("s", 1000.0), ("", 1.0)];
const PITCH: Dial = Dial { flag: "pitch", units: &[("hz", 1.0), ("", 1.0)], low: 4.0, high: 3500.0, unit: " Hz", hint: "a pitch in Hz, like 880" };
const LENGTH: Dial = Dial { flag: "length", units: MILLIS, low: 1.0, high: 3.0 * STAGE_MS, unit: " ms", hint: "a duration, like 300ms or 0.3s" };
const ATTACK: Dial = Dial { flag: "attack", units: MILLIS, low: 0.0, high: STAGE_MS, unit: " ms", hint: "a duration, like 20ms" };
const DECAY: Dial = Dial { flag: "decay", units: MILLIS, low: 0.0, high: STAGE_MS, unit: " ms", hint: "a duration, like 150ms" };
const SWEEP: Dial = Dial { flag: "sweep", units: &[("st", 1.0), ("", 1.0)], low: -72.0, high: 72.0, unit: " semitones", hint: "a number of semitones, like +12 or -7" };

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "coin, jump, hurt, explosion, powerup, blip, laser, or custom")]
    pub preset: String,
    #[arg(short, long, help = "where the wav lands (default: <preset>.wav in cwd)")]
    pub output: Option<PathBuf>,
    #[arg(long, help = "square, saw, sine, or noise")]
    pub wave: Option<String>,
    #[arg(long, allow_hyphen_values = true, help = "start pitch in Hz, 4 to 3500")]
    pub pitch: Option<String>,
    #[arg(long, allow_hyphen_values = true, help = "total length, like 300ms or 0.3s")]
    pub length: Option<String>,
    #[arg(long, allow_hyphen_values = true, help = "fade-in, like 20ms")]
    pub attack: Option<String>,
    #[arg(long, allow_hyphen_values = true, help = "fade-out, like 150ms")]
    pub decay: Option<String>,
    #[arg(long, allow_hyphen_values = true, help = "semitones of glide over the sound, like +12 or -7")]
    pub sweep: Option<String>,
    #[arg(long, allow_hyphen_values = true, help = "roll the preset's dice differently (default: 9)")]
    pub seed: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Remark { code: &'static str, message: String }

impl Remark {
    fn new(code: &'static str, message: impl Into<String>) -> Remark { Remark { code, message: message.into() } }

    fn json(&self) -> Value { json!({ "code": self.code, "message": self.message, "pattern": null, "lane": null, "at": null, "line": null }) }
}

struct Recipe { preset: &'static str, roll: Option<Roll>, seed: Option<u64>, wave: Option<WaveType>, pitch: Option<f64>, length: Option<u32>, attack: Option<u32>, decay: Option<u32>, sweep: Option<f64> }

struct Take { samples: Vec<i16>, stats: Value, warnings: Vec<Remark> }

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> {
    let landing = args.output.clone().unwrap_or_else(|| PathBuf::from(format!("{}.wav", args.preset)));
    let landed = sound(&args).and_then(|take| {
        let bytes = pressed(&take.samples).map_err(|e| vec![Remark::new("write-failed", format!("couldn't encode the wav: {e}"))])?;
        publish(&landing, &bytes).map_err(|e| vec![Remark::new("write-failed", format!("couldn't land {}: {e}", landing.display()))])?;
        Ok(take)
    });
    match landed {
        Ok(take) => {
            let stats = &take.stats;
            let peak = stats["peak_dbfs"].as_f64().map_or("silent".to_owned(), |db| format!("peak {db} dBFS"));
            let mut sentence = format!("wrote {}: {}, {:.3}s, {peak}", landing.display(), stats["preset"].as_str().unwrap_or("sfx"), stats["duration_seconds"].as_f64().unwrap_or(0.0));
            if let Some(seed) = stats["seed"].as_u64() { sentence.push_str(&format!(", seed {seed}")); }
            for warning in &take.warnings { sentence.push_str(&format!("\nwarning: {}", warning.message)); }
            surface::emit(human, json!({ "ok": true, "verb": "sfx", "output": landing.display().to_string(), "stats": take.stats, "warnings": take.warnings.iter().map(Remark::json).collect::<Vec<_>>(), "errors": [] }), &sentence);
            Ok(())
        }
        Err(faults) => {
            if !human { surface::emit(false, json!({ "ok": false, "verb": "sfx", "output": null, "stats": null, "warnings": [], "errors": faults.iter().map(Remark::json).collect::<Vec<_>>() }), ""); }
            Err(faults.iter().map(|f| f.message.as_str()).collect::<Vec<_>>().join("; ").into())
        }
    }
}

fn read(args: &Args) -> Result<Recipe, Vec<Remark>> {
    let mut faults = Vec::new();
    let preset = PRESETS.iter().map(|(name, _)| *name).chain(["custom"]).find(|name| *name == args.preset);
    if preset.is_none() { faults.push(Remark::new("unknown-preset", format!("`{}` isn't a preset; pick coin, jump, hurt, explosion, powerup, blip, laser, or custom", args.preset))); }
    let roll = PRESETS.iter().find(|(name, _)| *name == args.preset).map(|&(_, roll)| roll);
    let wave = args.wave.as_deref().and_then(|raw| pick_wave(raw, &mut faults));
    let pitch = turn(&PITCH, args.pitch.as_deref(), &mut faults);
    let length = turn(&LENGTH, args.length.as_deref(), &mut faults).map(samples);
    let attack = turn(&ATTACK, args.attack.as_deref(), &mut faults).map(samples);
    let decay = turn(&DECAY, args.decay.as_deref(), &mut faults).map(samples);
    let sweep = turn(&SWEEP, args.sweep.as_deref(), &mut faults);
    let seed = args.seed.as_deref().and_then(|raw| raw.trim().parse::<u64>().map_err(|_| faults.push(Remark::new("bad-value", format!("--seed `{raw}` isn't a whole number from 0 up")))).ok());
    if preset == Some("custom") {
        if args.wave.is_none() { faults.push(Remark::new("missing-flag", "custom has no preset to inherit a wave from; add --wave square, saw, sine, or noise")); }
        if args.pitch.is_none() { faults.push(Remark::new("missing-flag", "custom has no preset to inherit a pitch from; add --pitch in Hz, like 440")); }
        if args.seed.is_some() { faults.push(Remark::new("seed-in-custom", "custom has no dice to roll, so --seed would do nothing; drop it")); }
    }
    match preset {
        Some(preset) if faults.is_empty() => Ok(Recipe { preset, roll, seed: roll.map(|_| seed.unwrap_or(FIRST_COIN)), wave, pitch, length, attack, decay, sweep }),
        _ => Err(faults),
    }
}

fn pick_wave(raw: &str, faults: &mut Vec<Remark>) -> Option<WaveType> {
    let name = raw.trim().to_ascii_lowercase();
    let found = WAVES.iter().find(|(menu, _)| *menu == name).map(|&(_, wave)| wave);
    if found.is_none() {
        let why = if name.starts_with("tri") { "sfxr's triangle is really a falling saw, so it's called `saw` here" } else { "pick square, saw, sine, or noise" };
        faults.push(Remark::new("bad-value", format!("--wave `{raw}` isn't on the menu; {why}")));
    }
    found
}

fn turn(dial: &Dial, raw: Option<&str>, faults: &mut Vec<Remark>) -> Option<f64> {
    let raw = raw?;
    let said = raw.trim().to_ascii_lowercase();
    let reading = dial.units.iter().find_map(|&(suffix, scale)| Some(said.strip_suffix(suffix)?.trim().parse::<f64>().ok()? * scale));
    match reading {
        Some(value) if value.is_finite() && (dial.low..=dial.high).contains(&value) => Some(value),
        Some(value) if value.is_finite() => {
            faults.push(Remark::new("out-of-range", format!("--{} {value}{} is out of reach; keep it between {} and {}{}", dial.flag, dial.unit, dial.low, dial.high, dial.unit)));
            None
        }
        _ => {
            faults.push(Remark::new("bad-value", format!("--{} `{raw}` isn't {}", dial.flag, dial.hint)));
            None
        }
    }
}

fn sound(args: &Args) -> Result<Take, Vec<Remark>> {
    let recipe = read(args)?;
    let mut warnings = Vec::new();
    let mut s = recipe.roll.map_or_else(blank, |roll| roll(recipe.seed));
    if s.arp_mod < 0.0 { s.arp_mod = 0.0; }
    let base = [count(s.env_attack).saturating_sub(1), count(s.env_sustain).max(1), count(s.env_decay).max(1)];
    let base_total: u32 = base.iter().sum();
    let repeat = (s.repeat_speed > 0.0).then(|| f64::from((1.0 - s.repeat_speed).powi(2)) * 20_000.0 + 32.0);
    let base_cycle = cycle(repeat, base_total);
    let wanted = recipe.sweep.unwrap_or_else(|| travel(&s, base_cycle));
    if let Some(hz) = recipe.pitch { transpose(&mut s, hz); }

    let [attack, sustain, decay] = shape(base, &recipe).map_err(|fault| vec![fault])?;
    let total = attack + sustain + decay;
    let stretch = f64::from(total) / f64::from(base_total);
    if s.arp_mod > 0.0 && s.arp_speed < 1.0 {
        let due = (f64::from((1.0 - s.arp_speed).powi(2)) * 20_000.0 + 32.0) * stretch;
        let held = due.clamp(33.0, 20_032.0);
        if (held - due).abs() >= 1.0 { warnings.push(Remark::new("arp-clamped", format!("the arpeggio wants to jump at {} ms, but sfxr can only schedule it between 0.75 and 454 ms; it jumps at {} ms", hundredths(ms(due)), hundredths(ms(held))))); }
        s.arp_speed = (1.0 - ((held - 32.0) / 20_000.0).sqrt()) as f32;
    }
    let repeat = repeat.map(|period| period * stretch);
    s.repeat_speed = repeat.map_or(0.0, |period| (1.0 - (period / 640_000.0).min(1.0).sqrt()) as f32);
    let cycle = cycle(repeat, total);
    let ramp = match recipe.sweep {
        Some(semitones) => {
            s.freq_limit = 0.0;
            (1.0 - 2f64.powf(-semitones / (12.0 * f64::from(cycle)))) / 0.01
        }
        None => (1.0 - slide(s.freq_ramp).powf(f64::from(base_cycle) / f64::from(cycle))) / 0.01,
    };
    s.freq_ramp = ramp.cbrt().clamp(-1.0, 1.0);
    let reached = travel(&s, cycle);
    if (reached - wanted).abs() > 0.05 { warnings.push(Remark::new("sweep-clamped", format!("the glide wants {wanted:+.2} semitones but sfxr only gets {reached:+.2} out of this sound"))); }
    if let Some(wave) = recipe.wave { s.wave_type = wave; }
    s.env_attack = stage_float(attack + 1);
    s.env_sustain = stage_float(sustain);
    s.env_decay = stage_float(decay);

    let wave = WAVES.iter().find(|(_, w)| *w == s.wave_type).map_or("square", |(name, _)| *name);
    let pitch_hz = TICKS / fperiod(s.base_freq).floor().max(8.0);
    let mut buffer = vec![0f32; total as usize];
    Generator::new(s).generate(&mut buffer);
    let samples: Vec<i16> = buffer.iter().map(|x| (f64::from(*x) * f64::from(i16::MAX)).round() as i16).collect();
    let peak = samples.iter().map(|x| x.unsigned_abs()).max().unwrap_or(0);
    let saturated = samples.iter().filter(|x| x.unsigned_abs() >= i16::MAX.unsigned_abs()).count();
    if saturated > 0 { warnings.push(Remark::new("saturation", format!("{saturated} samples sit on full scale; the sound is clipping"))); }
    if peak == 0 { warnings.push(Remark::new("silent", "the whole sound came out silent")); }
    let stats = json!({
        "preset": recipe.preset, "seed": recipe.seed, "wave": wave,
        "pitch_hz": hundredths(pitch_hz), "sweep_semitones": hundredths(reached),
        "attack_ms": hundredths(ms(f64::from(attack))), "sustain_ms": hundredths(ms(f64::from(sustain))), "decay_ms": hundredths(ms(f64::from(decay))),
        "duration_seconds": f64::from(total) / f64::from(RATE), "samples": total,
        "peak_dbfs": hundredths(20.0 * (f64::from(peak) / f64::from(i16::MAX)).log10()), "saturated_samples": saturated,
    });
    Ok(Take { samples, stats, warnings })
}

fn blank() -> Sample {
    let mut s = Sample::new();
    s.env_attack = 0.0;
    s.env_sustain = stage_float(samples(100.0));
    s.env_decay = stage_float(samples(200.0));
    s
}

fn shape(base: [u32; 3], recipe: &Recipe) -> Result<[u32; 3], Remark> {
    let pins = [recipe.attack, None, recipe.decay];
    let floored = |[attack, sustain, decay]: [u32; 3]| [attack, sustain.max(1), decay.max(1)];
    let Some(length) = recipe.length else { return Ok(floored([recipe.attack.unwrap_or(base[0]), base[1], recipe.decay.unwrap_or(base[2])])) };
    let pinned: u32 = pins.iter().flatten().sum();
    let Some(budget) = length.checked_sub(pinned) else {
        let named = pins.iter().zip(STAGES).filter_map(|(pin, stage)| pin.map(|n| format!("--{stage} {} ms", hundredths(ms(f64::from(n)))))).collect::<Vec<_>>().join(" + ");
        return Err(Remark::new("envelope-overflow", format!("{named} run {} ms, past --length {} ms", hundredths(ms(f64::from(pinned))), hundredths(ms(f64::from(length))))));
    };
    let free = [0, 1, 2].map(|i| if pins[i].is_none() { u64::from(base[i]) } else { 0 });
    let weight: u64 = free.iter().sum();
    let mut stages: [u32; 3] = std::array::from_fn(|i| pins[i].unwrap_or_else(|| (u64::from(budget) * free[i]).checked_div(weight).unwrap_or(0) as u32));
    stages[1] = length - stages[0] - stages[2];
    if let Some(i) = (0..3).find(|&i| stages[i] >= STAGE_CAP) {
        let heaviest = free.iter().copied().max().unwrap_or(0);
        let reach = if heaviest == 0 { f64::from(pinned + STAGE_CAP - 1) } else { f64::from(pinned) + weight as f64 * f64::from(STAGE_CAP - 1) / heaviest as f64 };
        return Err(Remark::new("stage-too-long", format!("--length {} ms would stretch the {} past {STAGE_MS} ms, the longest stage sfxr can hold; this shape reaches {} ms, or pin --attack and --decay to go further", hundredths(ms(f64::from(length))), STAGES[i], ms(reach).floor())));
    }
    Ok(floored(stages))
}

fn transpose(s: &mut Sample, hz: f64) {
    let period = (TICKS / hz).round() + 0.5;
    let ratio = period / fperiod(s.base_freq);
    if s.freq_limit > 0.0 { s.freq_limit = (100.0 / (fperiod(s.freq_limit) * ratio) - 0.001).max(0.0).sqrt(); }
    s.base_freq = (100.0 / period - 0.001).sqrt();
}

fn travel(s: &Sample, cycle: u32) -> f64 {
    let start = fperiod(s.base_freq);
    let end = (start * slide(s.freq_ramp).powf(f64::from(cycle))).min(fperiod(s.freq_limit)).max(8.0);
    12.0 * (start / end).log2()
}

fn cycle(repeat: Option<f64>, total: u32) -> u32 { repeat.map(|period| period as u32).filter(|period| (1..total).contains(period)).unwrap_or(total) }

fn fperiod(knob: f64) -> f64 { 100.0 / (knob.powi(2) + 0.001) }

fn slide(ramp: f64) -> f64 { 1.0 - ramp.powi(3) * 0.01 }

fn count(stage: f32) -> u32 { (stage.powi(2) * 100_000.0) as u32 }

fn stage_float(samples: u32) -> f32 { (((f64::from(samples) + 0.5) / f64::from(STAGE_CAP)).sqrt() as f32).min(1.0) }

fn samples(ms: f64) -> u32 { (ms * f64::from(RATE) / 1000.0).round() as u32 }

fn ms(samples: f64) -> f64 { samples * 1000.0 / f64::from(RATE) }

fn hundredths(x: f64) -> f64 { (x * 100.0).round() / 100.0 }

fn pressed(samples: &[i16]) -> Result<Vec<u8>, hound::Error> { surface::pressed(samples, RATE) }

fn publish(path: &Path, bytes: &[u8]) -> std::io::Result<()> { surface::publish(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::fs;

    #[derive(Parser)]
    struct Probe {
        #[command(flatten)]
        args: Args,
    }

    fn args(line: &str) -> Args { Probe::try_parse_from(std::iter::once("piro").chain(line.split_whitespace())).unwrap_or_else(|e| panic!("`{line}` doesn't parse: {e}")).args }

    fn take(line: &str) -> Take { sound(&args(line)).unwrap_or_else(|faults| panic!("`{line}` refused: {faults:?}")) }

    fn refusals(line: &str) -> Vec<&'static str> { sound(&args(line)).err().unwrap_or_else(|| panic!("`{line}` should have been refused")).iter().map(|f| f.code).collect() }

    fn upward_crossings(samples: &[i16]) -> usize { samples.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count() }

    #[test]
    fn the_same_command_presses_the_same_bytes() {
        for (preset, _) in PRESETS {
            let first = pressed(&take(preset).samples).unwrap();
            let second = pressed(&take(preset).samples).unwrap();
            assert_eq!(first, second, "{preset} came out different twice");
            assert_eq!(take(preset).stats["seed"], 9);
        }
        assert_ne!(take("coin").samples, take("coin --seed 7").samples);
        assert_eq!(take("coin --seed 9").samples, take("coin").samples);
    }

    #[test]
    fn stage_counts_survive_the_float_round_trip() {
        for n in 0..=STAGE_CAP { assert_eq!(count(stage_float(n)), n, "stage of {n} samples"); }
    }

    #[test]
    fn a_pitch_in_hz_cycles_that_many_times_a_second() {
        for hz in [55.0, 440.0, 1000.0, 3000.0] {
            let one_second = take(&format!("custom --wave sine --pitch {hz} --length 1s --decay 0"));
            let period = (TICKS / hz).round();
            assert_eq!(one_second.stats["pitch_hz"], hundredths(TICKS / period));
            let heard = upward_crossings(&one_second.samples) as f64;
            assert!((heard - TICKS / period).abs() <= 2.0, "{hz} Hz crossed zero {heard} times in a second");
        }
    }

    #[test]
    fn a_sweep_of_twelve_lands_an_octave_up() {
        let glide = take("custom --wave sine --pitch 440 --sweep +12 --length 1s --decay 0");
        assert_eq!(glide.stats["sweep_semitones"], 12.0);
        let last_tenth = upward_crossings(&glide.samples[glide.samples.len() - 4410..]);
        assert!((83..=88).contains(&last_tenth), "the last tenth of a second crossed {last_tenth} times, wants about 85");
        assert_eq!(take("custom --wave sine --pitch 440 --sweep -7 --length 1s").stats["sweep_semitones"], -7.0);
    }

    #[test]
    fn attack_and_decay_cannot_outgrow_the_length() {
        let overflow = sound(&args("coin --length 300ms --attack 200ms --decay 200ms")).err().unwrap();
        assert_eq!(overflow, vec![Remark::new("envelope-overflow", "--attack 200 ms + --decay 200 ms run 400 ms, past --length 300 ms")]);
        let snug = take("coin --length 300ms --attack 100ms --decay 200ms");
        assert_eq!(snug.stats["saturated_samples"], 0, "an empty sustain leaked a NaN into the filters");
        assert_eq!((snug.stats["attack_ms"].as_f64(), snug.stats["decay_ms"].as_f64(), snug.samples.len()), (Some(100.0), Some(200.0), 13_231));
    }

    #[test]
    fn length_alone_stretches_the_whole_shape() {
        let coin = take("coin");
        let double = take(&format!("coin --length {}ms", coin.stats["duration_seconds"].as_f64().unwrap() * 2000.0));
        for stage in ["sustain_ms", "decay_ms"] {
            let (was, now) = (coin.stats[stage].as_f64().unwrap(), double.stats[stage].as_f64().unwrap());
            assert!((now / was - 2.0).abs() < 0.01, "{stage} went {was} -> {now}");
        }
        let jump = take("jump");
        let short_jump = take("jump --length 60ms");
        assert_eq!(jump.stats["sweep_semitones"], short_jump.stats["sweep_semitones"]);
        assert_eq!(short_jump.samples.len(), 2646);
    }

    #[test]
    fn pinning_without_a_length_keeps_the_sustain() {
        let coin = take("coin");
        let pinned = take("coin --attack 30ms");
        assert_eq!(pinned.stats["attack_ms"], 30.0);
        assert_eq!(pinned.stats["sustain_ms"], coin.stats["sustain_ms"]);
        assert_eq!(pinned.stats["decay_ms"], coin.stats["decay_ms"]);
    }

    #[test]
    fn a_stretch_too_far_names_its_reach() {
        let fault = sound(&args("explosion --length 6000ms")).err().unwrap();
        assert_eq!(fault[0].code, "stage-too-long");
        assert!(fault[0].message.contains("this shape reaches"), "{}", fault[0].message);
    }

    #[test]
    fn custom_needs_a_wave_and_a_pitch_and_no_dice() {
        assert_eq!(refusals("custom"), ["missing-flag", "missing-flag"]);
        assert_eq!(refusals("custom --wave sine --pitch 440 --seed 3"), ["seed-in-custom"]);
        let plain = take("custom --wave square --pitch 440");
        assert_eq!((plain.stats["seed"].clone(), plain.stats["duration_seconds"].as_f64()), (Value::Null, Some(0.3)));
    }

    #[test]
    fn the_wave_menu_names_what_comes_out() {
        let fault = sound(&args("blip --wave triangle")).err().unwrap();
        assert!(fault[0].message.contains("`saw`"), "{}", fault[0].message);
        assert_eq!(take("blip --wave saw").stats["wave"], "saw");
        assert_eq!(take("explosion").stats["wave"], "noise");
    }

    #[test]
    fn bad_numbers_are_refused_all_at_once() {
        assert_eq!(refusals("laser --pitch nan --length banana --sweep 99 --seed -1"), ["bad-value", "bad-value", "out-of-range", "bad-value"]);
        assert_eq!(refusals("laser --pitch inf --attack 3000ms"), ["bad-value", "out-of-range"]);
        assert_eq!(refusals("kazoo --pitch 2"), ["unknown-preset", "out-of-range"]);
        assert_eq!(take("laser --pitch 880hz --length 0.25s --sweep -12st").stats["sweep_semitones"], -12.0);
    }

    #[test]
    fn transposing_moves_the_floor_along() {
        let laser = take("laser");
        let low = take("laser --pitch 400");
        assert!((laser.stats["sweep_semitones"].as_f64().unwrap() - low.stats["sweep_semitones"].as_f64().unwrap()).abs() < 0.1);
        assert!(low.warnings.is_empty(), "{:?}", low.warnings);
    }

    #[test]
    fn every_preset_and_seed_renders_sane() {
        for (preset, _) in PRESETS {
            for seed in 0..40 {
                let roll = take(&format!("{preset} --seed {seed}"));
                assert!(roll.samples.iter().any(|x| *x != 0), "{preset} seed {seed} is silent");
                assert!(roll.warnings.iter().all(|w| w.code != "sweep-clamped"), "{preset} seed {seed}: {:?}", roll.warnings);
            }
        }
    }

    #[test]
    fn publishing_lands_whole_or_not_at_all() {
        let dir = std::env::temp_dir().join(format!("piro-sfx-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("coin.wav");
        publish(&target, b"first").unwrap();
        publish(&target, b"second").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"second");
        assert!(publish(&dir, b"third").is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "a draft was left behind");
        fs::remove_dir_all(&dir).unwrap();
    }
}
