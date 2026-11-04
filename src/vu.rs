use crate::listen::{peak_dbfs, round};
use piropipo::profile;
use piropipo::render::SAMPLE_RATE;
use piropipo::score::{Pattern, Song};
use serde_json::{Map, Value, json};
use std::ops::Range;

const NOTCHES_PER_DB: f64 = 2.0;

const DIAL_DB: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level { pub rms: Option<f64>, pub peak: Option<f64> }

impl Level {
    fn of(samples: &[i16]) -> Self { Level { rms: rms_dbfs(samples), peak: peak_dbfs(samples) } }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bar { pub bar: u32, pub at: f64, pub level: Level }

#[derive(Debug, Clone, PartialEq)]
pub struct Section { pub pattern: String, pub play: usize, pub plays: usize, pub shift: i8, pub beats: u32, pub grid: u32, pub line: usize, pub samples: Range<usize>, pub level: Level, pub lanes: Vec<(String, usize)>, pub bars: Vec<Bar> }

impl Section {
    fn json(&self) -> Value {
        let lanes: Vec<Value> = self.lanes.iter().map(|(lane, line)| json!({ "lane": lane, "line": line })).collect();
        let bars: Vec<Value> = self.bars.iter().map(|b| json!({ "bar": b.bar, "start_seconds": b.at, "rms_dbfs": b.level.rms, "peak_dbfs": b.level.peak })).collect();
        json!({
            "pattern": self.pattern, "play": self.play, "transpose": self.shift, "beats": self.beats, "grid": self.grid, "line": self.line,
            "start_seconds": seconds(self.samples.start), "end_seconds": seconds(self.samples.end), "start_sample": self.samples.start, "end_sample": self.samples.end,
            "rms_dbfs": self.level.rms, "peak_dbfs": self.level.peak, "lanes": lanes, "bars": bars,
        })
    }

    fn label(&self) -> String {
        let mut asides = Vec::new();
        if self.plays > 1 { asides.push(format!("play {} of {}", self.play, self.plays)); }
        if self.shift != 0 { asides.push(format!("{:+}", self.shift)); }
        if asides.is_empty() { self.pattern.clone() } else { format!("{} ({})", self.pattern, asides.join(", ")) }
    }

    fn tell(&self, loudest: f64) -> Vec<String> {
        let head = format!("{} at {:.2}s: rms {}, peak {}", self.label(), seconds(self.samples.start), db(self.level.rms), db(self.level.peak));
        std::iter::once(head).chain(self.bars.iter().map(|b| format!("  bar {:<3} {:>6} rms {:>6} peak  {}", b.bar, db(b.level.rms), db(b.level.peak), needle(b.level.rms, loudest)))).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Turnout { pub pattern: String, pub lanes: Vec<(String, f64)> }

#[derive(Debug, Clone, PartialEq)]
pub struct Reading { pub rms: Option<f64>, pub sections: Vec<Section>, pub turnouts: Vec<Turnout>, pub whole: Vec<(String, f64)> }

impl Reading {
    pub fn json(&self) -> Value {
        let patterns: Map<String, Value> = self.turnouts.iter().map(|t| (t.pattern.clone(), percents(&t.lanes))).collect();
        json!({ "rms_dbfs": self.rms, "sections": self.sections.iter().map(Section::json).collect::<Vec<_>>(), "sounding_percent": { "song": percents(&self.whole), "patterns": patterns } })
    }

    pub fn tell(&self) -> String {
        let loudest = self.sections.iter().flat_map(|s| &s.bars).filter_map(|b| b.level.rms).fold(f64::NEG_INFINITY, f64::max);
        let mut lines = vec![format!("loudness by bar in dbfs, whole song rms {} (gauge: {NOTCHES_PER_DB} # per db, the loudest bar fills {:.0})", db(self.rms), DIAL_DB * NOTCHES_PER_DB)];
        lines.extend(self.sections.iter().flat_map(|s| s.tell(loudest)));
        lines.extend(self.roster());
        lines.join("\n")
    }

    fn roster(&self) -> Vec<String> {
        if self.whole.is_empty() { return Vec::new(); }
        let names: Vec<&str> = self.whole.iter().map(|(lane, _)| lane.as_str()).collect();
        let wide = self.turnouts.iter().map(|t| t.pattern.len()).chain(["whole song".len()]).max().unwrap_or_default();
        let row = |label: &str, lanes: &[(String, f64)]| {
            let cells: String = names.iter().map(|name| {
                let cell = lanes.iter().find(|(lane, _)| lane == name).map_or("-".to_owned(), |(_, share)| format!("{:.1}", round(share * 100.0, 1)));
                format!("  {cell:>w$}", w = name.len().max(5))
            }).collect();
            format!("  {label:<wide$}{cells}")
        };
        let header = format!("  {:wide$}{}", "", names.iter().map(|name| format!("  {name:>w$}", w = name.len().max(5))).collect::<String>());
        let mut lines = vec!["sounding % by lane (- is a lane the pattern never writes; the whole song weighs each play by its length):".to_owned(), header];
        lines.extend(self.turnouts.iter().map(|t| row(&t.pattern, &t.lanes)));
        lines.push(row("whole song", &self.whole));
        lines
    }
}

pub fn read(song: &Song, samples: &[i16], plays: &[Range<usize>]) -> Reading {
    let cues: Vec<(&Pattern, i8, &Range<usize>)> = song.setlist.iter().zip(plays).filter_map(|(cue, span)| Some((song.pattern(&cue.pattern)?, cue.shift, span))).collect();
    let sections = cues.iter().enumerate().map(|(i, &(pattern, shift, span))| {
        let play = cues.iter().take(i + 1).filter(|c| c.0.name == pattern.name).count();
        let plays = cues.iter().filter(|c| c.0.name == pattern.name).count();
        let bars = (0..pattern.bars).map(|b| { let (from, to) = (edge(span, pattern.bars, b), edge(span, pattern.bars, b + 1)); Bar { bar: b + 1, at: seconds(from), level: Level::of(samples.get(from..to).unwrap_or_default()) } }).collect();
        let lanes = pattern.lanes.iter().map(|l| (l.name.clone(), l.line)).collect();
        Section { pattern: pattern.name.clone(), play, plays, shift, beats: pattern.beats, grid: pattern.grid, line: pattern.line, samples: span.clone(), level: Level::of(samples.get(span.clone()).unwrap_or_default()), lanes, bars }
    }).collect();
    let mut turnouts: Vec<Turnout> = Vec::new();
    for &(pattern, _, _) in &cues { if !turnouts.iter().any(|t| t.pattern == pattern.name) { turnouts.push(turnout(song, pattern)); } }
    let heard: usize = cues.iter().map(|c| c.2.len()).sum();
    let mut whole: Vec<(String, f64)> = Vec::new();
    for &(pattern, _, span) in &cues {
        let weight = span.len() as f64 / heard.max(1) as f64;
        for (lane, share) in turnouts.iter().filter(|t| t.pattern == pattern.name).flat_map(|t| &t.lanes) {
            match whole.iter_mut().find(|(name, _)| name == lane) {
                Some((_, sum)) => *sum += share * weight,
                None => whole.push((lane.clone(), share * weight)),
            }
        }
    }
    Reading { rms: rms_dbfs(samples), sections, turnouts, whole }
}

fn turnout(song: &Song, pattern: &Pattern) -> Turnout {
    let card = profile::card(song.profile.as_deref()).ok();
    let slots = u64::from(pattern.bars) * u64::from(pattern.grid);
    let lanes = pattern.lanes.iter().map(|lane| {
        let home = card.and_then(|c| c.part(&lane.name).ok()).and_then(|p| p.setup(&lane.params).ok()).map_or(1, |s| s.volume);
        let lit: u64 = lane.notes.iter().filter(|n| n.pitch.is_some() && n.ornaments.vol.unwrap_or(home) > 0).map(|n| u64::from(n.len)).sum();
        (lane.name.clone(), lit as f64 / slots.max(1) as f64)
    }).collect();
    Turnout { pattern: pattern.name.clone(), lanes }
}

fn edge(span: &Range<usize>, bars: u32, bar: u32) -> usize { span.start + usize::try_from(span.len() as u64 * u64::from(bar) / u64::from(bars.max(1))).unwrap_or(span.len()) }

fn seconds(sample: usize) -> f64 { round(sample as f64 / f64::from(SAMPLE_RATE), 3) }

fn rms_dbfs(samples: &[i16]) -> Option<f64> {
    let energy: u64 = samples.iter().map(|s| u64::from(s.unsigned_abs()).pow(2)).sum();
    (energy > 0).then(|| round(10.0 * (energy as f64 / samples.len() as f64 / f64::from(i16::MAX).powi(2)).log10(), 2))
}

fn percents(lanes: &[(String, f64)]) -> Value { Value::Object(lanes.iter().map(|(lane, share)| (lane.clone(), json!(round(share * 100.0, 1)))).collect()) }

fn db(level: Option<f64>) -> String { level.map_or("silent".to_owned(), |l| format!("{l:.1}")) }

fn needle(rms: Option<f64>, loudest: f64) -> String { "#".repeat(rms.map_or(0.0, |r| ((r - loudest + DIAL_DB) * NOTCHES_PER_DB).round().max(0.0)) as usize) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listen::take;

    fn song(text: &str) -> Song { Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}")) }

    const ROUNDS: &str = "song \"s\" tempo 120\npattern a bars 2 grid 4\nlane pulse1\n  1: C4 x8\npattern b bars 1 grid 4\nlane pulse1\n  1: C4 x2\n  3: - x2\nlane noise\n  1: C4 x4\nplay\n  a\n  b\n  a +2\n";

    fn rounds() -> Reading {
        let mut samples = vec![16_384; 44_100];
        samples.extend([0; 44_100]);
        samples.extend([3_277; 44_100]);
        samples.extend([i16::MAX; 88_200]);
        read(&song(ROUNDS), &samples, &[0..88_200, 88_200..132_300, 132_300..220_500])
    }

    #[test]
    fn rms_is_measured_against_full_scale() {
        assert_eq!(rms_dbfs(&[]), None);
        assert_eq!(rms_dbfs(&[0, 0, 0]), None);
        assert_eq!(rms_dbfs(&[i16::MAX, i16::MIN]), Some(0.0));
        assert_eq!(rms_dbfs(&[i16::MAX, 0]), Some(-3.01));
        assert_eq!(rms_dbfs(&[16_384, -16_384]), Some(-6.02));
        assert_eq!(rms_dbfs(&[3_277, -3_277, 3_277]), Some(-20.0));
    }

    #[test]
    fn bar_edges_tile_a_play_without_losing_a_sample() {
        assert_eq!((0..=3).map(|b| edge(&(10..17), 3, b)).collect::<Vec<_>>(), [10, 12, 14, 17]);
        assert_eq!((0..=4).map(|b| edge(&(88_200..441_000), 4, b)).collect::<Vec<_>>(), [88_200, 176_400, 264_600, 352_800, 441_000]);
    }

    #[test]
    fn sections_follow_the_play_block_and_count_repeats() {
        let reading = rounds();
        assert_eq!(reading.sections.iter().map(Section::label).collect::<Vec<_>>(), ["a (play 1 of 2)", "b", "a (play 2 of 2, +2)"]);
        assert_eq!(reading.sections.iter().map(|s| (s.play, s.shift, s.samples.clone())).collect::<Vec<_>>(), [(1, 0, 0..88_200), (1, 0, 88_200..132_300), (2, 2, 132_300..220_500)]);
        let first = &reading.sections[0];
        assert_eq!(first.bars.iter().map(|b| (b.bar, b.at, b.level)).collect::<Vec<_>>(), [(1, 0.0, Level { rms: Some(-6.02), peak: Some(-6.02) }), (2, 1.0, Level { rms: None, peak: None })]);
        assert_eq!(first.level, Level { rms: Some(-9.03), peak: Some(-6.02) }, "a section's rms is over its samples, not an average of its bars' decibels");
        assert_eq!(reading.sections[2].bars.iter().map(|b| b.at).collect::<Vec<_>>(), [3.0, 4.0]);
    }

    #[test]
    fn turnout_counts_sounding_slots_not_rests_or_muted_notes() {
        let muted = song("song \"s\" tempo 120\npattern p bars 2 grid 4\nlane pulse1\n  1.1: C4 x2\n  1.3: - x2\n  2.1: C4 x2 vol 0\n  2.3: D4 x2\nlane noise vol 0\n  1.1: C4 x4\n  2.1: C4 x4 vol 6\nlane triangle\n  1.1: C3 x8\nlane pulse2\n");
        let reading = read(&muted, &[], std::slice::from_ref(&(0..0)));
        assert_eq!(reading.turnouts, [Turnout { pattern: "p".into(), lanes: vec![("pulse1".into(), 0.5), ("noise".into(), 0.5), ("triangle".into(), 1.0), ("pulse2".into(), 0.0)] }]);
    }

    #[test]
    fn the_whole_song_weighs_each_play_by_time_not_by_slot_count() {
        let mixed = song("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern b bars 3 grid 16\nlane pulse1\n  1.1: - x48\nlane triangle\n  1.1: C3 x48\nplay\n  a\n  a\n  b\n");
        let reading = read(&mixed, &[], &[0..100, 100..200, 200..500]);
        assert_eq!(reading.turnouts.iter().map(|t| t.pattern.as_str()).collect::<Vec<_>>(), ["a", "b"], "one turnout per pattern, however often it plays");
        assert_eq!(percents(&reading.whole), json!({ "pulse1": 40.0, "triangle": 60.0 }));
    }

    #[test]
    fn each_play_owns_its_own_stretch_of_the_render() {
        let heard = take("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern b bars 2 grid 4\nlane triangle\n  1: - x8\nplay\n  b\n  a\n").unwrap();
        assert_eq!(heard.plays, [0..176_400, 176_400..264_600]);
        let reading = read(&heard.song, &heard.samples, &heard.plays);
        let (b, a) = (&reading.sections[0], &reading.sections[1]);
        assert_eq!((b.level, b.bars.len()), (Level { rms: None, peak: None }, 2));
        assert_eq!((a.samples.clone(), a.level.peak), (176_400..264_600, heard.stats.peak_dbfs));
        assert!(a.level.rms.is_some_and(|rms| rms < -3.0), "{a:?}");
        assert_eq!(reading.rms, rms_dbfs(&heard.samples));
    }

    #[test]
    fn a_vast_shelved_pattern_costs_the_gauges_nothing() {
        let heard = take("song \"s\" tempo 120\npattern endless bars 4000000000 grid 255\nlane pulse1\n  1.1: C4 x1\npattern ding bars 1 grid 4\nlane pulse1\n  1: C5 x2\nplay\n  ding\n").unwrap();
        let reading = read(&heard.song, &heard.samples, &heard.plays);
        assert_eq!(reading.sections.iter().map(|s| (s.pattern.as_str(), s.bars.len())).collect::<Vec<_>>(), [("ding", 1)]);
        assert_eq!(reading.turnouts.iter().map(|t| t.pattern.as_str()).collect::<Vec<_>>(), ["ding"]);
    }

    #[test]
    fn the_reading_speaks_json_by_section_and_by_lane() {
        let json = rounds().json();
        assert_eq!(json["sections"][2], json!({ "pattern": "a", "play": 2, "transpose": 2, "beats": 4, "grid": 4, "line": 2, "start_seconds": 3.0, "end_seconds": 5.0, "start_sample": 132_300, "end_sample": 220_500, "rms_dbfs": 0.0, "peak_dbfs": 0.0, "lanes": [{ "lane": "pulse1", "line": 3 }], "bars": [
            { "bar": 1, "start_seconds": 3.0, "rms_dbfs": 0.0, "peak_dbfs": 0.0 },
            { "bar": 2, "start_seconds": 4.0, "rms_dbfs": 0.0, "peak_dbfs": 0.0 },
        ] }));
        assert_eq!(json["sections"][0]["bars"][1], json!({ "bar": 2, "start_seconds": 1.0, "rms_dbfs": null, "peak_dbfs": null }));
        assert_eq!(json["sounding_percent"], json!({ "song": { "pulse1": 90.0, "noise": 20.0 }, "patterns": { "a": { "pulse1": 100.0 }, "b": { "pulse1": 50.0, "noise": 100.0 } } }));
        assert!(json["rms_dbfs"].is_f64());
    }

    #[test]
    fn each_section_says_its_meter_and_splits_its_own_bars_evenly() {
        let heard = take("song \"s\" tempo 120\npattern four bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern seven bars 2 beats 7 grid 14\nlane pulse1\n  1.1: C4 x14\nplay\n  four\n  seven\n").unwrap();
        let json = read(&heard.song, &heard.samples, &heard.plays).json();
        assert_eq!(json["sections"].as_array().unwrap().iter().map(|s| s["beats"].clone()).collect::<Vec<_>>(), [json!(4), json!(7)]);
        assert_eq!(json["sections"][1]["bars"].as_array().unwrap().iter().map(|b| b["start_seconds"].clone()).collect::<Vec<_>>(), [json!(2.0), json!(5.5)], "a seven-beat bar at 120 lasts 3.5 s");
        assert_eq!(json["sections"][0].as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(), ["pattern", "play", "transpose", "beats", "grid", "line", "start_seconds", "end_seconds", "start_sample", "end_sample", "rms_dbfs", "peak_dbfs", "lanes", "bars"]);
    }

    #[test]
    fn each_section_names_its_source_lines_and_its_exact_samples() {
        let heard = take("song \"s\" tempo 133\npattern a bars 1 grid 16\nlane pulse1 duty 25\n  1: C4 x16\nlane noise\n  1: C4 x16\npattern b bars 2 beats 7 grid 14\nlane noise\n  1.1: C4 x28\nlane pulse1\n  1.1: - x28\nplay\n  a\n  b\n  a +2\n").unwrap();
        let json = read(&heard.song, &heard.samples, &heard.plays).json();
        let sections = json["sections"].as_array().unwrap();
        let spans: Vec<Range<usize>> = sections.iter().map(|s| s["start_sample"].as_u64().unwrap() as usize..s["end_sample"].as_u64().unwrap() as usize).collect();
        assert_eq!(spans, heard.plays, "a section is exactly the stretch of wav its play rendered");
        assert_eq!((spans[1].start, sections[1]["start_seconds"].clone(), sections[0]["end_seconds"].clone()), (79_579, json!(1.805), json!(1.805)), "four beats at 133 is 79578.9 samples, which the millisecond field can only round");
        assert_eq!(sections.iter().map(|s| (s["grid"].clone(), s["line"].clone())).collect::<Vec<_>>(), [(json!(16), json!(2)), (json!(14), json!(7)), (json!(16), json!(2))]);
        assert_eq!(sections[0]["lanes"], json!([{ "lane": "pulse1", "line": 3 }, { "lane": "noise", "line": 5 }]));
        assert_eq!(sections[1]["lanes"], json!([{ "lane": "noise", "line": 8 }, { "lane": "pulse1", "line": 10 }]), "each pattern block has its own lane headers, in written order");
        assert_eq!(sections[2]["lanes"], sections[0]["lanes"]);
    }

    #[test]
    fn the_chart_draws_a_gauge_per_bar_against_the_loudest() {
        let told = rounds().tell();
        let lines: Vec<&str> = told.lines().collect();
        assert_eq!(lines[1..4], ["a (play 1 of 2) at 0.00s: rms -9.0, peak -6.0", &format!("  bar 1     -6.0 rms   -6.0 peak  {}", "#".repeat(28)), "  bar 2   silent rms silent peak  "]);
        assert_eq!(lines[7], format!("  bar 1      0.0 rms    0.0 peak  {}", "#".repeat(40)));
        assert_eq!(lines[9..], ["sounding % by lane (- is a lane the pattern never writes; the whole song weighs each play by its length):", "              pulse1  noise", "  a            100.0      -", "  b             50.0  100.0", "  whole song    90.0   20.0"]);
    }
}
