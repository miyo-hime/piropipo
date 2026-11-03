use piropipo::profile::{self, Card, Voice, named};
use piropipo::render;
use piropipo::score::{Bend, Lane, Note, Pattern, Position, Song};
use serde_json::{Value, json};

const SOUR: [(u16, &str); 3] = [(1, "minor 2nd"), (6, "tritone"), (11, "major 7th")];

#[derive(Debug, Clone, PartialEq)]
pub struct Rub { pub pattern: String, pub lanes: [String; 2], pub pitches: [i16; 2], pub arp: [bool; 2], pub interval: &'static str, pub semitones: u16, pub at: Position, pub end: Position, pub slots: u64, pub beats: f64, pub strong: bool }

impl Rub {
    fn json(&self) -> Value {
        json!({ "pattern": self.pattern, "at": self.at.to_string(), "end": self.end.to_string(), "lanes": self.lanes, "pitches": self.pitches.map(named), "arp": self.arp, "interval": self.interval, "semitones": self.semitones, "slots": self.slots, "beats": self.beats, "strong_beat": self.strong })
    }

    fn tell(&self) -> String {
        let voice = |lane: &str, pitch: i16, arp: bool| format!("{lane} {}{}", named(pitch), if arp { " (arp)" } else { "" });
        let ([one, other], [a, b], [arp_a, arp_b]) = (&self.lanes, self.pitches, self.arp);
        let spread = if self.semitones >= 12 { format!(" ({} semitones apart)", self.semitones) } else { String::new() };
        let accent = if self.strong { ", on a strong beat" } else { "" };
        format!("  {} {} through {}: {} against {}, {}{spread}, {} slot{}{accent}", self.pattern, self.at, self.end, voice(one, a, arp_a), voice(other, b, arp_b), self.interval, self.slots, if self.slots == 1 { "" } else { "s" })
    }
}

pub fn json(rubs: &[Rub]) -> Value { json!({ "clashes": rubs.iter().map(Rub::json).collect::<Vec<_>>() }) }

pub fn tell(rubs: &[Rub]) -> String {
    let count = match rubs.len() { 0 => "no harsh interval".to_owned(), 1 => "1 harsh interval".to_owned(), n => format!("{n} harsh intervals") };
    std::iter::once(format!("harmony advisory: {count} held 3/4 of a beat or longer")).chain(rubs.iter().map(Rub::tell)).collect::<Vec<_>>().join("\n")
}

pub fn rubs(song: &Song) -> Vec<Rub> {
    let Ok(card) = profile::card(song.profile.as_deref()) else { return Vec::new() };
    let mut played: Vec<&Pattern> = Vec::new();
    for pattern in song.setlist.iter().filter_map(|cue| song.pattern(&cue.pattern)) {
        if !played.iter().any(|p| p.name == pattern.name) { played.push(pattern); }
    }
    played.into_iter().flat_map(|p| rubs_in(song.tempo, card, p)).collect()
}

fn rubs_in(tempo: u32, card: &Card, pattern: &Pattern) -> Vec<Rub> {
    let mut voices: Vec<(usize, &str, Vec<Held>)> = pattern.lanes.iter().filter_map(|lane| voiced(tempo, pattern.grid, card, lane)).collect();
    voices.sort_by_key(|(seat, ..)| *seat);
    let mut found = Vec::new();
    for (i, (_, one, ones)) in voices.iter().enumerate() {
        for (_, other, others) in voices.iter().skip(i + 1) { found.extend(sustain(grind(ones, others)).into_iter().filter_map(|run| rub(pattern, [*one, *other], &run))); }
    }
    found.sort_by_key(|r| r.at);
    found
}

fn rub(pattern: &Pattern, [one, other]: [&str; 2], run: &Overlap) -> Option<Rub> {
    let grid = pattern.grid;
    let slots = run.to - run.from;
    if !lingers(slots, grid) { return None; }
    let interval = sour(run.one.pitch, run.other.pitch)?;
    let beats = (slots * 4) as f64 / f64::from(grid);
    Some(Rub {
        pattern: pattern.name.clone(), lanes: [one.into(), other.into()], pitches: [run.one.pitch, run.other.pitch], arp: [run.one.arp, run.other.arp],
        interval, semitones: run.one.pitch.abs_diff(run.other.pitch), at: Position::from_tick(run.from, grid), end: Position::from_tick(run.to.saturating_sub(1), grid),
        slots, beats: (beats * 100.0).round() / 100.0, strong: accented(run.from, run.to, grid),
    })
}

fn sour(a: i16, b: i16) -> Option<&'static str> { SOUR.iter().find(|(class, _)| *class == a.abs_diff(b) % 12).map(|(_, name)| *name) }

fn lingers(slots: u64, grid: u32) -> bool { slots * 16 >= 3 * u64::from(grid) }

// doubled ticks put beats 1 and 3 of every bar on the multiples of the grid, odd grids included
fn accented(from: u64, to: u64, grid: u32) -> bool { let grid = u64::from(grid); (2 * from).div_ceil(grid) * grid < 2 * to }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tone { pitch: i16, arp: bool }

#[derive(Debug)]
struct Held { from: u64, to: u64, tones: Vec<Tone> }

#[derive(Debug)]
struct Overlap { one: Tone, other: Tone, from: u64, to: u64 }

fn voiced<'l>(tempo: u32, grid: u32, card: &Card, lane: &'l Lane) -> Option<(usize, &'l str, Vec<Held>)> {
    let (seat, part) = card.parts.iter().enumerate().find(|(_, p)| p.name == lane.name)?;
    if part.voice == Voice::Noise { return None; }
    let level = part.setup(&lane.params).ok()?.volume;
    let mut notes: Vec<Held> = lane.notes.iter().filter(|n| n.ornaments.vol.unwrap_or(level) > 0).filter_map(|n| held(tempo, grid, n)).collect();
    notes.sort_unstable_by_key(|h| h.from);
    Some((seat, lane.name.as_str(), notes))
}

fn held(tempo: u32, grid: u32, note: &Note) -> Option<Held> {
    let pitch = note.pitch?;
    let from = note.at.tick(grid);
    let to = from + u64::from(note.len);
    let written = Tone { pitch: pitch.into(), arp: false };
    match &note.ornaments.bend {
        Some(Bend::Arpeggio(offsets)) => {
            let mut tones: Vec<Tone> = offsets.iter().map(|o| Tone { pitch: i16::from(pitch).saturating_add(*o), arp: true }).collect();
            tones.sort_unstable_by_key(|t| t.pitch);
            tones.dedup();
            Some(Held { from, to, tones })
        }
        Some(Bend::Slide(departure)) => {
            let (dwells, _) = render::arp_rate(tempo, grid, note.len);
            let walk = u64::from(pitch.abs_diff(*departure)) * u64::from(note.len);
            let landed = from + (2 * walk + dwells).checked_div(2 * dwells)?;
            (landed < to).then(|| Held { from: landed, to, tones: vec![written] })
        }
        _ => Some(Held { from, to, tones: vec![written] }),
    }
}

fn grind(a: &[Held], b: &[Held]) -> Vec<Overlap> {
    let mut found = Vec::new();
    let (mut i, mut j) = (0, 0);
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        let (from, to) = (x.from.max(y.from), x.to.min(y.to));
        if from < to {
            for &one in &x.tones { found.extend(y.tones.iter().filter(|other| sour(one.pitch, other.pitch).is_some()).map(|&other| Overlap { one, other, from, to })); }
        }
        if x.to <= y.to { i += 1; } else { j += 1; }
    }
    found
}

fn sustain(mut overlaps: Vec<Overlap>) -> Vec<Overlap> {
    overlaps.sort_unstable_by_key(|o| (o.one.pitch, o.other.pitch, o.from));
    let mut runs: Vec<Overlap> = Vec::new();
    for o in overlaps {
        match runs.last_mut() {
            Some(run) if (run.one.pitch, run.other.pitch, run.to) == (o.one.pitch, o.other.pitch, o.from) => { run.to = o.to; run.one.arp |= o.one.arp; run.other.arp |= o.other.arp; }
            _ => runs.push(o),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use piropipo::score::Cue;

    fn song(text: &str) -> Song { Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}")) }

    fn heard(text: &str) -> Vec<Rub> { rubs(&song(text)) }

    fn duo(grid: u32, pulse1: &str, pulse2: &str) -> String { format!("song \"s\" tempo 120\npattern p bars 2 grid {grid}\nlane pulse1\n{pulse1}lane pulse2\n{pulse2}") }

    fn pos(bar: u32, slot: u32) -> Position { Position { bar, slot } }

    fn gist(rubs: &[Rub]) -> Vec<String> { rubs.iter().map(|r| format!("{} {} {} x{}", named(r.pitches[0]), named(r.pitches[1]), r.interval, r.slots)).collect() }

    fn gisted(rubs: &[Rub]) -> Vec<(&str, &str, &'static str, u64)> { rubs.iter().map(|r| (r.lanes[0].as_str(), r.lanes[1].as_str(), r.interval, r.slots)).collect() }

    #[test]
    fn only_the_minor_second_the_tritone_and_the_major_seventh_are_sour_at_any_octave() {
        assert_eq!((sour(60, 61), sour(61, 60), sour(60, 66), sour(60, 71)), (Some("minor 2nd"), Some("minor 2nd"), Some("tritone"), Some("major 7th")));
        assert_eq!((sour(60, 73), sour(48, 78), sour(36, 83)), (Some("minor 2nd"), Some("tritone"), Some("major 7th")), "compound intervals keep their class");
        for d in [0, 2, 3, 4, 5, 7, 8, 9, 10, 12, 14, 19, 24] { assert_eq!(sour(60, 60 + d), None, "{d} semitones is not sour"); }
    }

    #[test]
    fn sustained_means_three_sixteenths_of_a_bar_on_any_grid() {
        for (grid, short, long, beats) in [(16, 2, 3, 0.75), (8, 1, 2, 1.0), (12, 2, 3, 1.0), (24, 4, 5, 0.83), (32, 5, 6, 0.75)] {
            assert_eq!(heard(&duo(grid, &format!("  1.1: C4 x{short}\n"), &format!("  1.1: C#4 x{short}\n"))), Vec::new(), "grid {grid}, {short} slots is a passing rub");
            let rubs = heard(&duo(grid, &format!("  1.1: C4 x{long}\n"), &format!("  1.1: C#4 x{long}\n")));
            assert_eq!(rubs.iter().map(|r| (r.slots, r.beats)).collect::<Vec<_>>(), [(long, beats)], "grid {grid}");
        }
        assert!(lingers(1, 4) && lingers(1, 3) && lingers(1, 1), "on a coarse grid one slot is already a beat or more");
        assert!(!lingers(2, 11) && lingers(3, 11));
    }

    #[test]
    fn a_restruck_pitch_keeps_the_rub_and_a_rest_or_a_new_pitch_ends_it() {
        let restruck = heard(&duo(16, "  1.1: C4 x1\n  1.2: C4 x1 vol 5\n  1.3: C4 x1 fade\n", "  1.1: C#4 x3\n"));
        assert_eq!(gist(&restruck), ["C4 C#4 minor 2nd x3"]);
        assert_eq!((restruck[0].at, restruck[0].end), (pos(1, 1), pos(1, 3)));
        assert_eq!(heard(&duo(16, "  1.1: C4 x1\n  1.2: - x1\n  1.3: C4 x1\n", "  1.1: C#4 x3\n")), Vec::new(), "a rest splits it into two blips");
        assert_eq!(heard(&duo(16, "  1.1: C4 x2\n  1.3: D4 x2\n", "  1.1: C#4 x4\n")), Vec::new(), "two different two-slot rubs are two passing tones, not one held clash");
    }

    #[test]
    fn a_rub_carries_across_the_barline() {
        let rubs = heard(&duo(16, "  1.15: E4 x4\n", "  1.13: Bb3 x8\n"));
        assert_eq!(gist(&rubs), ["E4 A#3 tritone x4"]);
        assert_eq!((rubs[0].at, rubs[0].end, rubs[0].strong), (pos(1, 15), pos(2, 2), true));
    }

    #[test]
    fn an_arp_sounds_as_its_whole_chord() {
        let rubs = heard(&duo(16, "  1.1: C4 x4 arp 0,4,7\n", "  1.1: F#4 x4\n"));
        assert_eq!(gist(&rubs), ["C4 F#4 tritone x4", "G4 F#4 minor 2nd x4"]);
        assert!(rubs.iter().all(|r| r.arp == [true, false]));
        assert_eq!(heard(&duo(16, "  1.1: C4 x4 arp 0,4,7\n", "  1.1: E4 x4\n")), Vec::new(), "a chord tone in the other lane is consonant");
        let both = heard(&duo(16, "  1.1: C4 x4 arp 0,4,7\n", "  1.1: C#4 x4 arp 0,4,7\n"));
        assert_eq!(gist(&both).len(), 4, "C-C#, E-F, G-G#, and G against C#: {:?}", gist(&both));
        assert!(both.iter().all(|r| r.arp == [true, true]));
    }

    #[test]
    fn an_arp_rub_holds_across_a_change_of_chord_that_keeps_the_tone() {
        let rubs = heard(&duo(16, "  1.1: C4 x2 arp 0,4,7\n  1.3: G4 x2 arp 0,4\n", "  1.1: F#4 x4\n"));
        assert_eq!(gist(&rubs), ["G4 F#4 minor 2nd x4"], "C4 drops out after two slots, G4 rides through both chords");
        assert_eq!(rubs[0].arp, [true, false]);
    }

    #[test]
    fn a_slide_counts_from_where_it_lands_not_from_its_walk() {
        let scoop = heard(&duo(16, "  1.1: C#5 x3 from C5\n", "  1.1: C4 x3\n"));
        assert_eq!(gist(&scoop), ["C#5 C4 minor 2nd x3"], "a one-semitone scoop settles inside its first slot");
        assert_eq!(heard(&duo(16, "  1.1: C#5 x4 from C#3\n", "  1.1: C4 x4\n")), Vec::new(), "two octaves of walk eat most of a four-slot note, and the walk grinds nothing");
        let long = heard(&duo(16, "  1.1: C#5 x8 from C#3\n", "  1.1: C4 x8\n"));
        assert_eq!(gist(&long), ["C#5 C4 minor 2nd x5"]);
        assert_eq!(long[0].at, pos(1, 4));
    }

    #[test]
    fn a_vibrato_is_heard_at_its_written_pitch() {
        assert_eq!(gist(&heard(&duo(16, "  1.1: C#4 x4 vib 20ct 6hz\n", "  1.1: C4 x4\n"))), ["C#4 C4 minor 2nd x4"]);
    }

    #[test]
    fn the_noise_has_no_pitch_and_the_triangle_does() {
        let band = "song \"s\" tempo 120\npattern p bars 1 grid 16\nlane noise\n  1: C#4 x4\nlane triangle\n  1: C#3 x4\nlane pulse1\n  1: C4 x4\n";
        assert_eq!(gisted(&heard(band)), [("pulse1", "triangle", "major 7th", 4)], "lanes pair in card order, noise sits out");
    }

    #[test]
    fn a_muted_note_grinds_against_nothing() {
        assert_eq!(heard(&duo(16, "  1.1: C4 x4 vol 0\n", "  1.1: C#4 x4\n")), Vec::new());
        assert_eq!(heard("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1 vol 0\n  1: C4 x4\nlane pulse2\n  1: C#4 x4\n"), Vec::new());
        assert_eq!(heard("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1 vol 0\n  1: C4 x4 vol 6\nlane pulse2\n  1: C#4 x4\n").len(), 1, "a note's own vol speaks over the lane's");
    }

    #[test]
    fn a_strong_beat_is_beat_one_or_three_sounding_under_the_rub() {
        let at = |start: &str| heard(&duo(16, &format!("  {start}: C4 x3\n"), &format!("  {start}: C#4 x3\n")))[0].strong;
        assert_eq!(["1.1", "1.2", "1.6", "1.7", "1.9", "1.14", "1.15"].map(at), [true, false, false, true, true, false, true]);
        assert!(accented(4, 7, 12) && !accented(1, 4, 12), "grid 12 puts beat 3 on slot 7");
        assert!(accented(1, 2, 3) && !accented(2, 3, 3), "grid 3 puts beat 3 halfway through slot 2");
    }

    #[test]
    fn a_transposed_play_moves_every_pitched_lane_together_so_one_pass_per_pattern_holds() {
        let trio = song("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n  1: C4 x4 arp 0,4,7\n  5: F4 x4 from D4\nlane pulse2\n  1: F#4 x8 vib 20ct 6hz\nlane triangle\n  1: C#3 x8\nlane noise\n  1: C#4 x8\n");
        let card = profile::card(None).unwrap();
        let written = rubs_in(120, card, &trio.patterns[0]);
        assert!(!written.is_empty());
        for shift in [-12, -1, 5, 7] {
            let moved = render::transposed(&trio, &trio.patterns[0], &Cue { pattern: "p".into(), shift, line: 9 }).unwrap();
            let played = rubs_in(120, card, &moved);
            let shape = |r: &Rub| (r.lanes.clone(), r.interval, r.semitones, r.at, r.end, r.slots, r.strong, r.arp);
            assert_eq!(played.iter().map(shape).collect::<Vec<_>>(), written.iter().map(shape).collect::<Vec<_>>(), "shift {shift}");
            assert!(played.iter().zip(&written).all(|(p, w)| p.pitches == w.pitches.map(|x| x + i16::from(shift))), "shift {shift}");
        }
    }

    #[test]
    fn each_played_pattern_is_heard_once_and_an_unplayed_one_never() {
        let pair = |name: &str, pulse1: &str, pulse2: &str| format!("pattern {name} bars 1 grid 16\nlane pulse1\n  1: {pulse1} x4\nlane pulse2\n  1: {pulse2} x4\n");
        let text = format!("song \"s\" tempo 120\nplay\n  b\n  a +2\n  b\n  a\n{}{}{}", pair("a", "C4", "C#4"), pair("b", "E4", "Bb4"), pair("shelf", "C4", "B4"));
        let rubs = heard(&text);
        assert_eq!(rubs.iter().map(|r| (r.pattern.as_str(), r.interval)).collect::<Vec<_>>(), [("b", "tritone"), ("a", "minor 2nd")]);
    }

    #[test]
    fn a_vast_pattern_is_swept_note_by_note_never_walked() {
        let vast = song("song \"s\" tempo 120\npattern vast bars 4000000000 grid 255\nlane pulse1\n  1.1: C4 x255\n  4000000000.1: C4 x255\nlane pulse2\n  1.1: E4 x255\n  4000000000.1: C#4 x255\n");
        let rubs = rubs_in(120, profile::card(None).unwrap(), &vast.patterns[0]);
        assert_eq!(rubs.iter().map(|r| (r.at, r.end, r.slots)).collect::<Vec<_>>(), [(pos(4_000_000_000, 1), pos(4_000_000_000, 255), 255)]);
        let shelved = crate::listen::take(&format!("{}pattern ding bars 1 grid 4\nlane pulse1\n  1: C5 x1\nplay\n  ding\n", "song \"s\" tempo 120\npattern vast bars 4000000000 grid 255\nlane pulse1\n  1.1: C4 x255\nlane pulse2\n  1.1: C#4 x255\n")).unwrap();
        assert!(shelved.rubs.is_empty(), "an unplayed pattern isn't heard, however long");
    }

    #[test]
    fn a_score_full_of_rubs_still_passes_and_warns_nothing() {
        let take = crate::listen::take(&duo(16, "  1.1: C4 x16\n  2.1: B3 x16\n", "  1.1: C#4 x16\n  2.1: F4 x16\n")).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(take.warnings.is_empty(), "{:?}", take.warnings);
        assert_eq!(gist(&take.rubs), ["C4 C#4 minor 2nd x16", "B3 F4 tritone x16"]);
    }

    #[test]
    fn the_json_names_every_field_and_the_human_line_reads_like_a_sentence() {
        let rubs = heard(&duo(16, "  1.1: E5 x4\n  1.5: - x4\n  1.9: C4 x2 arp 0,4,7\n", "  1.1: F4 x4\n  1.5: - x4\n  1.9: F#4 x2\n"));
        let [rub] = &rubs[..] else { panic!("{rubs:?}") };
        assert_eq!(json(&rubs), json!({ "clashes": [{ "pattern": "p", "at": "1.1", "end": "1.4", "lanes": ["pulse1", "pulse2"], "pitches": ["E5", "F4"], "arp": [false, false], "interval": "major 7th", "semitones": 11, "slots": 4, "beats": 1.0, "strong_beat": true }] }));
        assert_eq!(tell(&rubs), "harmony advisory: 1 harsh interval held 3/4 of a beat or longer\n  p 1.1 through 1.4: pulse1 E5 against pulse2 F4, major 7th, 4 slots, on a strong beat");
        assert_eq!(Rub { semitones: 23, pitches: [53, 76], arp: [true, false], strong: false, ..rub.clone() }.tell(), "  p 1.1 through 1.4: pulse1 F3 (arp) against pulse2 E5, major 7th (23 semitones apart), 4 slots");
        assert_eq!((json(&[]), tell(&[])), (json!({ "clashes": [] }), "harmony advisory: no harsh interval held 3/4 of a beat or longer".to_owned()));
    }
}
