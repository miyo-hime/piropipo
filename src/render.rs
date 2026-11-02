use crate::profile::{self, Breach, Flaw, Part, Setup, Voice, named};
use crate::score::{Bend, Cue, Note, Ornaments, Pattern, Position, Song};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

pub const SAMPLE_RATE: u16 = 44100;

const FRAME: u64 = SAMPLE_RATE as u64 / 60;

const STENCILS: usize = 128;

const SPELLING: [&str; 12] = ["c", "c+", "d", "d+", "e", "f", "f+", "g", "g+", "a", "a+", "b"];

impl Voice {
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

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    Profile(String),
    Tempo(u32),
    Grid(u32),
    Lane(String),
    Param { lane: String, key: String, takes: &'static [&'static str] },
    Volume { lane: String, key: String },
    Duty { lane: String, value: String },
    Loudness { lane: String, value: String },
    Range { lane: String, at: Position, pitch: u8, playable: &'static str },
    Shifted { lane: String, at: Position, pitch: u8, shift: i8, cue: usize, playable: &'static str },
    Ornament { lane: String, at: Position, flaw: Flaw, played: Option<(i8, usize)> },
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
            RenderError::Loudness { lane, value } => write!(f, "lane {lane}: {}", Breach::Loudness(value.clone())),
            RenderError::Range { lane, at, pitch, playable } => write!(f, "lane {lane}, at {at}: {}", Breach::Range { at: *at, pitch: *pitch, playable }),
            RenderError::Shifted { lane, at, pitch, shift, cue, playable } => write!(f, "lane {lane}, at {at}: {} transposed {shift:+} (play line {cue}) lands on {}, out of range; this lane plays {playable}", named(*pitch), named(i16::from(*pitch) + i16::from(*shift))),
            RenderError::Ornament { lane, at, flaw, played: None } => write!(f, "lane {lane}, at {at}: {flaw}"),
            RenderError::Ornament { lane, at, flaw, played: Some((shift, cue)) } => write!(f, "lane {lane}, at {at}: played {shift:+} (play line {cue}), {flaw}"),
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
        Breach::Loudness(value) => RenderError::Loudness { lane, value },
        Breach::Range { at, pitch, playable } => RenderError::Range { lane, at, pitch, playable },
        Breach::Ornament { at, flaw } => RenderError::Ornament { lane, at, flaw, played: None },
    }
}

#[derive(Debug, Default)]
pub struct Stencils { vibratos: Vec<[u8; 3]>, fades: Vec<Vec<u8>> }

type Plate = BTreeMap<(&'static str, usize), String>;

fn file<T: PartialEq>(book: &mut Vec<T>, stencil: T) -> Option<usize> {
    if let Some(n) = book.iter().position(|s| *s == stencil) { return Some(n); }
    if book.len() == STENCILS { return None; }
    book.push(stencil);
    Some(book.len() - 1)
}

pub fn pattern(song: &Song, pattern: &Pattern) -> Result<Vec<i16>, RenderError> { perform(&transcribe(song, pattern)?) }

pub fn pattern_with(song: &Song, pattern: &Pattern, stencils: &mut Stencils) -> Result<Vec<i16>, RenderError> { perform(&engrave(song, pattern, stencils)?) }

pub fn vet(song: &Song, pattern: &Pattern) -> Result<(), RenderError> {
    let card = profile::card(song.profile.as_deref()).map_err(|breach| charged("", breach))?;
    if !(1..=255).contains(&song.tempo) || !(1..=255).contains(&pattern.grid) { return Ok(()); }
    for lane in &pattern.lanes {
        let Ok(part) = card.part(&lane.name) else { continue };
        for note in lane.notes.iter().filter(|n| part.plays(n).is_ok()) {
            let Some(pitch) = note.pitch else { continue };
            timing(song.tempo, pattern.grid, note, pitch).map_err(|flaw| RenderError::Ornament { lane: lane.name.clone(), at: note.at, flaw, played: None })?;
        }
    }
    Ok(())
}

pub fn transposed<'p>(song: &Song, pattern: &'p Pattern, cue: &Cue) -> Result<Cow<'p, Pattern>, RenderError> {
    if cue.shift == 0 { return Ok(Cow::Borrowed(pattern)); }
    let card = profile::card(song.profile.as_deref()).map_err(|breach| charged("", breach))?;
    let mut moved = pattern.clone();
    for lane in &mut moved.lanes {
        let Ok(part) = card.part(&lane.name) else { continue };
        if part.voice == Voice::Noise { continue; }
        for note in &mut lane.notes {
            let Some(pitch) = note.pitch else { continue };
            let at = note.at;
            let landed = u8::try_from(i16::from(pitch) + i16::from(cue.shift)).ok().filter(|p| (part.low..=part.high).contains(p));
            note.pitch = Some(landed.ok_or_else(|| RenderError::Shifted { lane: lane.name.clone(), at, pitch, shift: cue.shift, cue: cue.line, playable: part.playable })?);
            let snag = |flaw| RenderError::Ornament { lane: lane.name.clone(), at, flaw, played: Some((cue.shift, cue.line)) };
            if let Some(Bend::Slide(from)) = &mut note.ornaments.bend {
                let departs = i16::from(*from) + i16::from(cue.shift);
                *from = u8::try_from(departs).map_err(|_| snag(Flaw::Reach { offset: None, pitch, landed: departs, playable: part.playable }))?;
            }
            if let Err(Breach::Ornament { flaw, .. }) = part.plays(note) { return Err(snag(flaw)); }
        }
    }
    Ok(Cow::Owned(moved))
}

pub fn arp_rate(tempo: u32, grid: u32, len: u32) -> (u64, f64) {
    let count = count(tempo, grid, len);
    (count, rate(tempo, grid, len, count))
}

fn transcribe(song: &Song, pattern: &Pattern) -> Result<String, RenderError> { engrave(song, pattern, &mut Stencils::default()) }

fn engrave(song: &Song, pattern: &Pattern, stencils: &mut Stencils) -> Result<String, RenderError> {
    let card = profile::card(song.profile.as_deref()).map_err(|breach| charged("", breach))?;
    if !(1..=255).contains(&song.tempo) { return Err(RenderError::Tempo(song.tempo)); }
    if !(1..=255).contains(&pattern.grid) { return Err(RenderError::Grid(pattern.grid)); }
    if let Some(stray) = pattern.lanes.iter().find_map(|l| card.part(&l.name).err()) { return Err(charged("", stray)); }
    let mut plate = Plate::new();
    let lines = card.parts.iter().map(|part| line(song.tempo, pattern, part, stencils, &mut plate)).collect::<Result<Vec<_>, _>>()?;
    Ok(plate.into_values().chain(lines).collect::<Vec<_>>().join("\n"))
}

fn line(tempo: u32, pattern: &Pattern, part: &Part, stencils: &mut Stencils, plate: &mut Plate) -> Result<String, RenderError> {
    let Part { name, channel, voice, .. } = *part;
    let Pattern { bars, grid, .. } = *pattern;
    let span = u64::from(bars) * u64::from(grid);
    let lane = pattern.lanes.iter().find(|l| l.name == name);
    let Setup { duty, volume } = part.setup(lane.map_or(&[][..], |l| l.params.as_slice())).map_err(|breach| charged(name, breach))?;
    // a triangle that rests before its first note idles at full positive output in ffmml (a DC shelf), so every voice starts at v0 and finds its volume on its first note
    let mut words = vec![channel.to_string(), format!("t{tempo}"), "v0".to_owned()];
    if let Some(timbre) = duty { words.push(format!("@{timbre}")); }
    let mut notes: Vec<(&Note, u8)> = lane.iter().flat_map(|l| &l.notes).filter_map(|n| Some((n, n.pitch?))).collect();
    notes.sort_by_key(|(n, _)| n.at.tick(grid));
    let mut cursor = 0;
    for (i, (note, pitch)) in notes.into_iter().enumerate() {
        let Position { bar, slot } = note.at;
        let start = note.at.tick(grid);
        let end = start + u64::from(note.len);
        if bar == 0 || slot == 0 || slot > grid || note.len == 0 || start < cursor || end > span { return Err(RenderError::Misfit { lane: name.into(), at: note.at }); }
        part.plays(note).map_err(|breach| charged(name, breach))?;
        let snag = |flaw| RenderError::Ornament { lane: name.into(), at: note.at, flaw, played: None };
        let spell = |pitch: u8| voice.spell(pitch).ok_or_else(|| charged(name, Breach::Range { at: note.at, pitch, playable: part.playable }));
        let (octave, letter) = spell(pitch)?;
        timing(tempo, grid, note, pitch).map_err(snag)?;
        words.extend(lengths(grid, start - cursor).map(|d| format!("r{d}")));
        let Ornaments { bend, duty: tint, vol, fade } = &note.ornaments;
        let leveled = vol.is_some() || *fade;
        if i == 0 && !leveled { words.push(format!("v{volume}")); }
        if let Some(tint) = tint { words.push(format!("@{tint}")); }
        if *fade {
            let n = file(&mut stencils.fades, fall(vol.unwrap_or(volume), frames(tempo, grid, start, end))).ok_or_else(|| snag(Flaw::Crowded("fade")))?;
            plate.entry(("v", n)).or_insert_with(|| format!("@v{n} = {{{}}}", stencils.fades.get(n).map_or(String::new(), |f| f.iter().map(u8::to_string).collect::<Vec<_>>().join(", "))));
            words.push(format!("@v{n}"));
        } else if let Some(vol) = vol {
            words.push(format!("v{vol}"));
        }
        match bend {
            Some(Bend::Vibrato(vib)) => {
                let depth = profile::ticks(pitch, vib.cents) as u8;
                let shape = [vib.delay, vib.speed, depth];
                let n = file(&mut stencils.vibratos, shape).ok_or_else(|| snag(Flaw::Crowded("vibrato")))?;
                plate.entry(("MP", n)).or_insert_with(|| format!("@MP{n} = {{{} {} {}}}", shape[0], shape[1], shape[2]));
                words.extend([format!("MP{n}"), format!("o{octave}"), tie(&letter, grid, end - start), "MPOF".to_owned()]);
            }
            Some(Bend::Arpeggio(offsets)) => {
                for (units, offset) in steps(tempo, grid, note.len).into_iter().zip(offsets.iter().cycle()) {
                    let (octave, letter) = spell(u8::try_from(i16::from(pitch) + offset).unwrap_or(u8::MAX))?;
                    words.extend([format!("o{octave}"), tie(&letter, lattice(grid), units)]);
                }
            }
            Some(Bend::Slide(from)) => {
                let semitones = u16::from(pitch.abs_diff(*from));
                let steps = steps(tempo, grid, note.len);
                let (walk, hold) = steps.split_at(usize::from(semitones));
                let rungs = if *from < pitch { (*from..pitch).collect::<Vec<_>>() } else { (pitch + 1..=*from).rev().collect() };
                for (&units, rung) in walk.iter().zip(rungs) {
                    let (octave, letter) = spell(rung)?;
                    words.extend([format!("o{octave}"), tie(&letter, lattice(grid), units)]);
                }
                words.extend([format!("o{octave}"), tie(&letter, lattice(grid), hold.iter().sum())]);
            }
            None => words.extend([format!("o{octave}"), tie(&letter, grid, end - start)]),
        }
        if let (Some(_), Some(home)) = (tint, duty) { words.push(format!("@{home}")); }
        if leveled { words.push(format!("v{volume}")); }
        cursor = end;
    }
    words.extend(lengths(grid, span - cursor).map(|d| format!("r{d}")));
    Ok(words.join(" "))
}

// `&` and never `^`: ffmml 0.1.2 only looks one command back for the note a `^` extends, so `c4^4^4` errors on the second tie
fn tie(letter: &str, grid: u32, slots: u64) -> String { lengths(grid, slots).map(|d| format!("{letter}{d}")).collect::<Vec<_>>().join("&") }

fn seconds(tempo: u32, grid: u32, slots: u32) -> f64 { f64::from(slots) * 240.0 / (f64::from(tempo) * f64::from(grid)) }

fn sample_at(tempo: u32, grid: u32, tick: u64) -> u128 { (u128::from(tick) * 240 * u128::from(SAMPLE_RATE)).div_ceil(u128::from(tempo) * u128::from(grid)) }

fn frames(tempo: u32, grid: u32, start: u64, end: u64) -> u64 { u64::try_from((sample_at(tempo, grid, end) - sample_at(tempo, grid, start)).div_ceil(u128::from(FRAME))).unwrap_or(u64::MAX) }

fn frames_in(tempo: u32, grid: u32, start: u64, len: u32) -> u64 { frames(tempo, grid, start, start + u64::from(len)) }

fn fall(level: u8, frames: u64) -> Vec<u8> {
    let last = frames - 1;
    (0..frames).map(|k| u8::try_from((2 * u64::from(level) * (last - k) + last) / (2 * last)).unwrap_or(level)).collect()
}

// ※ steps live on the finest multiple of the grid at or under 255; mixed-denominator steps could get slow tempos nearer c255 if a song ever needs it
fn lattice(grid: u32) -> u32 { 255 / grid * grid }

fn count(tempo: u32, grid: u32, len: u32) -> u64 {
    let pace = u64::from(tempo) * u64::from(grid);
    ((28_800 * u64::from(len) + pace - 1) / (2 * pace)).min(u64::from(len) * u64::from(lattice(grid) / grid))
}

fn steps(tempo: u32, grid: u32, len: u32) -> Vec<u64> {
    let units = u64::from(len) * u64::from(lattice(grid) / grid);
    let count = count(tempo, grid, len);
    (0..count).map(|i| (i + 1) * units / count - i * units / count).collect()
}

fn rate(tempo: u32, grid: u32, len: u32, count: u64) -> f64 { count as f64 / seconds(tempo, grid, len) }

fn fits(len: u32, admits: impl Fn(u32) -> bool) -> Option<u32> { (len.saturating_add(1)..=len.saturating_add(4096)).find(|l| admits(*l)) }

fn timing(tempo: u32, grid: u32, note: &Note, pitch: u8) -> Result<(), Flaw> {
    let start = note.at.tick(grid);
    if note.ornaments.fade {
        let frames = frames_in(tempo, grid, start, note.len);
        if frames < 2 { return Err(Flaw::Blink { frames, fits: fits(note.len, |len| frames_in(tempo, grid, start, len) >= 2) }); }
    }
    match &note.ornaments.bend {
        Some(Bend::Vibrato(vib)) if u64::from(vib.delay) * u64::from(tempo) * u64::from(grid) >= 14_400 * u64::from(note.len) => Err(Flaw::Late { frames: vib.delay, note_ms: seconds(tempo, grid, note.len) * 1000.0 }),
        Some(Bend::Arpeggio(offsets)) => arpeggiate(tempo, grid, note.len, offsets.len()),
        Some(Bend::Slide(from)) => walk(tempo, grid, note.len, u16::from(pitch.abs_diff(*from))),
        _ => Ok(()),
    }
}

fn arpeggiate(tempo: u32, grid: u32, len: u32, offsets: usize) -> Result<(), Flaw> {
    let offsets = offsets as u64;
    let paced = |len: u32| { let count = count(tempo, grid, len); count >= offsets && (45.0..=75.0).contains(&rate(tempo, grid, len, count)) };
    let steps = count(tempo, grid, len);
    let rate = rate(tempo, grid, len, steps);
    let fastest = f64::from(tempo) * f64::from(lattice(grid)) / 240.0;
    if fastest < 45.0 { return Err(Flaw::Pace { rate, fastest, fits: None }); }
    if steps < offsets { return Err(Flaw::Crammed { offsets, steps, fits: fits(len, paced) }); }
    if !(45.0..=75.0).contains(&rate) { return Err(Flaw::Pace { rate, fastest, fits: fits(len, paced) }); }
    Ok(())
}

fn walk(tempo: u32, grid: u32, len: u32, semitones: u16) -> Result<(), Flaw> {
    let dwells = count(tempo, grid, len);
    if dwells > u64::from(semitones) { return Ok(()); }
    Err(Flaw::Cramped { semitones, dwells, fits: fits(len, |len| count(tempo, grid, len) > u64::from(semitones)) })
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
        let doc = include_str!("../skills/piropipo/references/format.md");
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

    fn cue(shift: i8) -> Cue { Cue { pattern: "p".into(), shift, line: 12 } }

    #[test]
    fn transpose_moves_the_pitched_lanes_and_leaves_the_noise_alone() {
        let trio = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x2\n  3: - x2\nlane triangle\n  1: E2 x4\nlane noise\n  1: C4 x1\n");
        let pitches = |shift| transposed(&trio, &trio.patterns[0], &cue(shift)).unwrap().lanes.iter().map(|l| l.notes.iter().map(|n| n.pitch).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(pitches(3), [vec![Some(63), None], vec![Some(43)], vec![Some(60)]]);
        assert_eq!(pitches(-12), [vec![Some(48), None], vec![Some(28)], vec![Some(60)]]);
        assert!(matches!(transposed(&trio, &trio.patterns[0], &cue(0)), Ok(Cow::Borrowed(_))));
        let noise = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane noise\n  1: B9 x1\n");
        assert_eq!(transposed(&noise, &noise.patterns[0], &cue(127)).unwrap().lanes[0].notes[0].pitch, Some(131));
    }

    #[test]
    fn a_transpose_off_the_chip_is_refused_with_both_pitches_named() {
        let song = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C4 x1\n  2: A#7 x1\n");
        let refused = transposed(&song, &song.patterns[0], &cue(3)).unwrap_err();
        assert_eq!(refused, RenderError::Shifted { lane: "pulse1".into(), at: Position { bar: 1, slot: 2 }, pitch: 106, shift: 3, cue: 12, playable: "C2 through B7" });
        assert_eq!(refused.to_string(), "lane pulse1, at 1.2: A#7 transposed +3 (play line 12) lands on C#8, out of range; this lane plays C2 through B7");
        assert_eq!(transposed(&song, &song.patterns[0], &cue(-127)).unwrap_err().to_string(), "lane pulse1, at 1.1: C4 transposed -127 (play line 12) lands on F-7, out of range; this lane plays C2 through B7");
        assert!(transposed(&song, &song.patterns[0], &cue(1)).is_ok(), "A#7 up one is B7, the top of the pulse");
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
        assert_eq!(refusal("song \"s\" tempo 90", "lane pulse1 volume 15").to_string(), "lane pulse1: doesn't take `volume`; it takes `duty` and `vol`");
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


    fn flaw(text: &str) -> Flaw {
        match render(text) { Err(RenderError::Ornament { flaw, played: None, .. }) => flaw, other => panic!("expected an ornament refusal, got {other:?}") }
    }

    fn lone(lane: &str, note: &str) -> String { format!("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane {lane}\n  1: {note}\n") }

    #[test]
    fn effects_compile_native_and_put_the_lane_back_after_every_note() {
        let s = song("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1 duty 25 vol 9\n  1: E5 x4 vib 20ct 6hz 125ms\n  5: G5 x2 duty 12.5\n  7: C5 x4 from A4\n  11: B4 x2 vol 13 fade\n  13: A4 x4 arp 0,4,7 vol 7\n");
        let mml = transcribe(&s, &s.patterns[0]).unwrap();
        let lines: Vec<&str> = mml.lines().collect();
        assert_eq!(lines[..2], ["@MP0 = {8 3 2}", "@v0 = {13, 12, 11, 10, 9, 8, 7, 7, 6, 5, 4, 3, 2, 1, 0}"]);
        let arp = "o4 a120 o5 c+120 o5 e120 ".repeat(10);
        assert_eq!(lines[2], format!("A t120 v0 @1 v9 MP0 o5 e4 MPOF @0 o5 g8 @1 o4 a120 o4 a+120 o4 b120 o5 c5&c40 @v0 o4 b8 v9 v7 {arp}v9"));
        assert_eq!(lines[3..], ["B t120 v0 @2 r1", "C t120 v0 r1", "D t120 v0 r1"]);
    }

    #[test]
    fn step_schedules_sum_exactly_to_the_written_length() {
        for tempo in [1, 30, 61, 97, 120, 144, 200, 255] {
            for grid in [1, 3, 4, 7, 8, 12, 16, 24, 48, 64, 96, 128, 200, 255] {
                for len in 1..=48 {
                    let steps = steps(tempo, grid, len);
                    let units = u64::from(len) * u64::from(lattice(grid) / grid);
                    assert_eq!(steps.len() as u64, count(tempo, grid, len));
                    assert!(steps.iter().all(|s| *s >= 1), "tempo {tempo} grid {grid} x{len}: a step under the shortest note: {steps:?}");
                    if !steps.is_empty() { assert_eq!(steps.iter().sum::<u64>(), units, "tempo {tempo} grid {grid} x{len}: {steps:?}"); }
                    assert!(steps.iter().max().zip(steps.iter().min()).is_none_or(|(hi, lo)| hi - lo <= 1), "widening spreads one unit at most: {steps:?}");
                    let written = steps.iter().map(|s| lengths(lattice(grid), *s).map(|d| 1.0 / f64::from(d)).sum::<f64>()).sum::<f64>();
                    if !steps.is_empty() { assert!((written - f64::from(len) / f64::from(grid)).abs() < 1e-9); }
                }
            }
        }
    }

    #[test]
    fn arps_and_slides_ring_exactly_as_long_as_their_note() {
        let plain = render(&lone("pulse1", "A4 x7 ; plain")).unwrap();
        for effect in ["arp 0,4,7", "arp 4,-5", "from C4", "from C6"] {
            let bent = render(&lone("pulse1", &format!("A4 x7 {effect}"))).unwrap();
            assert_eq!(bent.len(), plain.len(), "{effect}");
            let sounding = |s: &[i16]| s.iter().rposition(|x| *x != 0);
            assert_eq!(sounding(&bent).map(|i| i / 1000), sounding(&plain).map(|i| i / 1000), "{effect} should stop where the note does");
        }
    }

    #[test]
    fn a_slide_walks_semitone_by_semitone_into_its_target() {
        let s = song("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane triangle\n  1: A2 x4 from C3\n");
        assert_eq!(transcribe(&s, &s.patterns[0]).unwrap().lines().nth(2), Some("C t120 v0 v15 o4 c120 o3 b120 o3 a+120 o3 a5&a40 r2 r4"));
    }

    #[test]
    fn the_vibrato_is_linted_where_it_really_swings_after_rounding() {
        let crossing = render(&lone("pulse1", "A#7 x4 vib 90ct 6hz")).unwrap_err();
        let RenderError::Ornament { flaw: Flaw::Swing { ticks, up, down, edge, .. }, .. } = crossing.clone() else { panic!("{crossing:?}") };
        assert_eq!((ticks, edge), (2.0, 107));
        assert!((119.0..120.0).contains(&up) && (111.0..112.5).contains(&down), "+{up}/-{down}");
        assert_eq!(crossing.to_string(), format!("lane pulse1, at 1.1: the vibrato on A#7 rounds to 2 register ticks and really swings +{up:.1}/-{down:.1} cents, past B7; this lane plays C2 through B7"));
        assert!(render(&lone("pulse1", "A#7 x4 vib 50ct 6hz")).is_ok(), "one tick at A#7 climbs about 59 cents and stays under B7");
        assert!(matches!(flaw(&lone("pulse1", "C2 x4 vib 20ct 6hz")), Flaw::Swing { edge: 36, .. }), "the trough counts too");
    }

    #[test]
    fn the_vibrato_keeps_its_register_strictly_positive() {
        let headroom = flaw(&lone("pulse1", "C6 x4 vib 1800ct 6hz"));
        assert!(matches!(headroom, Flaw::Headroom { pitch: 84, ticks: 111.0, .. }), "{headroom:?}");
        assert_eq!(headroom.to_string(), "a 111-tick vibrato on C6 would drive the chip's period register (sitting at 106.9) through zero, where the pitch runs off to infinity; keep it at 106 ticks or fewer");
    }

    #[test]
    fn a_vibrato_too_shallow_or_too_deep_for_the_register_is_refused() {
        let shallow = flaw(&lone("pulse1", "B6 x4 vib 10ct 6hz"));
        assert!(matches!(shallow, Flaw::Shallow { pitch: 95, .. }), "{shallow:?}");
        assert!(shallow.to_string().starts_with("`vib 10ct` on B6 rounds to 0 register ticks and would never move; ask for at least "), "{shallow}");
        assert!(matches!(flaw(&lone("triangle", "C1 x4 vib 150ct 6hz")), Flaw::Deep { pitch: 24, ticks: 296.0 }));
        assert!(render(&lone("triangle", "C#1 x4 vib 60ct 6hz")).is_ok());
    }

    #[test]
    fn a_vibrato_that_would_never_start_is_refused() {
        let late = flaw(&lone("pulse1", "A4 x2 vib 20ct 6hz 250ms"));
        assert_eq!(late, Flaw::Late { frames: 15, note_ms: 250.0 });
        assert!(matches!(flaw(&lone("pulse1", "A4 x1 vib 20ct 6hz 125ms")), Flaw::Late { frames: 8, .. }), "125 ms rounds to 8 frames, past the 7.5 the note lasts");
        assert!(render(&lone("pulse1", "A4 x1 vib 20ct 6hz 100ms")).is_ok());
    }

    #[test]
    fn the_arp_rate_is_judged_and_named() {
        let slow = flaw("song \"s\" tempo 30\npattern p bars 1 grid 16\nlane pulse1\n  1: A4 x16 arp 0,4,7\n");
        assert!(matches!(slow, Flaw::Pace { fastest: 30.0, fits: None, .. }), "{slow:?}");
        assert!(slow.to_string().ends_with("at this tempo and grid the fastest step is 30.0 a second, so it writes no arps"), "{slow}");
        let fast = flaw("song \"s\" tempo 145\npattern p bars 1 grid 64\nlane pulse1\n  1: A4 x1 arp 0,7\n");
        assert!(matches!(fast, Flaw::Pace { fits: Some(2), .. }), "{fast:?}");
        assert!(fast.to_string().starts_with("the arp steps 77.3 times a second, outside 45 to 75"), "{fast}");
        let crammed = flaw("song \"s\" tempo 120\npattern p bars 1 grid 64\nlane pulse1\n  1: A4 x1 arp 0,4,7\n");
        assert_eq!(crammed, Flaw::Crammed { offsets: 3, steps: 2, fits: Some(2) });
        assert_eq!(arp_rate(120, 16, 4), (30, 60.0));
    }

    #[test]
    fn a_slide_needs_room_for_every_dwell_and_the_target() {
        let cramped = flaw("song \"s\" tempo 255\npattern p bars 1 grid 255\nlane pulse1\n  1: C5 x1 from B4\n");
        assert_eq!(cramped, Flaw::Cramped { semitones: 1, dwells: 0, fits: Some(7) });
        assert_eq!(cramped.to_string(), "a 1-semitone slide needs 2 dwells at about 60 a second, the target's hold included, and this note only fits 0; x7 would fit");
        assert!(render("song \"s\" tempo 255\npattern p bars 1 grid 255\nlane pulse1\n  1: C5 x7 from B4\n").is_ok());
    }

    #[test]
    fn a_fade_needs_two_frames_to_reach_zero() {
        let blink = flaw("song \"s\" tempo 255\npattern p bars 1 grid 255\nlane pulse1\n  1: C5 x1 fade\n");
        assert_eq!(blink, Flaw::Blink { frames: 1, fits: Some(5) });
        assert!(render("song \"s\" tempo 255\npattern p bars 1 grid 255\nlane pulse1\n  1: C5 x5 fade\n").is_ok());
    }

    #[test]
    fn a_fade_lands_on_silence_in_its_last_frame() {
        let samples = render("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: A4 x4 vol 15 fade\n").unwrap();
        let frame = usize::try_from(FRAME).unwrap();
        assert_eq!(samples.len(), 120 * frame);
        let loudest = |from: usize| samples[from * frame..(from + 1) * frame].iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert_eq!(loudest(0), 8192);
        assert!(loudest(60) < loudest(0) && loudest(60) > 0);
        assert_eq!(loudest(119), 0);
    }

    #[test]
    fn the_noise_lane_refuses_pitch_effects_and_the_triangle_refuses_volume() {
        for (effect, word) in [("vib 20ct 6hz", "vib"), ("arp 0,4", "arp"), ("from C3", "from")] { assert_eq!(flaw(&lone("noise", &format!("C4 x4 {effect}"))), Flaw::Hiss(word)); }
        assert_eq!(flaw(&lone("noise", "C4 x4 vib 20ct 6hz")).to_string(), "`vib` doesn't play on the noise lane; noise pitch is a timer setting, and wobbling it makes a different instrument, not vibrato");
        assert!(render(&lone("noise vol 5", "C4 x4 vol 9 fade")).is_ok());
        assert_eq!(flaw(&lone("triangle", "C3 x4 vol 9")), Flaw::Mute("vol"));
        assert_eq!(flaw(&lone("triangle", "C3 x4 fade")).to_string(), "doesn't take `fade`; the triangle has no volume, it's either on or off");
        assert_eq!(flaw(&lone("triangle", "C3 x4 duty 25")).to_string(), "doesn't take `duty` on its notes; this lane's notes take `vib`, `arp`, and `from`");
        assert_eq!(flaw(&lone("noise", "C3 x4 duty 25")), Flaw::Unfit { word: "duty", takes: &["vol", "fade"] });
    }

    #[test]
    fn arp_offsets_and_slide_departures_answer_to_the_range() {
        assert_eq!(flaw(&lone("pulse1", "A7 x4 arp 0,4")).to_string(), "the arp's +4 lands on C#8 from A7, out of range; this lane plays C2 through B7");
        assert_eq!(flaw(&lone("triangle", "C1 x4 from B0")).to_string(), "the slide departs from B0, out of range; this lane plays C1 through B6");
    }

    #[test]
    fn macros_are_shared_by_their_whole_content() {
        let s = song("song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n  1: C4 x2 vib 20ct 6hz\n  3: C4 x2 vib 21ct 6hz\n  5: C4 x2 vib 20ct 5hz\n  7: C4 x2 vib 20ct 6hz 50ms\n  9: C4 x2 fade\n  13: C4 x2 fade\nlane pulse2\n  1: C4 x2 vib 20ct 6hz\n  3: C4 x2 vol 9 fade\n");
        let mml = transcribe(&s, &s.patterns[0]).unwrap();
        let macros: Vec<&str> = mml.lines().filter(|l| l.starts_with('@')).map(|l| l.split(" = ").next().unwrap()).collect();
        assert_eq!(macros, ["@MP0", "@MP1", "@MP2", "@v0", "@v1"]);
        assert!(mml.contains("@MP1 = {0 4 5}") && mml.contains("@MP2 = {3 3 5}"), "{mml}");
        let pulse1 = mml.lines().find(|l| l.starts_with("A ")).unwrap();
        assert_eq!(pulse1.matches("MP0 ").count(), 2, "every vibrato note re-arms its lfo: {pulse1}");
        assert_eq!(pulse1.matches("@v0 ").count(), 2);
    }

    #[test]
    fn a_song_holds_128_macros_of_a_kind_and_names_the_129th() {
        let crowd = |n: u32| song(&format!("song \"s\" tempo 255\npattern p bars {n} grid 1\nlane pulse1\n{}", (1..=n).map(|b| format!("  {b}.1: C4 x1 vib 5ct {:.6}hz\n", 20.0 / f64::from(b))).collect::<String>()));
        let fits = crowd(128);
        assert!(transcribe(&fits, &fits.patterns[0]).is_ok());
        let over = crowd(129);
        let refused = transcribe(&over, &over.patterns[0]).unwrap_err();
        assert_eq!(refused, RenderError::Ornament { lane: "pulse1".into(), at: Position { bar: 129, slot: 1 }, flaw: Flaw::Crowded("vibrato"), played: None });
        assert_eq!(refused.to_string(), "lane pulse1, at 129.1: this note needs a 129th distinct vibrato macro, past the 128 a song can hold; reuse the settings of an earlier vibrato");
    }

    #[test]
    fn transposed_vibratos_are_reconverted_and_relinted_per_play() {
        let s = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: G#7 x4 vib 90ct 6hz\n");
        assert!(pattern(&s, &s.patterns[0]).is_ok());
        assert!(transposed(&s, &s.patterns[0], &cue(1)).is_ok());
        let refused = transposed(&s, &s.patterns[0], &cue(2)).unwrap_err();
        assert!(matches!(&refused, RenderError::Ornament { flaw: Flaw::Swing { pitch: 106, ticks: 2.0, .. }, played: Some((2, 12)), .. }), "{refused:?}");
        assert!(refused.to_string().starts_with("lane pulse1, at 1.1: played +2 (play line 12), the vibrato on A#7 rounds to 2 register ticks"), "{refused}");
        let low = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane triangle\n  1: C3 x4 vib 30ct 6hz\n");
        let mml = |shift| { let moved = transposed(&low, &low.patterns[0], &cue(shift)).unwrap(); transcribe(&low, &moved).unwrap().lines().next().unwrap().to_owned() };
        assert_ne!(mml(0), mml(12), "an octave up halves the register, so the same cents need fewer ticks");
    }

    #[test]
    fn a_slide_departure_moves_with_its_pattern() {
        let s = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C5 x4 from A4\n");
        let moved = transposed(&s, &s.patterns[0], &cue(3)).unwrap();
        assert_eq!(moved.lanes[0].notes[0].ornaments.bend, Some(Bend::Slide(72)));
        let low = song("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: E2 x4 from C2\n");
        assert_eq!(transposed(&low, &low.patterns[0], &cue(-1)).unwrap_err().to_string(), "lane pulse1, at 1.1: played -1 (play line 12), the slide departs from B1, out of range; this lane plays C2 through B7");
    }

    fn runs(samples: &[i16]) -> Vec<usize> {
        let mut runs = vec![1];
        for w in samples.windows(2) { if (w[0] > 0) == (w[1] > 0) { *runs.last_mut().unwrap() += 1; } else { runs.push(1); } }
        runs[1..runs.len() - 1].to_vec()
    }

    #[test]
    fn a_plain_note_after_an_effect_renders_as_if_the_effect_never_was() {
        let pair = |first: &str| render(&format!("song \"s\" tempo 120\npattern p bars 1 grid 8\nlane pulse1\n  1: A4 x4 {first}\n  5: A4 x4\n")).unwrap();
        let clean = pair("");
        let half = clean.len() / 2;
        for level in ["vol 3", "duty 75", "fade", "vol 12 fade duty 12.5"] { assert_eq!(pair(level)[half..], clean[half..], "{level} leaked into the next note"); }
        assert_eq!(runs(&clean[half..]).into_iter().collect::<std::collections::BTreeSet<_>>(), [50, 51].into());
        assert!(runs(&pair("vib 60ct 6hz")[..half]).iter().any(|r| !(50..=51).contains(r)), "the run-length probe hears a vibrato");
        for bend in ["vib 60ct 6hz", "arp 0,7", "from C4", "vib 60ct 20hz duty 25 vol 4 fade"] {
            let heard = pair(bend);
            assert!(runs(&heard[half..]).iter().all(|r| (50..=51).contains(r)), "{bend} leaked pitch into the next note");
            assert_eq!(heard[half..].iter().map(|s| s.unsigned_abs()).max(), clean[half..].iter().map(|s| s.unsigned_abs()).max(), "{bend}");
        }
    }

    #[test]
    fn a_lane_volume_is_the_default_and_a_note_volume_overrides_it_for_that_note() {
        let lane = render("song \"s\" tempo 120\npattern p bars 1 grid 8\nlane pulse1 vol 6\n  1: A4 x2\n  3: A4 x2 vol 12\n  5: A4 x2\n").unwrap();
        let slot = samples_in(1, 120, 8);
        let loudest = |from: usize, to: usize| lane[from * slot..to * slot].iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert_eq!((loudest(0, 2), loudest(2, 4), loudest(4, 6), loudest(6, 8)), (3276, 6553, 3276, 0));
        let spelled = render("song \"s\" tempo 120\npattern p bars 1 grid 8\nlane pulse1\n  1: A4 x2 vol 6\n  3: A4 x2 vol 12\n  5: A4 x2 vol 6\n").unwrap();
        assert_eq!(lane, spelled);
        let noise = render("song \"s\" tempo 120\npattern p bars 1 grid 8\nlane noise vol 4\n  1: C4 x8\n").unwrap();
        assert_eq!(noise.iter().max(), Some(&2184));
    }
}
