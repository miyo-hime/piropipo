use crate::profile::{self, Breach, Part, Voice};
use crate::score::{Note, Pattern, Position, Song};
use std::fmt;

pub const SAMPLE_RATE: u16 = 44100;

const VOLUME: u8 = 10;

const SPELLING: [&str; 12] = ["c", "c+", "d", "d+", "e", "f", "f+", "g", "g+", "a", "a+", "b"];

impl Voice {
    fn volume(self) -> u8 { if self == Voice::Triangle { 15 } else { VOLUME } }

    fn spell(self, pitch: u8) -> Option<(i32, String)> {
        let class = usize::from(pitch % 12);
        let octave = i32::from(pitch / 12) - 1;
        // ffmml's triangle sounds an octave below a pulse on the same `o`, like the 2A03's.
        // its noise reads only the letter, and `c` is the shortest period, so the class runs backwards to keep higher written pitches brighter
        let (octave, letter) = match self {
            Voice::Pulse => (octave, SPELLING[class]),
            Voice::Triangle => (octave + 1, SPELLING[class]),
            Voice::Noise => (4, SPELLING[11 - class]),
        };
        match octave {
            2..=7 => Some((octave, letter.into())),
            // `o` stops at 2 and 7, but ffmml pitches c through g+ off the A below them, so stacked flats on o2's c reach down to A, and stacked sharps on o7's b climb to the G# above
            1 if class >= 9 => Some((2, format!("c{}", "-".repeat(12 - class)))),
            8 if class <= 8 => Some((7, format!("b{}", "+".repeat(class + 1)))),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    Profile(String),
    Tempo(u32),
    Grid(u32),
    Lane(String),
    Param { lane: String, key: String, takes: &'static [&'static str] },
    Volume { lane: String, key: String },
    Duty { lane: String, value: String },
    Range { lane: String, at: Position, pitch: u8, playable: &'static str },
    Misfit { lane: String, at: Position },
    Mml(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RenderError::Profile(profile) => write!(f, "{}", Breach::Profile(profile.clone())),
            RenderError::Tempo(tempo) => write!(f, "tempo {tempo} is out of the renderer's reach; keep it between 1 and 255"),
            RenderError::Grid(grid) => write!(f, "grid {grid} is out of the renderer's reach; keep it between 1 and 255 slots per bar"),
            RenderError::Lane(lane) => write!(f, "{}", Breach::Lane(lane.clone())),
            RenderError::Param { lane, key, takes } => write!(f, "lane {lane}: {}", Breach::Param { key: key.clone(), takes }),
            RenderError::Volume { lane, key } => write!(f, "lane {lane}: {}", Breach::Volume(key.clone())),
            RenderError::Duty { lane, value } => write!(f, "lane {lane}: {}", Breach::Duty(value.clone())),
            RenderError::Range { lane, at, pitch, playable } => write!(f, "lane {lane}, at {at}: {}", Breach::Range { at: *at, pitch: *pitch, playable }),
            RenderError::Misfit { lane, at } => write!(f, "lane {lane}, at {at}: this note doesn't fit the pattern (an overlap, a zero length, or a spot outside it); the parser's checks would have refused it"),
            RenderError::Mml(why) => write!(f, "ffmml refused the compiled score, which is a renderer bug:\n{why}"),
        }
    }
}

impl std::error::Error for RenderError {}

fn charged(lane: &str, breach: Breach) -> RenderError {
    let lane = lane.to_owned();
    match breach {
        Breach::Profile(name) => RenderError::Profile(name),
        Breach::Lane(name) => RenderError::Lane(name),
        Breach::Param { key, takes } => RenderError::Param { lane, key, takes },
        Breach::Volume(key) => RenderError::Volume { lane, key },
        Breach::Duty(value) => RenderError::Duty { lane, value },
        Breach::Range { at, pitch, playable } => RenderError::Range { lane, at, pitch, playable },
    }
}

pub fn pattern(song: &Song, pattern: &Pattern) -> Result<Vec<i16>, RenderError> { perform(&transcribe(song, pattern)?) }

fn transcribe(song: &Song, pattern: &Pattern) -> Result<String, RenderError> {
    let card = profile::card(song.profile.as_deref()).map_err(|breach| charged("", breach))?;
    if !(1..=255).contains(&song.tempo) { return Err(RenderError::Tempo(song.tempo)); }
    if !(1..=255).contains(&pattern.grid) { return Err(RenderError::Grid(pattern.grid)); }
    if let Some(stray) = pattern.lanes.iter().find_map(|l| card.part(&l.name).err()) { return Err(charged("", stray)); }
    let lines = card.parts.iter().map(|part| line(song.tempo, pattern, part)).collect::<Result<Vec<_>, _>>()?;
    Ok(lines.join("\n"))
}

fn line(tempo: u32, pattern: &Pattern, part: &Part) -> Result<String, RenderError> {
    let Part { name, channel, voice, .. } = *part;
    let Pattern { bars, grid, .. } = *pattern;
    let span = u64::from(bars) * u64::from(grid);
    let lane = pattern.lanes.iter().find(|l| l.name == name);
    // a triangle that rests before its first note idles at full positive output in ffmml (a DC shelf), so every voice starts at v0 and finds its volume on its first note
    let mut words = vec![channel.to_string(), format!("t{tempo}"), "v0".to_owned()];
    if let Some(timbre) = part.duty(lane.map_or(&[][..], |l| l.params.as_slice())).map_err(|breach| charged(name, breach))? { words.push(format!("@{timbre}")); }
    let mut notes: Vec<(&Note, u8)> = lane.iter().flat_map(|l| &l.notes).filter_map(|n| Some((n, n.pitch?))).collect();
    notes.sort_by_key(|(n, _)| n.at.tick(grid));
    let mut cursor = 0;
    for (i, (note, pitch)) in notes.into_iter().enumerate() {
        let Position { bar, slot } = note.at;
        let start = note.at.tick(grid);
        let end = start + u64::from(note.len);
        if bar == 0 || slot == 0 || slot > grid || note.len == 0 || start < cursor || end > span { return Err(RenderError::Misfit { lane: name.into(), at: note.at }); }
        part.plays(note).map_err(|breach| charged(name, breach))?;
        let (octave, letter) = voice.spell(pitch).ok_or_else(|| charged(name, Breach::Range { at: note.at, pitch, playable: part.playable }))?;
        words.extend(lengths(grid, start - cursor).map(|d| format!("r{d}")));
        if i == 0 { words.push(format!("v{}", voice.volume())); }
        words.push(format!("o{octave}"));
        // `&` and never `^`: ffmml 0.1.2 only looks one command back for the note a `^` extends, so `c4^4^4` errors on the second tie
        words.push(lengths(grid, end - start).map(|d| format!("{letter}{d}")).collect::<Vec<_>>().join("&"));
        cursor = end;
    }
    words.extend(lengths(grid, span - cursor).map(|d| format!("r{d}")));
    Ok(words.join(" "))
}

fn lengths(grid: u32, slots: u64) -> impl Iterator<Item = u32> {
    let mut left = slots;
    std::iter::from_fn(move || {
        let reach = u32::try_from(left).unwrap_or(u32::MAX).min(grid);
        let piece = (1..=reach).rev().find(|p| grid.is_multiple_of(*p))?;
        left -= u64::from(piece);
        Some(grid / piece)
    })
}

fn perform(mml: &str) -> Result<Vec<i16>, RenderError> {
    let music: ffmml::Music = mml.parse().map_err(|e: ffmml::ParseMusicError| RenderError::Mml(e.to_string()))?;
    let mut player = music.play(SAMPLE_RATE);
    let samples = (&mut player).map(ffmml::Sample::to_i16).collect();
    match player.take_last_error() {
        Some(e) => Err(RenderError::Mml(e.text(mml).to_string())),
        None => Ok(samples),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::named;

    fn spec_example() -> &'static str {
        let doc = include_str!("../docs/format.md");
        let (_, opened) = doc.split_once("```\n").expect("format.md lost its example");
        opened.split_once("```").expect("format.md example fence never closes").0
    }

    fn song(text: &str) -> Song { Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}")) }

    fn render(text: &str) -> Result<Vec<i16>, RenderError> {
        let song = song(text);
        pattern(&song, &song.patterns[0])
    }

    fn samples_in(slots: u64, tempo: u64, grid: u64) -> usize { usize::try_from((slots * 240 * u64::from(SAMPLE_RATE)).div_ceil(tempo * grid)).unwrap() }

    fn assert_lit_exactly(samples: &[i16], lit: &[(u64, u64)], slot: u64) {
        let mut on = vec![false; samples.len()];
        for &(start, len) in lit { on[usize::try_from(start * slot).unwrap()..usize::try_from((start + len) * slot).unwrap()].fill(true); }
        for (i, (&s, &want)) in samples.iter().zip(&on).enumerate() { assert_eq!(s != 0, want, "sample {i} (slot {}) should be {}", i as u64 / slot, if want { "sounding" } else { "silent" }); }
    }

    #[test]
    fn spec_example_renders_to_its_written_length() {
        let samples = render(spec_example()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(samples.len(), samples_in(32, 112, 16));
        assert_eq!(samples.len(), 189_000);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!((4_000..32_000).contains(&peak), "peak {peak} is either a whisper or a wall");
        let quiet = samples.iter().filter(|s| **s == 0).count();
        assert!(quiet < samples.len() / 4, "{quiet} of {} samples are silent", samples.len());
    }

    #[test]
    fn one_whole_note_lasts_four_beats() {
        let samples = render("song \"s\" tempo 105\npattern p bars 1 grid 16\nlane pulse1\n  1.1: C4 x16\n").unwrap();
        assert_eq!(samples.len(), 100_800);
        assert!(samples.iter().all(|s| *s != 0));
    }

    #[test]
    fn a_rest_gap_is_silent_to_the_sample() {
        let samples = render("song \"s\" tempo 105\npattern p bars 1 grid 16\nlane pulse1\n  1.1: A4 x4\n  1.13: A4 x4\nlane triangle\n").unwrap();
        assert_eq!(samples.len(), 100_800);
        assert_lit_exactly(&samples, &[(0, 4), (12, 4)], 6_300);
    }

    #[test]
    fn odd_grids_and_barlines_land_on_their_ticks() {
        let samples = render("song \"s\" tempo 105\npattern p bars 2 grid 12\nlane pulse2 duty 12\n  1.2: E5 x1\n  1.7: G4 x8 ; crosses into bar 2\n  2.5: - x2\n  2.12: B3 x1\n").unwrap();
        assert_eq!(samples.len(), samples_in(24, 105, 12));
        assert_lit_exactly(&samples, &[(1, 1), (6, 8), (23, 1)], 8_400);
    }

    #[test]
    fn mml_reads_the_way_the_grid_does() {
        let song = song("song \"s\" tempo 120\npattern p bars 2 grid 16\nlane pulse1 duty 25\n  1.5: C#4 x5\n  2.1: - x4\n  2.9: G5 x8\nlane triangle\n  1.1: E2 x24\n");
        let mml = transcribe(&song, &song.patterns[0]).unwrap();
        assert_eq!(mml.lines().collect::<Vec<_>>(), [
            "A t120 v0 @1 r4 v10 o4 c+4&c+16 r2 r4 r8 r16 o5 g2",
            "B t120 v0 @2 r1 r1",
            "C t120 v0 v15 o3 e1&e2 r2",
            "D t120 v0 r1 r1",
        ]);
    }

    #[test]
    fn lengths_always_sum_to_the_slots_asked_for() {
        for grid in 1..=96u32 {
            for slots in 0..=300u64 {
                let pieces: Vec<u32> = lengths(grid, slots).collect();
                assert!(pieces.iter().all(|d| (1..=255).contains(d) && grid.is_multiple_of(*d)), "grid {grid}, {slots} slots: {pieces:?}");
                assert_eq!(pieces.iter().map(|d| u64::from(grid / d)).sum::<u64>(), slots, "grid {grid}: {pieces:?}");
            }
        }
    }

    fn sounding(lane: &str, pitch: u8) -> f32 { sounding_on("nes", lane, pitch) }

    fn sounding_on(card: &str, lane: &str, pitch: u8) -> f32 {
        let song = song(&format!("song \"s\" tempo 120 profile {card}\npattern p bars 1 grid 4\nlane {lane}\n  1: {} x4\n", named(pitch)));
        let channel = profile::card(Some(card)).unwrap().part(lane).unwrap().channel;
        let music: ffmml::Music = transcribe(&song, &song.patterns[0]).unwrap().parse().unwrap();
        let mut player = music.play(SAMPLE_RATE);
        player.by_ref().take(64).for_each(drop);
        player.channels().find(|c| c.channel_name().as_char() == channel).unwrap().frequency()
    }

    #[test]
    fn every_playable_pitch_sounds_at_concert_pitch() {
        for (lane, low, high) in [("pulse1", 36, 107), ("pulse2", 36, 107), ("triangle", 24, 95)] {
            for pitch in low..=high {
                let heard = sounding(lane, pitch);
                let concert = 440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0);
                assert!((heard / concert - 1.0).abs() < 1e-4, "{lane} {} sounds at {heard} Hz, wants {concert} Hz", named(pitch));
            }
        }
        assert!((sounding("pulse1", 60) - 261.63).abs() < 0.01);
        assert!((sounding("triangle", 69) - 440.0).abs() < 0.01);
    }

    #[test]
    fn noise_gets_brighter_as_the_written_pitch_climbs() {
        let periods: Vec<f32> = (60..72).map(|p| sounding("noise", p)).collect();
        assert!(periods.windows(2).all(|w| w[1] < w[0]), "{periods:?}");
        assert_eq!((periods[0], periods[11]), (508.0, 4.0));
        assert_eq!(sounding("noise", 24), sounding("noise", 96));
    }

    #[test]
    fn the_edges_of_range_are_refused_by_name() {
        let pulse_too_low = render("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane pulse1\n  2: B1 x1\n").unwrap_err();
        assert_eq!(pulse_too_low, RenderError::Range { lane: "pulse1".into(), at: Position { bar: 1, slot: 2 }, pitch: 35, playable: "C2 through B7" });
        assert_eq!(pulse_too_low.to_string(), "lane pulse1, at 1.2: B1 is out of range; this lane plays C2 through B7");
        assert!(matches!(render("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane pulse2\n  1: C8 x1\n"), Err(RenderError::Range { pitch: 108, .. })));
        assert!(matches!(render("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane triangle\n  1: B0 x1\n"), Err(RenderError::Range { pitch: 23, .. })));
        assert!(matches!(render("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane triangle\n  1: C7 x1\n"), Err(RenderError::Range { pitch: 96, .. })));
    }

    #[test]
    fn nes_free_sounds_at_concert_pitch_out_to_the_engine_edges() {
        for (lane, low, high) in [("pulse2", 33, 116), ("triangle", 21, 104)] {
            for pitch in low..=high {
                let heard = sounding_on("nes-free", lane, pitch);
                let concert = 440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0);
                assert!((heard / concert - 1.0).abs() < 1e-4, "nes-free {lane} {} sounds at {heard} Hz, wants {concert} Hz", named(pitch));
            }
        }
        assert_eq!(sounding_on("nes-free", "noise", 60), sounding("noise", 60));
    }

    #[test]
    fn nes_free_draws_its_line_where_the_engine_does() {
        let free = |lane: &str, pitch: &str| song(&format!("song \"s\" tempo 90 profile nes-free\npattern p bars 1 grid 4\nlane {lane}\n  1: {pitch} x3\n"));
        let low = free("pulse1", "A1");
        assert_eq!(transcribe(&low, &low.patterns[0]).unwrap().lines().next(), Some("A t90 v0 @2 v10 o2 c---2&c---4 r4"));
        let high = free("triangle", "G#7");
        assert_eq!(transcribe(&high, &high.patterns[0]).unwrap().lines().nth(2), Some("C t90 v0 v15 o7 b+++++++++2&b+++++++++4 r4"));
        assert_eq!(pattern(&high, &high.patterns[0]).unwrap().len(), samples_in(4, 90, 4));
        let under = free("pulse1", "G#1");
        assert_eq!(pattern(&under, &under.patterns[0]).unwrap_err().to_string(), "lane pulse1, at 1.1: G#1 is out of range; this lane plays A1 through G#8");
        let over = free("triangle", "A7");
        assert!(matches!(pattern(&over, &over.patterns[0]), Err(RenderError::Range { pitch: 105, playable: "A0 through G#7", .. })));
    }

    #[test]
    fn the_triangle_refuses_a_volume_it_does_not_have() {
        let refused = render("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane triangle vol 8\n  1: C3 x1\n").unwrap_err();
        assert_eq!(refused, RenderError::Volume { lane: "triangle".into(), key: "vol".into() });
        assert_eq!(refused.to_string(), "lane triangle: doesn't take `vol`; the triangle has no volume, it's either on or off");
    }

    #[test]
    fn scores_the_chip_cannot_play_report_instead_of_panicking() {
        let refusal = |head: &str, lane: &str| render(&format!("{head}\npattern p bars 1 grid 4\n{lane}\n  1: C4 x1\n")).unwrap_err();
        assert_eq!(refusal("song \"s\" tempo 90 profile gameboy", "lane pulse1"), RenderError::Profile("gameboy".into()));
        assert_eq!(refusal("song \"s\" tempo 300", "lane pulse1"), RenderError::Tempo(300));
        assert_eq!(refusal("song \"s\" tempo 90", "lane bass"), RenderError::Lane("bass".into()));
        assert_eq!(refusal("song \"s\" tempo 90", "lane pulse1 duty 33"), RenderError::Duty { lane: "pulse1".into(), value: "33".into() });
        assert_eq!(refusal("song \"s\" tempo 90", "lane pulse1 vol 15").to_string(), "lane pulse1: doesn't take `vol`; it takes `duty`");
        assert_eq!(refusal("song \"s\" tempo 90", "lane triangle duty 50").to_string(), "lane triangle: doesn't take `duty`; it takes no parameters");
        assert_eq!(render("song \"s\" tempo 90\npattern p bars 1 grid 256\n").unwrap_err(), RenderError::Grid(256));
    }

    #[test]
    fn hand_built_patterns_that_skip_the_checker_are_refused() {
        let mut song = song("song \"s\" tempo 90\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x2\n  3: D4 x2\n");
        song.patterns[0].lanes[0].notes[1].at = Position { bar: 1, slot: 2 };
        assert_eq!(pattern(&song, &song.patterns[0]).unwrap_err(), RenderError::Misfit { lane: "pulse1".into(), at: Position { bar: 1, slot: 2 } });
        song.patterns[0].lanes[0].notes[1].at = Position { bar: 1, slot: 4 };
        assert!(matches!(pattern(&song, &song.patterns[0]), Err(RenderError::Misfit { .. })));
    }

    #[test]
    fn ffmml_refusals_surface_as_errors() {
        let unparsable = perform("A o9 c4").unwrap_err();
        assert!(matches!(&unparsable, RenderError::Mml(why) if !why.is_empty()), "{unparsable:?}");
        let unplayable = perform("A o4 c4&d4").unwrap_err();
        assert!(matches!(&unplayable, RenderError::Mml(why) if why.contains("'&' cannot combine different notes")), "{unplayable:?}");
        assert!(matches!(perform("A o4 c4^4^4"), Err(RenderError::Mml(_))));
        assert_eq!(perform("A o4 c4&c4&c4").unwrap().len(), samples_in(3, 120, 4));
    }
}
