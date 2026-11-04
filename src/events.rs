use crate::listen::{Take, round};
use piropipo::profile::{self, DUTIES, Setup, Voice, named};
use piropipo::render::{self, SAMPLE_RATE};
use piropipo::score::{Bend, Ornaments, Position};
use serde_json::{Value, json};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Strike { pub pattern: String, pub play: usize, pub lane: String, pub voice: Voice, pub pitch: Option<u8>, pub at: Position, pub end: Position, pub samples: Range<u64>, pub volume: u8, pub duty: Option<u8>, pub line: usize, pub ornaments: Ornaments, pub pace: Option<(u64, f64)> }

impl Strike {
    fn json(&self, pitch: u8) -> Value {
        let noise = self.voice == Voice::Noise;
        // the noise hears only the letter: its octave never reaches the chip
        let heard = if noise { named(pitch).trim_end_matches(|c: char| c.is_ascii_digit() || c == '-').to_owned() } else { named(pitch) };
        json!({
            "pattern": self.pattern, "play": self.play, "lane": self.lane, "pitch": heard, "midi": (!noise).then_some(pitch), "at": self.at.to_string(), "end": self.end.to_string(),
            "start_seconds": seconds(self.samples.start), "end_seconds": seconds(self.samples.end), "start_sample": self.samples.start, "end_sample": self.samples.end,
            "volume": self.volume, "duty": self.duty.and_then(width), "line": self.line, "effects": self.effects(pitch),
        })
    }

    fn hush(&self) -> Value {
        json!({
            "pattern": self.pattern, "play": self.play, "lane": self.lane, "at": self.at.to_string(), "end": self.end.to_string(),
            "start_seconds": seconds(self.samples.start), "end_seconds": seconds(self.samples.end), "start_sample": self.samples.start, "end_sample": self.samples.end, "line": self.line,
        })
    }

    fn effects(&self, pitch: u8) -> Value {
        let Ornaments { bend, duty, vol, fade } = &self.ornaments;
        let (steps, rate) = (self.pace.map(|(steps, _)| steps), self.pace.map(|(_, rate)| round(rate, 2)));
        let bend = bend.as_ref().map(|b| (b.word(), match b {
            Bend::Vibrato(vib) => {
                let (up, down) = profile::swing(pitch, profile::ticks(pitch, vib.cents));
                json!({ "rate_hz": round(vib.hertz(), 3), "delay_ms": round(vib.delay_ms(), 1), "up_cents": round(up, 2), "down_cents": round(down, 2) })
            }
            Bend::Arpeggio(offsets) => json!({ "offsets": offsets, "steps": steps, "steps_per_second": rate }),
            Bend::Slide(from) => json!({ "pitch": named(*from), "midi": from, "steps_per_second": rate }),
        }));
        let marks = [bend, duty.map(|d| ("duty", json!(width(d)))), vol.map(|v| ("vol", json!(v))), fade.then(|| ("fade", json!(true)))];
        Value::Object(marks.into_iter().flatten().map(|(word, said)| (word.to_owned(), said)).collect())
    }
}

pub fn strikes(take: &Take) -> Vec<Strike> {
    let Take { song, plays, .. } = take;
    let Ok(card) = profile::card(song.profile.as_deref()) else { return Vec::new() };
    let mut heard = Vec::new();
    for (i, (cue, span)) in song.setlist.iter().zip(plays).enumerate() {
        let pattern = song.pattern(&cue.pattern).expect("the copyist refuses a play line that names no pattern");
        let moved = render::transposed(song, pattern, cue).expect("a take only exists once every play has landed on the chip");
        let play = song.setlist.iter().take(i + 1).filter(|c| c.pattern == cue.pattern).count();
        let sample = |slots: u64| span.start as u64 + render::sample_in(song.tempo, &moved, slots).expect("a take only exists once every played pattern has its staff");
        let mut batch: Vec<(usize, Strike)> = Vec::new();
        for lane in &moved.lanes {
            let Some((seat, part)) = card.parts.iter().enumerate().find(|(_, p)| p.name == lane.name) else { continue };
            let Ok(Setup { volume: home, duty: cast }) = part.setup(&lane.params) else { continue };
            for note in &lane.notes {
                let from = note.at.tick(moved.grid);
                let pace = matches!(note.ornaments.bend, Some(Bend::Arpeggio(_) | Bend::Slide(_))).then(|| render::arp_rate(song.tempo, &moved, note.len)).flatten();
                let duty = note.pitch.and(note.ornaments.duty.or(cast));
                let struck = Strike { pattern: moved.name.clone(), play, lane: lane.name.clone(), voice: part.voice, pitch: note.pitch, at: note.at, end: note.rings_through(moved.grid), samples: sample(from)..sample(from + u64::from(note.len)), volume: note.ornaments.vol.unwrap_or(home), duty, line: note.line, ornaments: note.ornaments.clone(), pace };
                batch.push((seat, struck));
            }
        }
        batch.sort_by_key(|(seat, s)| (s.samples.start, *seat));
        heard.extend(batch.into_iter().map(|(_, s)| s));
    }
    heard
}

pub fn json(strikes: &[Strike]) -> Value { strikes.iter().filter_map(|s| Some(s.json(s.pitch?))).collect() }

pub fn rests(strikes: &[Strike]) -> Value { strikes.iter().filter(|s| s.pitch.is_none()).map(Strike::hush).collect() }

pub fn tell(strikes: &[Strike]) -> String {
    let rests = strikes.iter().filter(|s| s.pitch.is_none()).count();
    let tally = |n: usize, one: &str, many: &str| match n { 0 => format!("no {many}"), 1 => format!("1 {one}"), n => format!("{n} {many}") };
    format!("events: {}, {}; the lists print in the json only, so drop --human to read them", tally(strikes.len() - rests, "note struck", "notes struck"), tally(rests, "rest written", "rests written"))
}

fn width(register: u8) -> Option<f64> { DUTIES.get(usize::from(register)).and_then(|names| names.first()).and_then(|name| name.parse().ok()) }

fn seconds(sample: u64) -> f64 { round(sample as f64 / f64::from(SAMPLE_RATE), 6) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listen::take;
    use piropipo::score::Song;

    fn pos(bar: u32, slot: u32) -> Position { Position { bar, slot } }

    fn struck(text: &str) -> (Take, Vec<Strike>) {
        let heard = take(text).unwrap_or_else(|e| panic!("{e:?}"));
        let strikes = strikes(&heard);
        (heard, strikes)
    }

    fn lit(samples: &[i16]) -> Vec<Range<u64>> {
        let mut runs: Vec<Range<u64>> = Vec::new();
        for (i, _) in samples.iter().enumerate().filter(|(_, s)| **s != 0) {
            let i = i as u64;
            match runs.last_mut() { Some(run) if run.end == i => run.end += 1, _ => runs.push(i..i + 1) }
        }
        runs
    }

    #[test]
    fn every_strike_lights_exactly_the_samples_the_wav_lights_whatever_the_meter() {
        let text = "song \"s\" tempo 133\npattern four bars 1 grid 16\nlane pulse1\n  1: C4 x3\n  5: E4 x2 duty 12.5\n  11: G4 x5\npattern seven bars 2 beats 7 grid 14\nlane pulse1\n  1.1: A4 x5\n  1.9: B4 x4 vib 20ct 6hz\n  2.3: C5 x9 arp 0,4,7\n  2.13: D5 x1 vol 4\nplay\n  four\n  seven\n  four +2\n  seven -5\n";
        let (heard, strikes) = struck(text);
        assert_eq!(strikes.len(), 14);
        assert_eq!(strikes.iter().map(|s| s.samples.clone()).collect::<Vec<_>>(), lit(&heard.samples), "at tempo 133 no slot of either meter is a whole number of samples, and still every note starts and stops on the very sample the render does");
    }

    #[test]
    fn a_seven_beat_bar_keeps_its_own_clock_after_a_four_beat_one() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern four bars 1 grid 16\nlane pulse1\n  1: C4 x16\npattern seven bars 2 beats 7 grid 14\nlane triangle\n  1.9: E3 x4\n  2.1: G3 x14\nplay\n  four\n  seven\n");
        let told = json(&strikes);
        let clock: Vec<(f64, f64, u64)> = told.as_array().unwrap().iter().map(|e| (e["start_seconds"].as_f64().unwrap(), e["end_seconds"].as_f64().unwrap(), e["start_sample"].as_u64().unwrap())).collect();
        assert_eq!(clock, [(0.0, 2.0, 0), (4.0, 5.0, 176_400), (5.5, 9.0, 242_550)], "a half-beat slot at 120 is 0.25 s, so 1.9 is two seconds into the bar that starts at 2");
    }

    #[test]
    fn a_transposed_play_moves_every_pitch_but_the_noise() {
        let text = "song \"s\" tempo 120\npattern p bars 1 grid 16\nlane noise\n  1: C4 x2 vol 12 fade\n  3: B6 x2\nlane pulse1\n  1: C4 x4 arp 0,4,7\n  5: C5 x8 from A4\nlane pulse2\n  1: A4 x8 vib 20ct 6hz\nlane triangle\n  1: E2 x4\nplay\n  p\n  p +3\n  p -12\n";
        let (_, strikes) = struck(text);
        let told = json(&strikes);
        let heard = |play: usize, lane: &str| told.as_array().unwrap().iter().filter(|e| e["play"] == play && e["lane"] == lane).map(|e| (e["pitch"].as_str().unwrap().to_owned(), e["midi"].clone())).collect::<Vec<_>>();
        assert_eq!(heard(1, "pulse1"), [("C4".into(), json!(60)), ("C5".into(), json!(72))]);
        assert_eq!(heard(2, "pulse1"), [("D#4".into(), json!(63)), ("D#5".into(), json!(75))]);
        assert_eq!(heard(3, "pulse1"), [("C3".into(), json!(48)), ("C4".into(), json!(60))]);
        assert_eq!(heard(2, "triangle"), [("G2".into(), json!(43))]);
        for play in 1..=3 { assert_eq!(heard(play, "noise"), [("C".into(), Value::Null), ("B".into(), Value::Null)], "play {play}: the noise is its letter, unmoved"); }
        let effect = |play: usize, lane: &str, word: &str| told.as_array().unwrap().iter().find(|e| e["play"] == play && e["lane"] == lane && e["effects"].get(word).is_some()).map(|e| e["effects"][word].clone()).unwrap();
        assert_eq!([1, 2, 3].map(|p| effect(p, "pulse1", "from")["pitch"].clone()), [json!("A4"), json!("C5"), json!("A3")], "the departure moves with its play");
        assert!([1, 2, 3].map(|p| effect(p, "pulse1", "arp")["offsets"].clone()).iter().all(|o| *o == json!([0, 4, 7])), "arp offsets are relative and stay as written");
        let swing = |pitch: u8| { let (up, down) = profile::swing(pitch, profile::ticks(pitch, 20.0)); (json!(round(up, 2)), json!(round(down, 2))) };
        for (play, pitch) in [(1, 69), (2, 72), (3, 57)] {
            let vib = effect(play, "pulse2", "vib");
            assert_eq!((vib["up_cents"].clone(), vib["down_cents"].clone()), swing(pitch), "play {play}: the swing is re-derived from the pitch that plays");
        }
    }

    #[test]
    fn rests_list_apart_from_the_notes_and_a_play_lists_by_time_then_by_lane() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern p bars 1 grid 8\nlane noise\n  1: C4 x1\nlane pulse2\n  1: - x2\n  3: E4 x2 vol 0\nlane pulse1 vol 6\n  1: C4 x2\n  3: - x2\n");
        let told = json(&strikes);
        assert_eq!(told.as_array().unwrap().iter().map(|e| (e["at"].clone(), e["lane"].clone(), e["volume"].clone())).collect::<Vec<_>>(), [(json!("1.1"), json!("pulse1"), json!(6)), (json!("1.1"), json!("noise"), json!(10)), (json!("1.3"), json!("pulse2"), json!(0))], "a muted note is still written, so it's listed at volume 0");
        assert_eq!(rests(&strikes), json!([
            { "pattern": "p", "play": 1, "lane": "pulse2", "at": "1.1", "end": "1.2", "start_seconds": 0.0, "end_seconds": 0.5, "start_sample": 0, "end_sample": 22_050, "line": 6 },
            { "pattern": "p", "play": 1, "lane": "pulse1", "at": "1.3", "end": "1.4", "start_seconds": 0.5, "end_seconds": 1.0, "start_sample": 22_050, "end_sample": 44_100, "line": 10 },
        ]), "rests run on the same clock and order as the notes, and carry only what silence has");
    }

    #[test]
    fn every_play_writes_its_rests_again() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane triangle\n  1: C3 x2\n  3: - x2\nplay\n  p\n  p +5\n");
        let told = rests(&strikes);
        assert_eq!(told.as_array().unwrap().iter().map(|r| (r["play"].clone(), r["start_sample"].clone(), r["line"].clone())).collect::<Vec<_>>(), [(json!(1), json!(44_100), json!(5)), (json!(2), json!(132_300), json!(5))]);
        assert_eq!(json(&strikes).as_array().unwrap().len(), 2, "the notes list never sees a rest");
    }

    #[test]
    fn duty_is_what_the_note_plays_whether_the_lane_or_the_note_set_it() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1 duty 12.5\n  1: C4 x1\n  2: D4 x1 duty 75\n  3: E4 x1\n  4: - x1\nlane pulse2\n  1: G4 x4\nlane triangle\n  1: C3 x4\nlane noise\n  1: C4 x4\npattern b bars 1 grid 4\nlane pulse1 duty 25\n  1: C4 x4\nplay\n  a\n  b\n");
        let told = json(&strikes);
        let duties: Vec<(Value, Value, Value, Value)> = told.as_array().unwrap().iter().map(|e| (e["pattern"].clone(), e["lane"].clone(), e["duty"].clone(), e["effects"].get("duty").cloned().unwrap_or(Value::Null))).collect();
        assert_eq!(duties, [
            (json!("a"), json!("pulse1"), json!(12.5), Value::Null), (json!("a"), json!("pulse2"), json!(50.0), Value::Null), (json!("a"), json!("triangle"), Value::Null, Value::Null), (json!("a"), json!("noise"), Value::Null, Value::Null),
            (json!("a"), json!("pulse1"), json!(75.0), json!(75.0)), (json!("a"), json!("pulse1"), json!(12.5), Value::Null), (json!("b"), json!("pulse1"), json!(25.0), Value::Null),
        ], "the lane header casts the duty, a note's own duty lasts that note only, a bare pulse lane plays 50, and each pattern block casts afresh");
    }

    #[test]
    fn a_section_starts_on_the_very_sample_its_first_note_does() {
        let (heard, strikes) = struck("song \"s\" tempo 133\npattern a bars 1 grid 16\nlane pulse1\n  1: C4 x16\npattern b bars 1 beats 7 grid 14\nlane pulse1\n  1: - x2\n  3: E4 x12\nplay\n  a\n  b\n  a\n");
        let sections = crate::vu::read(&heard.song, &heard.samples, &heard.plays).json();
        let edges: Vec<(Value, Value)> = sections["sections"].as_array().unwrap().iter().map(|s| (s["start_sample"].clone(), s["end_sample"].clone())).collect();
        let notes = json(&strikes);
        let quiet = rests(&strikes);
        assert_eq!([notes[0]["start_sample"].clone(), quiet[0]["start_sample"].clone(), notes[2]["start_sample"].clone()], [edges[0].0.clone(), edges[1].0.clone(), edges[2].0.clone()]);
        assert_eq!(notes[2]["end_sample"], edges[2].1, "the last note rings out on the section's last sample");
        assert_eq!(edges[0].1, edges[1].0, "sections tile the render");
    }

    #[test]
    fn every_field_is_present_and_effects_list_only_what_the_note_carries() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1 duty 50\n  1: A4 x8 vib 20ct 6hz 125ms duty 25 vol 12 fade\n  9: E5 x4\nlane triangle\n  1: A2 x16\n");
        let (up, down) = profile::swing(69, profile::ticks(69, 20.0));
        let told = json(&strikes);
        assert_eq!(told[0], json!({
            "pattern": "p", "play": 1, "lane": "pulse1", "pitch": "A4", "midi": 69, "at": "1.1", "end": "1.8",
            "start_seconds": 0.0, "end_seconds": 1.0, "start_sample": 0, "end_sample": 44_100, "volume": 12, "duty": 25.0, "line": 4,
            "effects": { "vib": { "rate_hz": 6.667, "delay_ms": 133.3, "up_cents": round(up, 2), "down_cents": round(down, 2) }, "duty": 25.0, "vol": 12, "fade": true },
        }));
        assert_eq!((told[1]["lane"].clone(), told[1]["volume"].clone(), told[1]["duty"].clone(), told[1]["effects"].clone()), (json!("triangle"), json!(15), Value::Null, json!({})), "the triangle has no knob, so it plays at the renderer's full 15 and has no duty to speak of");
        assert_eq!((told[2]["at"].clone(), told[2]["volume"].clone(), told[2]["duty"].clone(), told[2]["effects"].clone()), (json!("1.9"), json!(10), json!(50.0), json!({})), "the lane's duty 50 comes back once the override's note is done");
    }

    #[test]
    fn arps_and_slides_say_their_pace() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n  1: A4 x4 arp 0,4,7\n  5: C5 x4 from A4\n");
        let told = json(&strikes);
        assert_eq!(told[0]["effects"], json!({ "arp": { "offsets": [0, 4, 7], "steps": 30, "steps_per_second": 60.0 } }));
        assert_eq!(told[1]["effects"], json!({ "from": { "pitch": "A4", "midi": 69, "steps_per_second": 60.0 } }));
    }

    #[test]
    fn a_repeated_pattern_counts_its_plays_like_the_sections_do() {
        let (heard, strikes) = struck("song \"s\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x2\npattern b bars 1 grid 4\nlane pulse1\n  3: G4 x2\nplay\n  a\n  b\n  a\n");
        assert_eq!(strikes.iter().map(|s| (s.pattern.as_str(), s.play, s.samples.start)).collect::<Vec<_>>(), [("a", 1, 0), ("b", 1, 132_300), ("a", 2, 176_400)]);
        let sections = crate::vu::read(&heard.song, &heard.samples, &heard.plays).json();
        assert_eq!(sections["sections"].as_array().unwrap().iter().map(|s| (s["pattern"].clone(), s["play"].clone())).collect::<Vec<_>>(), [(json!("a"), json!(1)), (json!("b"), json!(1)), (json!("a"), json!(2))]);
    }

    #[test]
    fn a_vast_shelved_pattern_strikes_nothing_and_costs_nothing() {
        let (_, strikes) = struck("song \"s\" tempo 120\npattern endless bars 4000000000 beats 32 grid 255\nlane pulse1\n  1.1: C4 x9 fade\n  4000000000.1: A4 x60 arp 0,4,7\npattern ding bars 1 grid 4\nlane pulse1\n  1: C5 x2\nplay\n  ding\n");
        assert_eq!(strikes.iter().map(|s| (s.pattern.as_str(), s.at)).collect::<Vec<_>>(), [("ding", pos(1, 1))]);
        let vast = Song::parse("song \"s\" tempo 120\npattern endless bars 4000000000 beats 32 grid 255\n").unwrap();
        assert_eq!(render::sample_in(120, &vast.patterns[0], 4_000_000_000 * 255), Some(4_000_000_000 * 705_600), "a 32-beat bar at 120 is 16 s; the clock is arithmetic, never a walk across the bars");
        assert_eq!(render::sample_in(120, &vast.patterns[0], u64::MAX), None);
    }

    #[test]
    fn the_human_line_counts_and_points_at_the_json() {
        assert_eq!(tell(&[]), "events: no notes struck, no rests written; the lists print in the json only, so drop --human to read them");
        let (_, strikes) = struck("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x1\n  2: - x1\n  3: D4 x1\n");
        assert_eq!(tell(&strikes), "events: 2 notes struck, 1 rest written; the lists print in the json only, so drop --human to read them");
    }
}
