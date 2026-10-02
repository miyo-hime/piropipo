use crate::surface;
use piropipo::render::{self, RenderError, SAMPLE_RATE};
use piropipo::score::{Lane, Pattern, Position, ScoreError, Song};
use serde_json::{Value, json};
use std::fmt;
use std::path::{Path, PathBuf};

const CAP_SECONDS: u64 = 600;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Solo { pub pattern: String, pub lane: String, pub line: usize, pub sounding: bool, pub notes: Vec<Ring> }

impl Solo {
    fn voiced(&self) -> bool { self.notes.iter().any(|r| !r.rest) }

    fn json(&self) -> Value {
        let notes: Vec<Value> = self.notes.iter().map(|r| json!({ "at": r.at.to_string(), "end": r.end.to_string(), "rest": r.rest })).collect();
        json!({ "pattern": self.pattern, "lane": self.lane, "sounding": self.sounding, "notes": notes })
    }

    fn tell(&self) -> String {
        let state = match (self.sounding, self.voiced()) { (true, _) => "sounds", (false, true) => "renders silent", (false, false) => "carries no pitched notes" };
        let rings: Vec<String> = self.notes.iter().map(|r| format!("{}{} through {}", if r.rest { "rest " } else { "" }, r.at, r.end)).collect();
        if rings.is_empty() { format!("pattern {}, lane {} {state}", self.pattern, self.lane) } else { format!("pattern {}, lane {} {state}: {}", self.pattern, self.lane, rings.join(", ")) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stats { pub seconds: f64, pub peak_dbfs: Option<f64>, pub saturated: usize, pub lanes: Vec<Solo> }

impl Stats {
    fn json(&self) -> Value {
        json!({ "duration_seconds": self.seconds, "peak_dbfs": self.peak_dbfs, "saturated_samples": self.saturated, "lanes": self.lanes.iter().map(Solo::json).collect::<Vec<_>>() })
    }
}

pub struct Take { pub samples: Vec<i16>, pub stats: Stats, pub warnings: Vec<Remark> }

pub fn read(input: &str) -> Result<String, Vec<Remark>> {
    surface::read_source(input).map(|(text, _)| text).map_err(|e| vec![Remark::loose("unreadable-input", format!("can't read {input}: {e}"))])
}

pub fn take(text: &str) -> Result<Take, Vec<Remark>> {
    let song = Song::parse(text).map_err(|errors| errors.into_iter().map(Remark::from).collect::<Vec<_>>())?;
    if let Some(remark) = overlong(&song) { return Err(vec![remark]); }
    let mut samples = Vec::new();
    let mut lanes = Vec::new();
    for pattern in &song.patterns {
        samples.extend(render::pattern(&song, pattern).map_err(|e| vec![misheard(e, pattern)])?);
        for lane in &pattern.lanes { lanes.push(solo(&song, pattern, lane)?); }
    }
    let stats = Stats { seconds: round(samples.len() as f64 / f64::from(SAMPLE_RATE), 3), peak_dbfs: peak_dbfs(&samples), saturated: saturated(&samples), lanes };
    let mut warnings: Vec<Remark> = stats.lanes.iter().filter_map(hush).collect();
    if stats.saturated > 0 { warnings.push(Remark::loose("saturation", format!("{} sample{} sit at full scale; the mix is hitting the rail and may be clipping", stats.saturated, if stats.saturated == 1 { "" } else { "s" }))); }
    Ok(Take { samples, stats, warnings })
}

pub fn answer(verb: &str, human: bool, played: Result<(Option<PathBuf>, Take), Vec<Remark>>) -> Result<(), Box<dyn std::error::Error>> {
    match played {
        Ok((output, take)) => {
            let envelope = json!({ "ok": true, "verb": verb, "output": output.as_ref().map(|p| p.display().to_string()), "stats": take.stats.json(), "warnings": take.warnings.iter().map(Remark::json).collect::<Vec<_>>(), "errors": [] });
            surface::emit(human, envelope, &tell(output.as_deref(), &take.stats, &take.warnings));
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

fn tell(output: Option<&Path>, stats: &Stats, warnings: &[Remark]) -> String {
    let head = output.map_or("checked, nothing written".to_owned(), |p| format!("wrote {}", p.display()));
    let peak = stats.peak_dbfs.map_or("silent throughout".to_owned(), |p| format!("peak {p:.2} dbfs"));
    let rail = match stats.saturated { 0 => "no samples at full scale".to_owned(), 1 => "1 sample at full scale".to_owned(), n => format!("{n} samples at full scale") };
    let mut lines = vec![format!("{head}: {:.3} seconds, {peak}, {rail}", stats.seconds)];
    lines.extend(stats.lanes.iter().map(Solo::tell));
    lines.extend(warnings.iter().map(|w| format!("warning: {w}")));
    lines.join("\n")
}

fn overlong(song: &Song) -> Option<Remark> {
    let bars: u64 = song.patterns.iter().map(|p| u64::from(p.bars)).sum();
    let tempo = u64::from(song.tempo);
    (bars * 240 > CAP_SECONDS * tempo).then(|| Remark::loose("too-long", format!("the song runs {:.1} seconds ({bars} bar{} at tempo {tempo}), past the {} minute cap", (bars * 240) as f64 / tempo as f64, if bars == 1 { "" } else { "s" }, CAP_SECONDS / 60)))
}

fn solo(song: &Song, pattern: &Pattern, lane: &Lane) -> Result<Solo, Vec<Remark>> {
    let mut notes: Vec<Ring> = lane.notes.iter().map(|n| Ring { at: n.at, end: n.rings_through(pattern.grid), rest: n.pitch.is_none() }).collect();
    notes.sort_by_key(|r| r.at);
    let mut heard = Solo { pattern: pattern.name.clone(), lane: lane.name.clone(), line: lane.line, sounding: false, notes };
    if heard.voiced() {
        let alone = Pattern { name: pattern.name.clone(), lanes: vec![lane.clone()], ..*pattern };
        heard.sounding = render::pattern(song, &alone).map_err(|e| vec![misheard(e, pattern)])?.iter().any(|s| *s != 0);
    }
    Ok(heard)
}

fn hush(solo: &Solo) -> Option<Remark> {
    (solo.voiced() && !solo.sounding).then(|| Remark { code: "silent-lane", message: "this lane has notes but renders silent".to_owned(), pattern: Some(solo.pattern.clone()), lane: Some(solo.lane.clone()), at: None, line: Some(solo.line) })
}

fn peak_dbfs(samples: &[i16]) -> Option<f64> {
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().filter(|p| *p > 0)?.min(i16::MAX.unsigned_abs());
    Some(round(20.0 * (f64::from(peak) / f64::from(i16::MAX)).log10(), 2))
}

fn saturated(samples: &[i16]) -> usize { samples.iter().filter(|s| **s == i16::MAX || **s == i16::MIN).count() }

fn round(x: f64, places: i32) -> f64 { let k = 10f64.powi(places); (x * k).round() / k }

fn misheard(e: RenderError, pattern: &Pattern) -> Remark {
    let (code, lane, at) = match &e {
        RenderError::Profile(_) => ("unknown-profile", None, None),
        RenderError::Tempo(_) => ("tempo-out-of-reach", None, None),
        RenderError::Grid(_) => ("grid-out-of-reach", None, None),
        RenderError::Lane(lane) => ("unknown-lane", Some(lane.clone()), None),
        RenderError::Param { lane, .. } => ("unknown-param", Some(lane.clone()), None),
        RenderError::Volume { lane, .. } => ("no-volume-knob", Some(lane.clone()), None),
        RenderError::Duty { lane, .. } => ("bad-duty", Some(lane.clone()), None),
        RenderError::Range { lane, at, .. } => ("out-of-range", Some(lane.clone()), Some(*at)),
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
        let doc = include_str!("../docs/format.md");
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
        let mute = Solo { pattern: "main".into(), lane: "noise".into(), line: 9, sounding: false, notes: vec![Ring { at: pos(1, 1), end: pos(1, 4), rest: false }] };
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
}
