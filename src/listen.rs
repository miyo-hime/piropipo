use crate::harmony::{self, Rub};
use crate::surface;
use crate::vu::{self, Reading};
use piropipo::profile::{self, Breach, Charge};
use piropipo::render::{self, RenderError, SAMPLE_RATE, Stencils};
use piropipo::score::{Bend, COMMON_TIME, Cue, Lane, Note, Pattern, Position, ScoreError, Song};
use serde_json::{Value, json};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

const CAP_SECONDS: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail { Headline, Deposition, Everything }

#[derive(Debug, Clone, PartialEq)]
pub struct Remark { pub code: &'static str, pub message: String, pub pattern: Option<String>, pub lane: Option<String>, pub at: Option<Position>, pub line: Option<usize> }

impl Remark {
    pub fn loose(code: &'static str, message: impl Into<String>) -> Self { Remark { code, message: message.into(), pattern: None, lane: None, at: None, line: None } }

    fn json(&self) -> Value { json!({ "code": self.code, "message": self.message, "pattern": self.pattern, "lane": self.lane, "at": self.at.map(|at| at.to_string()), "line": self.line }) }
}

impl fmt::Display for Remark {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut place = Vec::new();
        if let Some(n) = self.line { place.push(format!("line {n}")); }
        if let Some(p) = &self.pattern { place.push(format!("pattern {p}")); }
        if let Some(l) = &self.lane { place.push(format!("lane {l}")); }
        if let Some(at) = self.at { place.push(format!("at {at}")); }
        if place.is_empty() { write!(f, "{}", self.message) } else { write!(f, "{}: {}", place.join(", "), self.message) }
    }
}

impl From<ScoreError> for Remark {
    fn from(e: ScoreError) -> Self { Remark { code: e.fault.code(), message: e.fault.to_string(), pattern: e.pattern, lane: e.lane, at: e.at, line: e.line } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring { pub at: Position, pub end: Position, pub rest: bool }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swing { pub shift: i8, pub ticks: u8, pub up: f64, pub down: f64 }

#[derive(Debug, Clone, PartialEq)]
pub enum Flourish {
    Vibrato { at: Position, hertz: f64, delay_ms: f64, swings: Vec<Swing> },
    Arpeggio { at: Position, steps: u64, rate: f64 },
}

impl Flourish {
    fn json(&self) -> Value {
        match self {
            Flourish::Vibrato { at, hertz, delay_ms, swings } => {
                let swings: Vec<Value> = swings.iter().map(|s| json!({ "transpose": s.shift, "ticks": s.ticks, "up_cents": round(s.up, 2), "down_cents": round(s.down, 2) })).collect();
                json!({ "at": at.to_string(), "effect": "vib", "rate_hz": round(*hertz, 3), "delay_ms": round(*delay_ms, 1), "swings": swings })
            }
            Flourish::Arpeggio { at, steps, rate } => json!({ "at": at.to_string(), "effect": "arp", "steps": steps, "steps_per_second": round(*rate, 2) }),
        }
    }

    fn tell(&self) -> String {
        match self {
            Flourish::Vibrato { at, hertz, delay_ms, swings } => {
                let swings: Vec<String> = swings.iter().map(|s| format!("{}+{:.1}/-{:.1} cents ({} tick{})", if s.shift == 0 { String::new() } else { format!("played {:+}, ", s.shift) }, s.up, s.down, s.ticks, if s.ticks == 1 { "" } else { "s" })).collect();
                format!("{at} vib: {} Hz after {delay_ms:.0} ms, swings {}", round(*hertz, 3), swings.join("; "))
            }
            Flourish::Arpeggio { at, steps, rate } => format!("{at} arp: {steps} steps, {rate:.1} a second"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Solo { pub pattern: String, pub lane: String, pub line: usize, pub sounding: bool, pub notes: Vec<Ring>, pub flourishes: Vec<Flourish> }

impl Solo {
    fn voiced(&self) -> bool { self.notes.iter().any(|r| !r.rest) }

    fn json(&self) -> Value {
        let notes: Vec<Value> = self.notes.iter().map(|r| json!({ "at": r.at.to_string(), "end": r.end.to_string(), "rest": r.rest })).collect();
        json!({ "pattern": self.pattern, "lane": self.lane, "sounding": self.sounding, "notes": notes, "effects": self.flourishes.iter().map(Flourish::json).collect::<Vec<_>>() })
    }

    fn tell(&self) -> String {
        let state = match (self.sounding, self.voiced()) { (true, _) => "sounds", (false, true) => "renders silent", (false, false) => "carries no pitched notes" };
        let rings: Vec<String> = self.notes.iter().map(|r| format!("{}{} through {}", if r.rest { "rest " } else { "" }, r.at, r.end)).collect();
        let head = if rings.is_empty() { format!("pattern {}, lane {} {state}", self.pattern, self.lane) } else { format!("pattern {}, lane {} {state}: {}", self.pattern, self.lane, rings.join(", ")) };
        std::iter::once(head).chain(self.flourishes.iter().map(|f| format!("  {}", f.tell()))).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stats { pub seconds: f64, pub peak_dbfs: Option<f64>, pub saturated: usize, pub lanes: Vec<Solo>, pub gauges: Option<Reading> }

impl Stats {
    fn headline(&self) -> Value { json!({ "duration_seconds": self.seconds, "peak_dbfs": self.peak_dbfs, "saturated_samples": self.saturated }) }

    fn json(&self) -> Value {
        let mut stats = self.headline();
        if let (Some(fields), Some(Value::Object(gauged))) = (stats.as_object_mut(), self.gauges.as_ref().map(Reading::json)) { fields.extend(gauged); }
        stats["lanes"] = self.lanes.iter().map(Solo::json).collect();
        stats
    }
}

pub struct Take { pub samples: Vec<i16>, pub stats: Stats, pub warnings: Vec<Remark>, pub rubs: Vec<Rub>, pub song: Song, pub plays: Vec<Range<usize>> }

pub fn read(input: &str) -> Result<String, Vec<Remark>> {
    surface::read_source(input).map(|(text, _)| text).map_err(|e| vec![Remark::loose("unreadable-input", format!("can't read {input}: {e}"))])
}

pub fn take(text: &str) -> Result<Take, Vec<Remark>> {
    let song = Song::parse(text).map_err(|errors| errors.into_iter().map(Remark::from).collect::<Vec<_>>())?;
    if let Some(remark) = overlong(&song) { return Err(vec![remark]); }
    let mut samples = Vec::new();
    let mut plays = Vec::new();
    let mut lanes = Vec::new();
    let mut stencils = Stencils::default();
    for (i, cue) in song.setlist.iter().enumerate() {
        let pattern = song.pattern(&cue.pattern).expect("the copyist refuses a play line that names no pattern");
        let from = samples.len();
        samples.extend(render::transposed(&song, pattern, cue).and_then(|moved| render::pattern_with(&song, &moved, &mut stencils)).map_err(|e| vec![misheard(e, pattern)])?);
        plays.push(from..samples.len());
        if song.setlist.iter().take(i).any(|c| c.pattern == cue.pattern) { continue; }
        let mut shifts: Vec<i8> = song.setlist.iter().filter(|c| c.pattern == cue.pattern).map(|c| c.shift).collect();
        shifts.sort_unstable();
        shifts.dedup();
        for lane in &pattern.lanes { lanes.push(solo(&song, pattern, cue, lane, &shifts)?); }
    }
    let haunted = ghosts(&song);
    if !haunted.is_empty() { return Err(haunted); }
    let stats = Stats { seconds: round(samples.len() as f64 / f64::from(SAMPLE_RATE), 3), peak_dbfs: peak_dbfs(&samples), saturated: saturated(&samples), lanes, gauges: None };
    let mut warnings: Vec<Remark> = stats.lanes.iter().filter_map(hush).collect();
    if stats.saturated > 0 { warnings.push(Remark::loose("saturation", format!("{} sample{} sit at full scale; the mix is hitting the rail and may be clipping", stats.saturated, if stats.saturated == 1 { "" } else { "s" }))); }
    Ok(Take { samples, stats, warnings, rubs: harmony::rubs(&song), song, plays })
}

pub fn answer(verb: &str, human: bool, detail: Detail, played: Result<(Option<PathBuf>, Take), Vec<Remark>>) -> Result<(), Box<dyn std::error::Error>> {
    match played {
        Ok((output, mut take)) => {
            if detail == Detail::Everything { take.stats.gauges = Some(vu::read(&take.song, &take.samples, &take.plays)); }
            let stats = if detail == Detail::Headline { take.stats.headline() } else { take.stats.json() };
            let mut envelope = json!({ "ok": true, "verb": verb, "output": output.as_ref().map(|p| p.display().to_string()), "stats": stats, "warnings": take.warnings.iter().map(Remark::json).collect::<Vec<_>>() });
            if detail == Detail::Everything { envelope["harmony"] = harmony::json(&take.rubs); }
            envelope["errors"] = json!([]);
            let mut told = tell(output.as_deref(), &take.stats, detail, &take.warnings);
            if detail == Detail::Everything { told = format!("{told}\n{}", harmony::tell(&take.rubs)); }
            surface::emit(human, envelope, &told);
            Ok(())
        }
        Err(errors) => {
            if !human { println!("{}", json!({ "ok": false, "verb": verb, "output": null, "stats": null, "warnings": [], "errors": errors.iter().map(Remark::json).collect::<Vec<_>>() })); }
            Err(match &errors[..] {
                [one] => one.to_string(),
                many => format!("{} errors\n{}", many.len(), many.iter().map(|e| format!("  {e}")).collect::<Vec<_>>().join("\n")),
            }.into())
        }
    }
}

fn tell(output: Option<&Path>, stats: &Stats, detail: Detail, warnings: &[Remark]) -> String {
    let head = output.map_or("checked, nothing written".to_owned(), |p| format!("wrote {}", p.display()));
    let peak = stats.peak_dbfs.map_or("silent throughout".to_owned(), |p| format!("peak {p:.2} dbfs"));
    let rail = match stats.saturated { 0 => "no samples at full scale".to_owned(), 1 => "1 sample at full scale".to_owned(), n => format!("{n} samples at full scale") };
    let mut lines = vec![format!("{head}: {:.3} seconds, {peak}, {rail}", stats.seconds)];
    if detail != Detail::Headline {
        lines.extend(stats.gauges.as_ref().map(Reading::tell));
        lines.extend(stats.lanes.iter().map(Solo::tell));
    }
    lines.extend(warnings.iter().map(|w| format!("warning: {w}")));
    lines.join("\n")
}

fn overlong(song: &Song) -> Option<Remark> {
    let played: Vec<&Pattern> = song.setlist.iter().filter_map(|cue| song.pattern(&cue.pattern)).collect();
    let bars: u64 = played.iter().map(|p| u64::from(p.bars)).fold(0, u64::saturating_add);
    let beats: u64 = played.iter().map(|p| u64::from(p.bars) * u64::from(p.beats)).fold(0, u64::saturating_add);
    let tempo = u64::from(song.tempo);
    let meter = if beats == bars.saturating_mul(COMMON_TIME.into()) { String::new() } else { format!(", {beats} beats") };
    (beats.saturating_mul(60) > CAP_SECONDS * tempo).then(|| Remark::loose("too-long", format!("the song runs {:.1} seconds ({bars} bar{}{meter} at tempo {tempo}), past the {} minute cap", beats.saturating_mul(60) as f64 / tempo as f64, if bars == 1 { "" } else { "s" }, CAP_SECONDS / 60)))
}

fn ghosts(song: &Song) -> Vec<Remark> {
    let Ok(card) = profile::card(song.profile.as_deref()) else { return Vec::new() };
    let unplayed = song.patterns.iter().filter(|p| !song.setlist.iter().any(|c| c.pattern == p.name));
    unplayed.flat_map(|p| {
        let charges: Vec<Remark> = card.audit(p).into_iter().map(|charge| indicted(p, &charge)).collect();
        if !charges.is_empty() { return charges; }
        render::vet(song, p).err().map(|e| misheard(e, p)).into_iter().collect()
    }).collect()
}

fn indicted(pattern: &Pattern, charge: &Charge) -> Remark {
    let (code, at) = match &charge.breach {
        Breach::Profile(_) => ("unknown-profile", None),
        Breach::Lane(_) => ("unknown-lane", None),
        Breach::Param { .. } => ("unknown-param", None),
        Breach::Volume(_) => ("no-volume-knob", None),
        Breach::Duty(_) => ("bad-duty", None),
        Breach::Loudness(_) => ("bad-volume", None),
        Breach::Range { at, .. } => ("out-of-range", Some(*at)),
        Breach::Ornament { at, flaw } => (flaw.code(), Some(*at)),
    };
    let line = charge.note.map_or(charge.lane.line, |n| n.line);
    Remark { code, message: charge.breach.to_string(), pattern: Some(pattern.name.clone()), lane: Some(charge.lane.name.clone()), at, line: Some(line) }
}

fn solo(song: &Song, pattern: &Pattern, cue: &Cue, lane: &Lane, shifts: &[i8]) -> Result<Solo, Vec<Remark>> {
    let mut notes: Vec<Ring> = lane.notes.iter().map(|n| Ring { at: n.at, end: n.rings_through(pattern.grid), rest: n.pitch.is_none() }).collect();
    notes.sort_by_key(|r| r.at);
    let mut flourishes: Vec<Flourish> = lane.notes.iter().filter_map(|n| flourish(song.tempo, pattern, n, shifts)).collect();
    flourishes.sort_by_key(|f| match f { Flourish::Vibrato { at, .. } | Flourish::Arpeggio { at, .. } => *at });
    let mut heard = Solo { pattern: pattern.name.clone(), lane: lane.name.clone(), line: lane.line, sounding: false, notes, flourishes };
    if heard.voiced() {
        let alone = Pattern { name: pattern.name.clone(), lanes: vec![lane.clone()], ..*pattern };
        heard.sounding = render::transposed(song, &alone, cue).and_then(|moved| render::pattern(song, &moved)).map_err(|e| vec![misheard(e, pattern)])?.iter().any(|s| *s != 0);
    }
    Ok(heard)
}

fn flourish(tempo: u32, pattern: &Pattern, note: &Note, shifts: &[i8]) -> Option<Flourish> {
    let pitch = note.pitch?;
    match note.ornaments.bend.as_ref()? {
        Bend::Vibrato(vib) => {
            let swings = shifts.iter().filter_map(|&shift| {
                let played = u8::try_from(i16::from(pitch) + i16::from(shift)).ok()?;
                let ticks = profile::ticks(played, vib.cents);
                let (up, down) = profile::swing(played, ticks);
                Some(Swing { shift, ticks: u8::try_from(ticks as u64).ok()?, up, down })
            }).collect();
            Some(Flourish::Vibrato { at: note.at, hertz: vib.hertz(), delay_ms: vib.delay_ms(), swings })
        }
        Bend::Arpeggio(_) => {
            let (steps, rate) = render::arp_rate(tempo, pattern, note.len)?;
            Some(Flourish::Arpeggio { at: note.at, steps, rate })
        }
        Bend::Slide(_) => None,
    }
}

fn hush(solo: &Solo) -> Option<Remark> {
    (solo.voiced() && !solo.sounding).then(|| Remark { code: "silent-lane", message: "this lane has notes but renders silent".to_owned(), pattern: Some(solo.pattern.clone()), lane: Some(solo.lane.clone()), at: None, line: Some(solo.line) })
}

pub fn peak_dbfs(samples: &[i16]) -> Option<f64> {
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().filter(|p| *p > 0)?.min(i16::MAX.unsigned_abs());
    Some(round(20.0 * (f64::from(peak) / f64::from(i16::MAX)).log10(), 2))
}

fn saturated(samples: &[i16]) -> usize { samples.iter().filter(|s| **s == i16::MAX || **s == i16::MIN).count() }

pub fn round(x: f64, places: i32) -> f64 { let k = 10f64.powi(places); (x * k).round() / k }

fn misheard(e: RenderError, pattern: &Pattern) -> Remark {
    let (code, lane, at) = match &e {
        RenderError::Profile(_) => ("unknown-profile", None, None),
        RenderError::Tempo(_) => ("tempo-out-of-reach", None, None),
        RenderError::Grid(_) | RenderError::Meter { .. } => ("grid-out-of-reach", None, None),
        RenderError::Beats(_) => ("beats-out-of-reach", None, None),
        RenderError::Lane(lane) => ("unknown-lane", Some(lane.clone()), None),
        RenderError::Param { lane, .. } => ("unknown-param", Some(lane.clone()), None),
        RenderError::Volume { lane, .. } => ("no-volume-knob", Some(lane.clone()), None),
        RenderError::Duty { lane, .. } => ("bad-duty", Some(lane.clone()), None),
        RenderError::Loudness { lane, .. } => ("bad-volume", Some(lane.clone()), None),
        RenderError::Range { lane, at, .. } => ("out-of-range", Some(lane.clone()), Some(*at)),
        RenderError::Shifted { lane, at, .. } => ("transposed-out-of-range", Some(lane.clone()), Some(*at)),
        RenderError::Ornament { lane, at, flaw, .. } => (flaw.code(), Some(lane.clone()), Some(*at)),
        RenderError::Misfit { lane, at } => ("misfit", Some(lane.clone()), Some(*at)),
        RenderError::Mml(_) => ("renderer-bug", None, None),
    };
    let song_wide = matches!(e, RenderError::Profile(_) | RenderError::Tempo(_));
    let written = lane.as_ref().and_then(|name| pattern.lanes.iter().find(|l| l.name == *name));
    let note_line = written.and_then(|l| l.notes.iter().find(|n| Some(n.at) == at)).map(|n| n.line);
    let line = note_line.or(written.map(|l| l.line)).or((!song_wide).then_some(pattern.line));
    let said = e.to_string();
    let prefix = match (&lane, at) { (Some(l), Some(a)) => format!("lane {l}, at {a}: "), (Some(l), None) => format!("lane {l}: "), _ => String::new() };
    let message = said.strip_prefix(&prefix).unwrap_or(&said).to_owned();
    Remark { code, message, pattern: (!song_wide).then(|| pattern.name.clone()), lane, at, line }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_example() -> &'static str {
        let doc = include_str!("../skills/piropipo/references/format.md");
        let (_, opened) = doc.split_once("```\n").expect("format.md lost its example");
        opened.split_once("```").expect("format.md example fence never closes").0
    }

    fn pos(bar: u32, slot: u32) -> Position { Position { bar, slot } }

    fn refusal(text: &str) -> Remark {
        let Err(errors) = take(text) else { panic!("expected a refusal") };
        let [one] = &errors[..] else { panic!("expected one error, got {errors:?}") };
        one.clone()
    }

    #[test]
    fn spec_example_is_measured() {
        let heard = take(spec_example()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(heard.samples.len(), 189_000);
        assert_eq!(heard.stats.seconds, 4.286);
        let peak = heard.stats.peak_dbfs.expect("the example is not silent");
        assert!((-20.0..0.0).contains(&peak), "peak {peak} dbfs");
        assert_eq!(heard.stats.saturated, 0);
        assert!(heard.warnings.is_empty(), "{:?}", heard.warnings);
        let names: Vec<(&str, &str, bool)> = heard.stats.lanes.iter().map(|s| (s.pattern.as_str(), s.lane.as_str(), s.sounding)).collect();
        assert_eq!(names, [("main", "pulse1", true), ("main", "pulse2", true), ("main", "triangle", true)]);
        assert_eq!(heard.stats.lanes[0].notes[5], Ring { at: pos(1, 13), end: pos(1, 16), rest: true });
        let bass: Vec<(Position, Position)> = heard.stats.lanes[2].notes.iter().map(|r| (r.at, r.end)).collect();
        assert_eq!(bass, [(pos(1, 1), pos(1, 8)), (pos(1, 9), pos(2, 8)), (pos(2, 9), pos(2, 16))]);
    }

    #[test]
    fn ends_come_back_in_position_order_not_line_order() {
        let heard = take("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n  1.10: C4 x2\n  1.5: D4 x2\n  1.1: E4 x2\n").unwrap();
        let starts: Vec<String> = heard.stats.lanes[0].notes.iter().map(|r| r.at.to_string()).collect();
        assert_eq!(starts, ["1.1", "1.5", "1.10"]);
    }

    #[test]
    fn rests_and_empty_lanes_stay_quiet_without_a_warning() {
        let heard = take("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x4\nlane pulse2\n  1: - x4\nlane noise\n").unwrap();
        let sounding: Vec<bool> = heard.stats.lanes.iter().map(|s| s.sounding).collect();
        assert_eq!(sounding, [true, false, false]);
        assert!(heard.warnings.is_empty(), "{:?}", heard.warnings);
    }

    #[test]
    fn a_voiced_lane_that_renders_silent_is_warned_with_its_place() {
        let mute = Solo { pattern: "main".into(), lane: "noise".into(), line: 9, sounding: false, notes: vec![Ring { at: pos(1, 1), end: pos(1, 4), rest: false }], flourishes: Vec::new() };
        let warned = hush(&mute).expect("a voiced silent lane should warn");
        assert_eq!((warned.code, warned.pattern.as_deref(), warned.lane.as_deref(), warned.at, warned.line), ("silent-lane", Some("main"), Some("noise"), None, Some(9)));
        assert_eq!(hush(&Solo { sounding: true, ..mute.clone() }), None);
        assert_eq!(hush(&Solo { notes: vec![Ring { at: pos(1, 1), end: pos(1, 4), rest: true }], ..mute }), None);
    }

    #[test]
    fn peak_is_measured_against_full_scale() {
        assert_eq!(peak_dbfs(&[]), None);
        assert_eq!(peak_dbfs(&[0, 0, 0]), None);
        assert_eq!(peak_dbfs(&[i16::MAX]), Some(0.0));
        assert_eq!(peak_dbfs(&[i16::MIN]), Some(0.0));
        assert_eq!(peak_dbfs(&[0, 16_384, -100]), Some(-6.02));
        assert_eq!(peak_dbfs(&[3_277, -3_277]), Some(-20.0));
    }

    #[test]
    fn saturation_counts_rail_hits_on_both_rails() {
        assert_eq!(saturated(&[i16::MAX, i16::MIN, i16::MAX - 1, i16::MIN + 1, 0]), 2);
        assert_eq!(saturated(&[]), 0);
    }

    #[test]
    fn the_cap_is_ten_minutes_inclusive() {
        let song = |bars: u32, tempo: u32| Song::parse(&format!("song \"s\" tempo {tempo}\npattern a bars {bars} grid 4\npattern b bars {bars} grid 4\n")).unwrap();
        assert_eq!(overlong(&song(150, 120)), None);
        let over = overlong(&song(151, 120)).expect("604 seconds is past the cap");
        assert_eq!((over.code, over.message.as_str()), ("too-long", "the song runs 604.0 seconds (302 bars at tempo 120), past the 10 minute cap"));
    }

    #[test]
    fn the_cap_refuses_before_a_single_sample_is_allocated() {
        let over = refusal("song \"s\" tempo 1\npattern endless bars 4000000000 grid 255\nlane pulse1\n  1.1: C4 x1\n");
        assert_eq!(over.code, "too-long");
        assert!(over.message.starts_with("the song runs 960000000000.0 seconds"), "{}", over.message);
    }

    #[test]
    fn patterns_play_back_to_back_in_written_order() {
        let heard = take("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern b bars 2 grid 8\nlane triangle\n  1: - x8\n").unwrap();
        assert_eq!(heard.stats.seconds, 6.0);
        assert!(heard.samples[..88_200].iter().any(|s| *s != 0));
        assert!(heard.samples[88_200..].iter().all(|s| *s == 0));
    }

    const DUO: &str = "song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern b bars 1 grid 4\nlane triangle\n  1: - x4\n";

    #[test]
    fn the_play_block_is_what_plays() {
        let once = take(&format!("{DUO}play\n  a\n")).unwrap();
        let heard = take(&format!("{DUO}play\n  b\n  a\n  a\n")).unwrap();
        assert_eq!(heard.stats.seconds, 6.0);
        assert_eq!(heard.samples, [vec![0; 88_200], once.samples.clone(), once.samples.clone()].concat());
        let soloed: Vec<(&str, &str)> = heard.stats.lanes.iter().map(|s| (s.pattern.as_str(), s.lane.as_str())).collect();
        assert_eq!(soloed, [("b", "triangle"), ("a", "pulse1")], "one solo per played pattern, in the order they first play");
        assert_eq!(once.stats.lanes.len(), 1, "an unplayed pattern isn't in the wav, so it isn't in the stats");
    }

    #[test]
    fn transpose_moves_the_pulse_and_the_noise_stays_put() {
        let grid = |pulse: &str, noise: &str, play: &str| format!("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  1: {pulse} x4\nlane noise\n  1: {noise} x4\n{play}");
        let moved = take(&grid("C4", "C4", "play\n  a +5\n")).unwrap().samples;
        assert_eq!(moved, take(&grid("F4", "C4", "")).unwrap().samples);
        assert_ne!(moved, take(&grid("F4", "F4", "")).unwrap().samples);
    }

    #[test]
    fn a_transpose_that_falls_off_the_chip_is_refused_at_the_note() {
        let off = refusal("song \"s\" tempo 120\npattern bass bars 1 grid 4\nlane triangle\n  1: C1 x4\nplay\n  bass\n  bass -1\n");
        assert_eq!((off.code, off.pattern.as_deref(), off.lane.as_deref(), off.at, off.line), ("transposed-out-of-range", Some("bass"), Some("triangle"), Some(pos(1, 1)), Some(4)));
        assert_eq!(off.to_string(), "line 4, pattern bass, lane triangle, at 1.1: C1 transposed -1 (play line 7) lands on B0, out of range; this lane plays C1 through B6");
        assert!(take("song \"s\" tempo 120\npattern bass bars 1 grid 4\nlane triangle\n  1: B0 x4\nplay\n  bass +1\n").is_ok(), "the pitch that plays is the one the card judges");
    }

    #[test]
    fn an_unplayed_pattern_still_answers_to_the_card() {
        let ghost = refusal(&format!("{DUO}pattern ghost bars 1 grid 4\nlane pulse1 duty 25\n  1: C9 x1\nplay\n  a\n"));
        assert_eq!((ghost.code, ghost.pattern.as_deref(), ghost.lane.as_deref(), ghost.at, ghost.line), ("out-of-range", Some("ghost"), Some("pulse1"), Some(pos(1, 1)), Some(10)));
        assert_eq!(ghost.to_string(), "line 10, pattern ghost, lane pulse1, at 1.1: C9 is out of range; this lane plays C2 through B7");
        let knob = refusal(&format!("{DUO}pattern ghost bars 1 grid 4\nlane triangle vol 8\nplay\n  a\n"));
        assert_eq!((knob.code, knob.lane.as_deref(), knob.at, knob.line), ("no-volume-knob", Some("triangle"), None, Some(9)));
    }

    #[test]
    fn notes_never_carry_across_into_the_next_pattern() {
        let crossing = refusal("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  3: C4 x4\npattern b bars 1 grid 4\nplay\n  a\n  b\n");
        assert_eq!((crossing.code, crossing.pattern.as_deref(), crossing.line), ("runs-past-end", Some("a"), Some(4)));
    }

    #[test]
    fn the_cap_counts_the_play_list_repeats_and_all() {
        let song = |plays: usize| Song::parse(&format!("song \"s\" tempo 120\npattern a bars 100 grid 4\nplay\n{}", "  a\n".repeat(plays))).unwrap();
        assert_eq!(overlong(&song(3)), None);
        assert_eq!(overlong(&song(4)).expect("800 seconds is past the cap").message, "the song runs 800.0 seconds (400 bars at tempo 120), past the 10 minute cap");
        let shelved = take("song \"s\" tempo 120\npattern endless bars 4000000000 grid 255\nlane pulse1\n  1.1: C4 x1\npattern ding bars 1 grid 4\nplay\n  ding\n").unwrap();
        assert_eq!(shelved.stats.seconds, 2.0, "an unplayed pattern costs nothing, however long");
    }

    #[test]
    fn a_mixed_meter_play_block_runs_each_bar_at_its_own_length() {
        let heard = take("song \"s\" tempo 120\npattern four bars 1 grid 16\nlane pulse1\n  1: C4 x16\npattern seven bars 1 beats 7 grid 14\nlane pulse1\n  1: G4 x6\n  7: E4 x8 arp 0,4\nplay\n  four\n  seven\n  four +2\n").unwrap();
        assert_eq!(heard.plays, [0..88_200, 88_200..242_550, 242_550..330_750], "2 s, then 3.5 s, then 2 s");
        assert_eq!(heard.stats.seconds, 7.5);
        assert!(heard.samples[88_200..242_550].iter().all(|s| *s != 0), "the seven-beat bar is written full and plays full");
        let [Flourish::Arpeggio { at, steps, rate }] = &heard.stats.lanes[1].flourishes[..] else { panic!("{:?}", heard.stats.lanes[1]) };
        assert_eq!((*at, *steps, *rate), (pos(1, 7), 120, 60.0), "eight half-beat slots at 120 are 2 s of arp");
    }

    #[test]
    fn the_cap_counts_beats_not_bars() {
        let song = |beats: u32, plays: usize| Song::parse(&format!("song \"s\" tempo 120\npattern a bars 100 beats {beats} grid 4\nplay\n{}", "  a\n".repeat(plays))).unwrap();
        assert_eq!(overlong(&song(4, 3)), None);
        let seven = overlong(&song(7, 2)).expect("1400 beats at 120 is 700 seconds");
        assert_eq!(seven.message, "the song runs 700.0 seconds (200 bars, 1400 beats at tempo 120), past the 10 minute cap", "200 four-beat bars would have slipped under at 400 s");
        assert_eq!(overlong(&song(2, 6)), None, "600 short bars are exactly the ten minutes");
        let vast = refusal("song \"s\" tempo 120\npattern endless bars 4000000000 beats 32 grid 255\nlane pulse1\n  1.1: C4 x1\n");
        assert_eq!(vast.message, "the song runs 64000000000.0 seconds (4000000000 bars, 128000000000 beats at tempo 120), past the 10 minute cap");
    }

    #[test]
    fn a_vast_thirty_two_beat_pattern_on_the_shelf_costs_nothing() {
        let shelf = "pattern endless bars 4000000000 beats 32 grid 255\nlane pulse1\n  1.1: C4 x9 fade\n  4000000000.1: A4 x60 arp 0,4,7\n";
        let heard = take(&format!("{DUO}{shelf}play\n  a\n")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(heard.stats.seconds, 2.0);
        let late = refusal(&format!("{DUO}{shelf}  4000000000.255: E5 x1 vib 20ct 6hz 2000ms\nplay\n  a\n"));
        assert_eq!((late.code, late.pattern.as_deref(), late.at), ("vibrato-never-starts", Some("endless"), Some(pos(4_000_000_000, 255))));
        assert!(late.message.contains("outlasts the note's 62.7 ms"), "a slot of 32 beats over 255 at tempo 120: {}", late.message);
    }

    #[test]
    fn parse_errors_keep_their_place() {
        let overlap = refusal("song \"s\" tempo 120\npattern main bars 1 grid 8\nlane pulse1\n  1: C4 x4\n  3: D4 x2\n");
        assert_eq!(overlap.code, "overlap");
        assert_eq!((overlap.pattern.as_deref(), overlap.lane.as_deref(), overlap.at, overlap.line), (Some("main"), Some("pulse1"), Some(pos(1, 3)), Some(5)));
        let header = refusal("song \"s\" tempo fast\n");
        assert_eq!((header.code, header.pattern.as_deref(), header.lane.as_deref(), header.at, header.line), ("not-a-count", None, None, None, Some(1)));
        assert_eq!(header.json()["at"], Value::Null);
    }

    #[test]
    fn render_refusals_carry_location_without_saying_it_twice() {
        let low = refusal("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x1\n  2: B1 x1\n");
        assert_eq!((low.code, low.pattern.as_deref(), low.lane.as_deref(), low.at, low.line), ("out-of-range", Some("p"), Some("pulse1"), Some(pos(1, 2)), Some(5)));
        assert_eq!(low.to_string(), "line 5, pattern p, lane pulse1, at 1.2: B1 is out of range; this lane plays C2 through B7");
        let duty = refusal("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane pulse2 duty 33\n");
        assert_eq!((duty.code, duty.lane.as_deref(), duty.line), ("bad-duty", Some("pulse2"), Some(3)));
        assert!(duty.message.starts_with("duty `33`"), "{}", duty.message);
        let profile = refusal("song \"s\" tempo 90 profile gameboy\npattern p bars 1 grid 4\n");
        assert_eq!((profile.code, profile.pattern, profile.line), ("unknown-profile", None, None));
    }

    #[test]
    fn the_envelope_has_every_field_even_when_empty() {
        let heard = take(spec_example()).unwrap();
        let stats = heard.stats.json();
        assert_eq!(stats["lanes"][2]["notes"][1], json!({ "at": "1.9", "end": "2.8", "rest": false }));
        assert_eq!(stats["saturated_samples"], 0);
        let remark = Remark::loose("too-long", "x").json();
        for key in ["code", "message", "pattern", "lane", "at", "line"] { assert!(remark.get(key).is_some(), "{key} missing"); }
    }

    #[test]
    fn check_reports_what_the_effects_really_do() {
        let heard = take("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n  1: A4 x8 vib 20ct 6hz 125ms\n  9: A4 x4 arp 0,4,7\nplay\n  p\n  p +12\n  p\n").unwrap();
        let effects = &heard.stats.lanes[0].flourishes;
        let [Flourish::Vibrato { at, hertz, delay_ms, swings }, Flourish::Arpeggio { steps: 30, rate, .. }] = &effects[..] else { panic!("{effects:?}") };
        assert_eq!((*at, round(*hertz, 3), round(*delay_ms, 1), *rate), (pos(1, 1), 6.667, 133.3, 60.0));
        assert_eq!(swings.iter().map(|s| (s.shift, s.ticks)).collect::<Vec<_>>(), [(0, 3), (12, 1)], "one swing per distinct play, ticks re-derived from the played root");
        assert!(swings.iter().all(|s| s.up > s.down), "the register bends further up than down: {swings:?}");
        let json = heard.stats.json();
        assert_eq!(json["lanes"][0]["effects"][1], json!({ "at": "1.9", "effect": "arp", "steps": 30, "steps_per_second": 60.0 }));
        assert_eq!(json["lanes"][0]["effects"][0]["swings"][1]["transpose"], 12);
        let told = tell(None, &heard.stats, Detail::Deposition, &[]);
        assert!(told.contains("\n  1.1 vib: 6.667 Hz after 133 ms, swings +"), "{told}");
        assert!(told.contains("; played +12, +"), "{told}");
        assert!(told.contains("\n  1.9 arp: 30 steps, 60.0 a second"), "{told}");
    }

    #[test]
    fn a_fading_arp_is_measured_like_any_arp_and_leaves_the_next_note_alone() {
        let pair = |first: &str| take(&format!("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1 vol 6\n  1: A4 x4 arp 0,4,7{first}\n  5: A4 x4\n")).unwrap();
        let (faded, plain) = (pair(" vol 12 fade duty 75"), pair(""));
        assert_eq!(faded.stats.lanes[0].flourishes, [Flourish::Arpeggio { at: pos(1, 1), steps: 30, rate: 60.0 }]);
        assert_eq!(faded.stats.lanes[0].notes.iter().map(|r| (r.at, r.end)).collect::<Vec<_>>(), [(pos(1, 1), pos(1, 4)), (pos(1, 5), pos(1, 8))]);
        assert_eq!((faded.stats.seconds, faded.samples.len()), (plain.stats.seconds, plain.samples.len()));
        assert_eq!(faded.samples[22_050..], plain.samples[22_050..], "the next note plays at the lane's own duty and volume");
        assert!(faded.warnings.is_empty(), "{:?}", faded.warnings);
    }

    #[test]
    fn a_transposed_play_that_swings_off_the_chip_is_refused_at_the_note() {
        let off = refusal("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: G#7 x4 vib 90ct 6hz\nplay\n  p\n  p +2\n");
        assert_eq!((off.code, off.lane.as_deref(), off.at, off.line), ("vibrato-out-of-range", Some("pulse1"), Some(pos(1, 1)), Some(4)));
        assert!(off.message.starts_with("played +2 (play line 7), the vibrato on A#7"), "{}", off.message);
    }

    #[test]
    fn the_macro_cap_counts_the_whole_song() {
        let bars = |name: &str, from: u32, n: u32| format!("pattern {name} bars 1 grid {n}\nlane pulse1\n{}", (1..=n).map(|b| format!("  {b}: C4 x1 vib 5ct {:.6}hz\n", 20.0 / f64::from(from + b))).collect::<String>());
        let fits = take(&format!("song \"s\" tempo 255\n{}{}play\n  a\n  b\n  a\n", bars("a", 0, 64), bars("b", 64, 64))).unwrap();
        assert_eq!(fits.stats.lanes.len(), 2);
        let crowded = refusal(&format!("song \"s\" tempo 255\n{}{}play\n  a\n  b\n", bars("a", 0, 64), bars("b", 64, 65)));
        assert_eq!((crowded.code, crowded.pattern.as_deref(), crowded.at), ("too-many-macros", Some("b"), Some(pos(1, 65))));
    }

    #[test]
    fn an_unplayed_pattern_answers_for_its_effects_without_rendering() {
        let hiss = refusal(&format!("{DUO}pattern ghost bars 1 grid 4\nlane noise\n  1: C4 x1 vib 20ct 6hz\nplay\n  a\n"));
        assert_eq!((hiss.code, hiss.pattern.as_deref(), hiss.at, hiss.line), ("noise-pitch-effect", Some("ghost"), Some(pos(1, 1)), Some(10)));
        let blink = refusal(&format!("{DUO}pattern ghost bars 4000000000 grid 255\nlane pulse1\n  1: C4 x1 fade\nplay\n  a\n"));
        assert_eq!((blink.code, blink.pattern.as_deref(), blink.line), ("fade-too-short", Some("ghost"), Some(10)));
        assert!(take(&format!("{DUO}pattern ghost bars 4000000000 grid 255\nlane pulse1\n  1: C4 x9 fade\n  4000000000.1: A4 x60 arp 0,4,7\nplay\n  a\n")).is_ok(), "a vast unplayed pattern is vetted note by note, never walked");
    }

    #[test]
    fn the_headline_alone_still_carries_its_warnings() {
        let heard = take("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x4\nlane noise vol 0\n  1: C4 x4\n").unwrap();
        let told = tell(None, &heard.stats, Detail::Headline, &heard.warnings);
        let lines: Vec<&str> = told.lines().collect();
        assert_eq!(lines.len(), 2, "{told}");
        assert!(lines[0].starts_with("checked, nothing written: 2.000 seconds, peak -"), "{told}");
        assert_eq!(lines[1], "warning: line 5, pattern p, lane noise: this lane has notes but renders silent");
        assert_eq!(heard.stats.headline().as_object().map(|s| s.keys().cloned().collect::<Vec<_>>()), Some(vec!["duration_seconds".to_owned(), "peak_dbfs".to_owned(), "saturated_samples".to_owned()]));
    }

    #[test]
    fn the_gauges_join_the_stats_without_moving_a_field_that_was_there() {
        let mut heard = take(spec_example()).unwrap();
        let before = heard.stats.json();
        heard.stats.gauges = Some(vu::read(&heard.song, &heard.samples, &heard.plays));
        let after = heard.stats.json();
        let keys: Vec<&str> = after.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["duration_seconds", "peak_dbfs", "saturated_samples", "rms_dbfs", "sections", "sounding_percent", "lanes"]);
        for (key, value) in before.as_object().unwrap() { assert_eq!(after[key], *value, "{key} changed under the gauges"); }
        assert_eq!(after["sections"][0]["pattern"], "main");
        let deposed = tell(None, &heard.stats, Detail::Everything, &[]);
        assert!(deposed.contains("\nloudness by bar in dbfs, whole song rms -") && deposed.contains("\npattern main, lane pulse1 sounds: "), "{deposed}");
    }
}
