use crate::surface::{emit, land_beside, publish, read_source};
use piropipo::render::{RenderError, transposed};
use piropipo::score::{Cue, Lane, Note, Pattern, Position, ScoreError, Song};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
    #[arg(short, long, help = "where the png lands (default: beside the input; a crop names itself, like song.bars-17-24.png)")]
    pub output: Option<PathBuf>,
    #[arg(long, value_name = "NAMES", help = "draw only these patterns, once each as written, like chorus,bridge")]
    pub patterns: Option<String>,
    #[arg(long, value_name = "SPAN", help = "draw only these bars of the song as it plays, like 17..24 (both ends included) or 17")]
    pub bars: Option<String>,
}

type Rgb = [u8; 3];

const PAPER: Rgb = [0x12, 0x14, 0x1a];
const STAFF: Rgb = [0x1c, 0x1f, 0x28];
const BARLINE: Rgb = [0x5c, 0x63, 0x75];
const BEATLINE: Rgb = [0x2e, 0x33, 0x3f];
const CHALK: Rgb = [0xec, 0xed, 0xf2];
const SMUDGE: Rgb = [0x8c, 0x93, 0xa6];

const VOICES: [(&str, Rgb); 4] = [("pulse1", [0xff, 0x6b, 0x6b]), ("pulse2", [0xff, 0xc4, 0x4d]), ("triangle", [0x4f, 0xd6, 0xc8]), ("noise", [0xb6, 0x8c, 0xff])];

const STRAYS: [Rgb; 4] = [[0x7e, 0xe0, 0x81], [0xff, 0x8f, 0xd8], [0x6f, 0xa8, 0xff], [0xdc, 0xdc, 0xdc]];

const SCIENTIFIC: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

const GLYPH_H: usize = 8;
const INK: usize = 2;
const TITLE: usize = 3;
const TEXT: usize = GLYPH_H * INK;
const LINE: usize = TEXT + 4;
const MARGIN: usize = 16;
const HEAD: usize = GLYPH_H * TITLE + 20;
const RULER: usize = 28;
const GAP: usize = 24;
const ROOM: usize = 10;
const NOTE: usize = 20;
const SEMITONE: usize = 8;
const LIMIT: usize = 16384;
const BILLED: usize = 4;

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> {
    match unroll(&args) {
        Ok(Rolled { target, song, size: [width, height], cut }) => {
            let shown = target.display().to_string();
            let mut stats = json!({ "png": shown, "width": width, "height": height });
            let (sentence, warnings) = match cut {
                None => {
                    let count = song.patterns.len();
                    (format!("rolled \"{}\" onto {shown}: {count} pattern{}, {width}x{height} pixels", song.title, if count == 1 { "" } else { "s" }), Vec::new())
                }
                Some(cut) => {
                    stats["crop"] = cut.json;
                    let asides: String = cut.warnings.iter().map(|w| format!("\nwarning: {}", w.human)).collect();
                    (format!("rolled \"{}\" onto {shown} (crop: {}): {}, {width}x{height} pixels{asides}", song.title, cut.scope, cut.names.join(", ")), cut.warnings)
                }
            };
            emit(human, json!({ "ok": true, "verb": "roll", "output": shown, "stats": stats, "warnings": warnings.iter().map(|w| &w.json).collect::<Vec<_>>(), "errors": [] }), &sentence);
            Ok(())
        }
        Err(blots) => {
            if !human { println!("{}", json!({ "ok": false, "verb": "roll", "output": null, "stats": null, "warnings": [], "errors": blots.iter().map(|b| &b.json).collect::<Vec<_>>() })); }
            Err(blots.into_iter().map(|b| b.human).collect::<Vec<_>>().join("\npiro: ").into())
        }
    }
}

struct Rolled { target: PathBuf, song: Song, size: [usize; 2], cut: Option<Cut> }

fn unroll(args: &Args) -> Result<Rolled, Vec<Blot>> {
    let crop = Crop::asked(args).map_err(|b| vec![b])?;
    let origin = (args.input != "-").then(|| Path::new(&args.input));
    let named = args.output.clone().or_else(|| Some(origin?.with_extension(format!("{}.png", crop.as_ref()?.tag()))));
    let target = land_beside(origin, named, "png").map_err(|(code, why)| vec![blot(code, why)])?;
    let (text, _) = read_source(&args.input).map_err(|e| vec![blot("unreadable-input", format!("can't read {}: {e}", origin.map_or("stdin".into(), |p| p.display().to_string())))])?;
    let song = Song::parse(&text).map_err(|errors| errors.iter().map(misprint).collect::<Vec<_>>())?;
    let (sheets, cut) = match &crop { None => (whole(&song), None), Some(crop) => snip(&song, crop).map(|(sheets, cut)| (sheets, Some(cut)))? };
    let canvas = roll(&song, &sheets, cut.as_ref().map(|c| c.scope.as_str())).map_err(|why| vec![blot("too-large", why)])?;
    let png = canvas.png().map_err(|e| vec![blot("unencodable", format!("the png encoder refused the roll: {e}"))])?;
    publish(&target, &png).map_err(|e| vec![blot("unwritable-output", format!("can't write {}: {e}", target.display()))])?;
    Ok(Rolled { target, song, size: [canvas.width, canvas.height], cut })
}

enum Crop { Patterns(Vec<String>), Bars(u64, u64) }

impl Crop {
    fn asked(args: &Args) -> Result<Option<Crop>, Blot> {
        match (args.patterns.as_deref(), args.bars.as_deref()) {
            (None, None) => Ok(None),
            (Some(_), Some(_)) => Err(blot("two-crops", "--patterns and --bars each choose the whole crop, so pick one: --patterns draws patterns once each as written, --bars draws a stretch of the song as it plays".into())),
            (Some(list), None) => {
                let names: Vec<String> = list.split(',').map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect();
                if names.is_empty() { return Err(blot("empty-crop", "--patterns names no pattern; list them by name, like --patterns chorus,bridge".into())); }
                Ok(Some(Crop::Patterns(names)))
            }
            (None, Some(span)) => spanned(span).map(|(first, last)| Some(Crop::Bars(first, last))),
        }
    }

    fn tag(&self) -> String {
        match self {
            Crop::Bars(first, last) if first == last => format!("bar-{first}"),
            Crop::Bars(first, last) => format!("bars-{first}-{last}"),
            Crop::Patterns(names) => names.join("+").replace(['/', '\\'], "_"),
        }
    }
}

fn spanned(text: &str) -> Result<(u64, u64), Blot> {
    let read = |s: &str| s.trim().parse::<u64>().ok();
    let span = match text.split_once("..").or_else(|| text.split_once('-')) {
        Some((first, last)) => read(first).zip(read(last)),
        None => read(text).map(|bar| (bar, bar)),
    };
    match span {
        None => Err(blot("bad-bars", format!("`{text}` isn't a span of bars; write the first and last bar of the song as it plays, like 17..24 (both ends included), or one bar like 17"))),
        Some((0, _)) => Err(blot("bad-bars", format!("bars count from 1, so `{text}` reaches for a bar before the song starts"))),
        Some((first, last)) if first > last => Err(blot("bad-bars", format!("`{text}` runs backwards; write the earlier bar first, like {last}..{first}"))),
        Some(span) => Ok(span),
    }
}

struct Cut { scope: String, names: Vec<String>, json: Value, warnings: Vec<Blot> }

struct Sheet<'s> { name: String, label: String, pattern: Cow<'s, Pattern>, first: u64, lead: usize }

struct Play<'s> { cue: &'s Cue, pattern: &'s Pattern, first: u64 }

impl Play<'_> {
    fn last(&self) -> u64 { self.first + u64::from(self.pattern.bars) - 1 }
}

fn playbill(song: &Song) -> Vec<Play<'_>> {
    let mut bill = Vec::new();
    let mut downbeat = 1;
    for cue in &song.setlist {
        let Some(pattern) = song.pattern(&cue.pattern) else { continue };
        bill.push(Play { cue, pattern, first: downbeat });
        downbeat += u64::from(pattern.bars);
    }
    bill
}

fn whole(song: &Song) -> Vec<Sheet<'_>> {
    song.patterns.iter().map(|p| Sheet { name: p.name.clone(), label: measures(p), pattern: Cow::Borrowed(p), first: 1, lead: 0 }).collect()
}

fn snip<'s>(song: &'s Song, crop: &Crop) -> Result<(Vec<Sheet<'s>>, Cut), Vec<Blot>> {
    match crop {
        Crop::Patterns(names) => chosen(song, names),
        Crop::Bars(first, last) => window(song, *first, *last),
    }
}

fn chosen<'s>(song: &'s Song, names: &[String]) -> Result<(Vec<Sheet<'s>>, Cut), Vec<Blot>> {
    let known = song.patterns.iter().map(|p| format!("`{}`", p.name)).collect::<Vec<_>>().join(", ");
    let strangers: Vec<Blot> = names.iter().filter(|n| song.pattern(n).is_none()).map(|n| blot("unknown-pattern", if known.is_empty() { format!("no pattern is named `{n}`; this song declares no patterns") } else { format!("no pattern is named `{n}`; the patterns are {known}") })).collect();
    if !strangers.is_empty() { return Err(strangers); }
    let bill = playbill(song);
    let sheets: Vec<Sheet> = song.patterns.iter().filter(|p| names.contains(&p.name)).map(|p| Sheet { name: p.name.clone(), label: format!("{}, {}", measures(p), billing(p, &bill)), pattern: Cow::Borrowed(p), first: 1, lead: 0 }).collect();
    let shown: Vec<String> = sheets.iter().map(|s| s.name.clone()).collect();
    let cut = Cut { scope: format!("{} of {} patterns, as written", sheets.len(), song.patterns.len()), json: json!({ "patterns": shown }), names: shown, warnings: Vec::new() };
    Ok((sheets, cut))
}

fn billing(pattern: &Pattern, bill: &[Play]) -> String {
    let plays: Vec<&Play> = bill.iter().filter(|p| p.cue.pattern == pattern.name).collect();
    let spans: Vec<String> = plays.iter().take(BILLED).map(|p| format!("{}{}", span(p.first, p.last()), shifted(p.cue.shift))).collect();
    let more = plays.len().saturating_sub(BILLED);
    match plays.len() {
        0 => "never played".into(),
        n => format!("plays at song bar{} {}{}", if n == 1 && pattern.bars == 1 { "" } else { "s" }, spans.join(", "), if more > 0 { format!(", and {more} more") } else { String::new() }),
    }
}

fn window<'s>(song: &'s Song, first: u64, asked: u64) -> Result<(Vec<Sheet<'s>>, Cut), Vec<Blot>> {
    let bill = playbill(song);
    let length = bill.last().map_or(0, Play::last);
    if first > length { return Err(vec![blot("bars-past-end", format!("the song is {length} bar{} long as it plays, so bar {first} never sounds", if length == 1 { "" } else { "s" }))]); }
    let last = asked.min(length);
    let mut sheets = Vec::new();
    for play in bill.iter().filter(|p| p.first <= last && p.last() >= first) {
        let heard = transposed(song, play.pattern, play.cue).map_err(|e| vec![unplayable(&e, play.cue)])?;
        let local = |bar: u64| u32::try_from(bar - play.first + 1).expect("a bar inside a play fits its pattern's u32 bar count");
        let (from, to) = (local(first.max(play.first)), local(last.min(play.last())));
        let entire = (from, to) == (1, play.pattern.bars);
        let label = if entire { measures(play.pattern) } else { format!("{} of {}, grid {}", bars(from.into(), to.into()), play.pattern.bars, play.pattern.grid) };
        let downbeat = play.first + u64::from(from) - 1;
        let pattern = if entire { heard } else { Cow::Owned(excerpt(&heard, from, to)) };
        sheets.push(Sheet { name: format!("{}{}", play.cue.pattern, shifted(play.cue.shift)), label, pattern, first: downbeat, lead: usize::try_from(downbeat - first).unwrap_or(usize::MAX) });
    }
    let warnings = if asked > length { vec![blot("crop-trimmed", format!("the song ends at bar {length} as it plays, so the crop stops there instead of bar {asked}"))] } else { Vec::new() };
    let cut = Cut { scope: format!("song {} of {length}, as played", bars(first, last)), names: sheets.iter().map(|s| s.name.clone()).collect(), json: json!({ "bars": [first, last], "song_bars": length }), warnings };
    Ok((sheets, cut))
}

fn excerpt(pattern: &Pattern, from: u32, to: u32) -> Pattern {
    let grid = u64::from(pattern.grid);
    let (open, close) = (u64::from(from - 1) * grid, u64::from(to) * grid);
    let lanes = pattern.lanes.iter().map(|lane| {
        let notes = lane.notes.iter().filter_map(|note| {
            let start = note.at.tick(pattern.grid);
            let (head, tail) = (start.max(open), (start + u64::from(note.len)).min(close));
            (head < tail).then(|| Note { at: Position::from_tick(head - open, pattern.grid), len: u32::try_from(tail - head).expect("a clipped note is never longer than the note"), ..note.clone() })
        }).collect();
        Lane { name: lane.name.clone(), params: lane.params.clone(), notes, line: lane.line }
    }).collect();
    Pattern { name: pattern.name.clone(), bars: to - from + 1, grid: pattern.grid, lanes, line: pattern.line }
}

fn measures(p: &Pattern) -> String { format!("{} bar{}, grid {}", p.bars, if p.bars == 1 { "" } else { "s" }, p.grid) }

fn span(first: u64, last: u64) -> String { if first == last { first.to_string() } else { format!("{first}-{last}") } }

fn bars(first: u64, last: u64) -> String { format!("bar{} {}", if first == last { "" } else { "s" }, span(first, last)) }

fn shifted(shift: i8) -> String { if shift == 0 { String::new() } else { format!(" {shift:+}") } }

struct Blot { human: String, json: Value }

fn blot(code: &str, message: String) -> Blot {
    Blot { json: json!({ "code": code, "message": message, "pattern": null, "lane": null, "at": null, "line": null }), human: message }
}

fn misprint(e: &ScoreError) -> Blot {
    Blot { json: json!({ "code": e.fault.code(), "message": e.fault.to_string(), "pattern": e.pattern, "lane": e.lane, "at": e.at.map(|at| at.to_string()), "line": e.line }), human: e.to_string() }
}

fn unplayable(e: &RenderError, cue: &Cue) -> Blot {
    let (code, lane, at) = match e {
        RenderError::Shifted { lane, at, .. } => ("transposed-out-of-range", Some(lane), Some(at)),
        RenderError::Ornament { lane, at, flaw, .. } => (flaw.code(), Some(lane), Some(at)),
        RenderError::Profile(_) => ("unknown-profile", None, None),
        _ => ("unplayable", None, None),
    };
    Blot { json: json!({ "code": code, "message": e.to_string(), "pattern": cue.pattern, "lane": lane, "at": at.map(|at| at.to_string()), "line": cue.line }), human: e.to_string() }
}


struct Band<'s> { lane: &'s Lane, hue: Rgb, pitches: Option<(u8, u8)>, reach: usize, captions: Vec<String>, height: usize }

impl<'s> Band<'s> {
    fn of(lane: &'s Lane, hue: Rgb) -> Self {
        let pitches = lane.notes.iter().filter_map(|n| n.pitch).fold(None, |span: Option<(u8, u8)>, p| Some(span.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p)))));
        let reach = pitches.map_or(NOTE, |(lo, hi)| usize::from(hi - lo) * SEMITONE + NOTE);
        let mut captions = vec![lane.name.clone()];
        captions.extend(pitches.map(|(lo, hi)| if lo == hi { named(lo) } else { format!("{}-{}", named(lo), named(hi)) }));
        captions.extend(lane.params.iter().map(|(key, value)| format!("{key} {value}")));
        let height = reach.max(captions.len() * LINE - (LINE - TEXT)) + 2 * ROOM;
        Band { lane, hue, pitches, reach, captions, height }
    }
}

fn roll(song: &Song, sheets: &[Sheet], crop: Option<&str>) -> Result<Canvas, String> {
    let mut strays: Vec<&str> = Vec::new();
    for lane in song.patterns.iter().flat_map(|p| &p.lanes) {
        if VOICES.iter().all(|(voice, _)| *voice != lane.name) && !strays.contains(&lane.name.as_str()) { strays.push(&lane.name); }
    }
    let hue = |name: &str| VOICES.iter().find(|(voice, _)| *voice == name).map_or_else(|| STRAYS[strays.iter().position(|s| *s == name).unwrap_or(0) % STRAYS.len()], |(_, rgb)| *rgb);
    let staves: Vec<(&Sheet, Vec<Band>)> = sheets.iter().map(|s| (s, s.pattern.lanes.iter().map(|l| Band::of(l, hue(&l.name))).collect())).collect();
    let widest = sheets.iter().map(|s| s.lead.saturating_add(s.pattern.bars as usize)).max().unwrap_or(1).max(1);
    let bar_px = (1024 / widest).clamp(64, 512);
    let gutter = staves.iter().flat_map(|(_, bands)| bands).flat_map(|b| &b.captions).map(|c| measure(c, INK) + MARGIN).max().unwrap_or(0);
    let left = MARGIN + gutter;
    let heading = song.profile.as_ref().map_or_else(|| format!("tempo {}", song.tempo), |p| format!("tempo {}, profile {p}", song.tempo));
    let crop = crop.map(|scope| format!("crop: {scope}"));
    let titled = MARGIN + measure(&song.title, TITLE) + MARGIN + measure(&heading, INK) + crop.as_ref().map_or(0, |c| MARGIN + measure(c, INK));
    let named_widest = sheets.iter().map(|s| MARGIN + measure(&s.name, INK) + 12 + measure(&s.label, INK)).max().unwrap_or(0);
    let width = widest.saturating_mul(bar_px).saturating_add(left).max(titled).max(named_widest) + MARGIN;
    let tall = |bands: &[Band]| LINE + 4 + RULER + bands.iter().map(|b| b.height + 2).sum::<usize>();
    let height = staves.iter().fold(MARGIN + HEAD, |y, (_, bands)| y + tall(bands) + GAP).saturating_sub(GAP).max(MARGIN + HEAD) + MARGIN;
    let way_out = if crop.is_some() { "narrow the crop" } else { "crop it with --bars or --patterns" };
    if width > LIMIT || height > LIMIT { return Err(format!("the roll would be {width}x{height} pixels, past the {LIMIT}-pixel limit on a side; that's too many bars or lanes for one picture, so {way_out}")); }

    let mut canvas = Canvas::new(width, height);
    let mut y = MARGIN;
    let x = canvas.write(MARGIN, y, &song.title, TITLE, CHALK);
    let x = canvas.write(x + MARGIN, y + (TITLE - INK) * GLYPH_H, &heading, INK, SMUDGE);
    if let Some(crop) = &crop { canvas.write(x + MARGIN, y + (TITLE - INK) * GLYPH_H, crop, INK, CHALK); }
    y += HEAD;
    for (sheet, bands) in &staves {
        let x = canvas.write(MARGIN, y, &sheet.name, INK, CHALK);
        canvas.write(x + 12, y, &sheet.label, INK, SMUDGE);
        y += LINE + 4;
        let (pattern, bars, grid) = (&sheet.pattern, sheet.pattern.bars as usize, sheet.pattern.grid as usize);
        let left = left + sheet.lead * bar_px;
        let span = bars * bar_px;
        let at = |tick: usize| left + tick * bar_px / grid;
        let beats: Vec<usize> = if grid.is_multiple_of(4) { (0..bars * 4).map(|b| b * grid / 4).filter(|t| !t.is_multiple_of(grid)).collect() } else { Vec::new() };
        for bar in 0..bars { canvas.write(at(bar * grid) + 6, y, &(sheet.first + bar as u64).to_string(), INK, CHALK); }
        if bar_px / grid >= 4 { for tick in 0..bars * grid { canvas.fill(at(tick), y + RULER - 5, 1, 5, SMUDGE); } }
        for &tick in &beats { canvas.fill(at(tick), y + RULER - 10, 1, 10, SMUDGE); }
        for bar in 0..=bars { canvas.fill(at(bar * grid) - 1, y, 2, RULER, BARLINE); }
        y += RULER;
        for band in bands {
            canvas.fill(left, y, span, band.height, STAFF);
            for &tick in &beats { canvas.fill(at(tick), y, 1, band.height, BEATLINE); }
            for bar in 0..=bars { canvas.fill(at(bar * grid) - 1, y, 2, band.height, BARLINE); }
            for (i, caption) in band.captions.iter().enumerate() { canvas.write(MARGIN, y + ROOM + i * LINE, caption, INK, if i == 0 { band.hue } else { SMUDGE }); }
            if let Some((_, hi)) = band.pitches {
                let ceiling = y + (band.height - band.reach) / 2;
                for note in &band.lane.notes {
                    let Some(pitch) = note.pitch else { continue };
                    let start = note.at.tick(pattern.grid) as usize;
                    let (x0, x1) = (at(start) + 1, at(start + note.len as usize) - 1);
                    let top = ceiling + usize::from(hi - pitch) * SEMITONE;
                    canvas.fill(x0, top, x1.saturating_sub(x0), NOTE, band.hue);
                    let name = named(pitch);
                    if measure(&name, INK) + 6 <= x1.saturating_sub(x0) { canvas.write(x0 + 3, top + (NOTE - TEXT) / 2, &name, INK, PAPER); }
                }
            }
            y += band.height + 2;
        }
        y += GAP;
    }
    Ok(canvas)
}

fn named(pitch: u8) -> String { format!("{}{}", SCIENTIFIC[usize::from(pitch % 12)], i32::from(pitch / 12) - 1) }

fn measure(text: &str, scale: usize) -> usize { (text.chars().count() * 6).saturating_sub(1) * scale }

struct Canvas { width: usize, height: usize, ink: Vec<u8> }

impl Canvas {
    fn new(width: usize, height: usize) -> Self { Canvas { width, height, ink: PAPER.repeat(width * height) } }

    fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, rgb: Rgb) {
        let (right, bottom) = ((x + w).min(self.width), (y + h).min(self.height));
        if x >= right { return; }
        for row in y..bottom { self.ink[(row * self.width + x) * 3..(row * self.width + right) * 3].as_chunks_mut::<3>().0.iter_mut().for_each(|px| *px = rgb); }
    }

    fn write(&mut self, x: usize, y: usize, text: &str, scale: usize, rgb: Rgb) -> usize {
        for (i, c) in text.chars().enumerate() {
            for (r, row) in glyph(c).iter().enumerate() {
                for (k, _) in row.bytes().enumerate().filter(|(_, b)| *b == b'#') { self.fill(x + (i * 6 + k) * scale, y + r * scale, scale, scale, rgb); }
            }
        }
        x + measure(text, scale)
    }

    fn png(&self) -> Result<Vec<u8>, png::EncodingError> {
        let mut out = Vec::new();
        let side = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let mut encoder = png::Encoder::new(&mut out, side(self.width), side(self.height));
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.ink)?;
        writer.finish()?;
        Ok(out)
    }
}

fn glyph(c: char) -> &'static [&'static str; GLYPH_H] { GLYPHS.iter().find(|(g, _)| *g == c).map_or(&TOFU, |(_, rows)| rows) }

static TOFU: [&str; GLYPH_H] = ["#####", "#...#", "#...#", "#...#", "#...#", "#...#", "#####", "....."];

static GLYPHS: [(char, [&str; GLYPH_H]); 95] = [
    (' ', [".....", ".....", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('!', ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..", "....."]),
    ('"', [".#.#.", ".#.#.", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('#', [".#.#.", ".#.#.", "#####", ".#.#.", "#####", ".#.#.", ".#.#.", "....."]),
    ('$', ["..#..", ".####", "#.#..", ".###.", "..#.#", "####.", "..#..", "....."]),
    ('%', ["##...", "##..#", "...#.", "..#..", ".#...", "#..##", "...##", "....."]),
    ('&', [".##..", "#..#.", "#.#..", ".#...", "#.#.#", "#..#.", ".##.#", "....."]),
    ('\'', ["..#..", "..#..", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('(', ["...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#.", "....."]),
    (')', [".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#...", "....."]),
    ('*', [".....", "..#..", "#.#.#", ".###.", "#.#.#", "..#..", ".....", "....."]),
    ('+', [".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....", "....."]),
    (',', [".....", ".....", ".....", ".....", ".....", "..#..", "..#..", ".#..."]),
    ('-', [".....", ".....", ".....", "#####", ".....", ".....", ".....", "....."]),
    ('.', [".....", ".....", ".....", ".....", ".....", ".....", "..#..", "....."]),
    ('/', [".....", "....#", "...#.", "..#..", ".#...", "#....", ".....", "....."]),
    ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.", "....."]),
    ('1', ["..#..", ".##..", "#.#..", "..#..", "..#..", "..#..", "#####", "....."]),
    ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####", "....."]),
    ('3', ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###.", "....."]),
    ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.", "....."]),
    ('5', ["#####", "#....", "####.", "....#", "....#", "#...#", ".###.", "....."]),
    ('6', ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.", "....."]),
    ('7', ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...", "....."]),
    ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.", "....."]),
    ('9', [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..", "....."]),
    (':', [".....", ".....", "..#..", ".....", ".....", "..#..", ".....", "....."]),
    (';', [".....", ".....", "..#..", ".....", ".....", "..#..", "..#..", ".#..."]),
    ('<', ["...#.", "..#..", ".#...", "#....", ".#...", "..#..", "...#.", "....."]),
    ('=', [".....", ".....", "#####", ".....", "#####", ".....", ".....", "....."]),
    ('>', [".#...", "..#..", "...#.", "....#", "...#.", "..#..", ".#...", "....."]),
    ('?', [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#..", "....."]),
    ('@', [".###.", "#...#", "#.###", "#.#.#", "#.###", "#....", ".####", "....."]),
    ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", "....."]),
    ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.", "....."]),
    ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.", "....."]),
    ('D', ["###..", "#..#.", "#...#", "#...#", "#...#", "#..#.", "###..", "....."]),
    ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####", "....."]),
    ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#....", "....."]),
    ('G', [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####", "....."]),
    ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", "....."]),
    ('I', [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", "....."]),
    ('J', ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..", "....."]),
    ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#", "....."]),
    ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####", "....."]),
    ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#", "....."]),
    ('N', ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "....."]),
    ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", "....."]),
    ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#....", "....."]),
    ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#", "....."]),
    ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#", "....."]),
    ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####.", "....."]),
    ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", "....."]),
    ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", "....."]),
    ('V', ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..", "....."]),
    ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#.", "....."]),
    ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#", "....."]),
    ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..", "....."]),
    ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####", "....."]),
    ('[', [".###.", ".#...", ".#...", ".#...", ".#...", ".#...", ".###.", "....."]),
    ('\\', [".....", "#....", ".#...", "..#..", "...#.", "....#", ".....", "....."]),
    (']', [".###.", "...#.", "...#.", "...#.", "...#.", "...#.", ".###.", "....."]),
    ('^', ["..#..", ".#.#.", "#...#", ".....", ".....", ".....", ".....", "....."]),
    ('_', [".....", ".....", ".....", ".....", ".....", ".....", ".....", "#####"]),
    ('`', [".#...", "..#..", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('a', [".....", ".....", ".###.", "....#", ".####", "#...#", ".####", "....."]),
    ('b', ["#....", "#....", "####.", "#...#", "#...#", "#...#", "####.", "....."]),
    ('c', [".....", ".....", ".###.", "#....", "#....", "#...#", ".###.", "....."]),
    ('d', ["....#", "....#", ".####", "#...#", "#...#", "#...#", ".####", "....."]),
    ('e', [".....", ".....", ".###.", "#...#", "#####", "#....", ".###.", "....."]),
    ('f', ["..##.", ".#..#", ".#...", "###..", ".#...", ".#...", ".#...", "....."]),
    ('g', [".....", ".....", ".####", "#...#", "#...#", ".####", "....#", ".###."]),
    ('h', ["#....", "#....", "#.##.", "##..#", "#...#", "#...#", "#...#", "....."]),
    ('i', ["..#..", ".....", ".##..", "..#..", "..#..", "..#..", ".###.", "....."]),
    ('j', ["...#.", ".....", "..##.", "...#.", "...#.", "...#.", "#..#.", ".##.."]),
    ('k', ["#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "....."]),
    ('l', [".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", "....."]),
    ('m', [".....", ".....", "##.#.", "#.#.#", "#.#.#", "#.#.#", "#.#.#", "....."]),
    ('n', [".....", ".....", "#.##.", "##..#", "#...#", "#...#", "#...#", "....."]),
    ('o', [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###.", "....."]),
    ('p', [".....", ".....", "####.", "#...#", "#...#", "####.", "#....", "#...."]),
    ('q', [".....", ".....", ".####", "#...#", "#...#", ".####", "....#", "....#"]),
    ('r', [".....", ".....", "#.##.", "##..#", "#....", "#....", "#....", "....."]),
    ('s', [".....", ".....", ".####", "#....", ".###.", "....#", "####.", "....."]),
    ('t', [".#...", ".#...", "###..", ".#...", ".#...", ".#..#", "..##.", "....."]),
    ('u', [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#", "....."]),
    ('v', [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..", "....."]),
    ('w', [".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#.", "....."]),
    ('x', [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "....."]),
    ('y', [".....", ".....", "#...#", "#...#", "#...#", ".####", "....#", ".###."]),
    ('z', [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####", "....."]),
    ('{', ["..##.", "..#..", "..#..", "##...", "..#..", "..#..", "..##.", "....."]),
    ('|', ["..#..", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", "....."]),
    ('}', [".##..", "..#..", "..#..", "...##", "..#..", "..#..", ".##..", "....."]),
    ('~', [".....", ".....", ".#...", "#.#.#", "...#.", ".....", ".....", "....."]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use piropipo::score::Fault;

    fn spec_example() -> &'static str {
        let doc = include_str!("../skills/piropipo/references/format.md");
        let (_, opened) = doc.split_once("```\n").expect("format.md lost its example");
        opened.split_once("```").expect("format.md example fence never closes").0
    }

    fn rolled(text: &str) -> Canvas {
        let song = Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}"));
        roll(&song, &whole(&song), None).unwrap_or_else(|why| panic!("{why}"))
    }

    fn pixel(canvas: &Canvas, x: usize, y: usize) -> Rgb {
        let at = (y * canvas.width + x) * 3;
        [canvas.ink[at], canvas.ink[at + 1], canvas.ink[at + 2]]
    }

    #[test]
    fn spec_example_rolls_to_a_png_that_decodes_back_to_itself() {
        let canvas = rolled(spec_example());
        let png = canvas.png().unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(&png)).read_info().unwrap();
        let (width, height) = (reader.info().width, reader.info().height);
        assert_eq!((width, height), (1166, 422));
        assert!((800..=1600).contains(&width), "a 2-bar grid-16 roll should land between 800 and 1600 wide, got {width}");
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut decoded).unwrap();
        assert!(decoded == canvas.ink, "the png doesn't decode to the pixels that were drawn");
    }

    #[test]
    fn notes_land_where_the_grid_says_and_rests_stay_empty() {
        let canvas = rolled("song \"t\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n  1.1: C4 x4\n  1.5: C4 x4\n  1.9: C5 x2\n  1.13: - x4\n");
        let coral = VOICES[0].1;
        let left = canvas.width - MARGIN - 512;
        let lit: Vec<(usize, usize)> = (left..canvas.width).filter_map(|x| (0..canvas.height).find(|&y| pixel(&canvas, x, y) == coral).map(|top| (x, top))).collect();
        let mut runs: Vec<(usize, usize, usize)> = Vec::new();
        for (x, top) in lit {
            match runs.last_mut() { Some(run) if run.1 == x => run.1 = x + 1, _ => runs.push((x, x + 1, top)) }
        }
        assert_eq!(runs.iter().map(|r| (r.0 - left, r.1 - left)).collect::<Vec<_>>(), [(1, 127), (129, 255), (257, 319)], "two seamed C4s, one short C5, then a written rest drawn as nothing");
        assert_eq!(runs[0].2, runs[1].2);
        assert_eq!(runs[0].2 - runs[2].2, 12 * SEMITONE, "an octave up should sit twelve semitone steps higher");
    }

    #[test]
    fn a_roll_too_big_to_look_at_is_refused_not_allocated() {
        let song = Song::parse("song \"t\" tempo 120\npattern p bars 400 grid 4\nlane pulse1\n  1.1: C4 x1\n").unwrap();
        assert!(roll(&song, &whole(&song), None).is_err_and(|why| why.starts_with("the roll would be 25718x") && why.ends_with("crop it with --bars or --patterns")));
    }

    const DUET: &str = "song \"t\" tempo 120\npattern a bars 2 grid 4\nlane pulse1\n  1.1: C4 x2\n  1.3: E4 x4\n  2.3: D4 x2\npattern b bars 2 grid 4\nlane pulse1\n  1.1: D4 x8\npattern c bars 1 grid 4\nlane pulse2\n  1: C3 x4\nplay\n  a\n  b\n  a +2\n";

    fn duet() -> Song { Song::parse(DUET).unwrap_or_else(|errors| panic!("the duet doesn't parse: {errors:?}")) }

    fn cropped<'s>(song: &'s Song, crop: &Crop) -> (Vec<Sheet<'s>>, Cut) { snip(song, crop).unwrap_or_else(|blots| panic!("{:?}", blots.iter().map(|b| &b.human).collect::<Vec<_>>())) }

    fn codes(blots: &[Blot]) -> Vec<&str> { blots.iter().map(|b| b.json["code"].as_str().unwrap()).collect() }

    fn asked(patterns: Option<&str>, bars: Option<&str>) -> Result<Option<Crop>, Blot> { Crop::asked(&Args { input: "song.piro".into(), output: None, patterns: patterns.map(Into::into), bars: bars.map(Into::into) }) }

    #[test]
    fn a_whole_unshifted_play_crops_to_the_full_rolls_own_sheet_pixel_for_pixel() {
        let song = duet();
        let full = roll(&song, &whole(&song), None).unwrap();
        let (sheets, cut) = cropped(&song, &Crop::Bars(1, 2));
        let crop = roll(&song, &sheets, Some(&cut.scope)).unwrap();
        assert_eq!(full.width, crop.width);
        let row = |y: usize| y * crop.width * 3;
        let sheet = row(MARGIN + HEAD)..row(crop.height - MARGIN);
        assert!(full.ink[sheet.clone()] == crop.ink[sheet], "bars 1-2 are pattern a played as written, so they should draw exactly as a does in the full roll");
        assert!(full.ink[..row(MARGIN + HEAD)] != crop.ink[..row(MARGIN + HEAD)], "the crop has to say it's a crop up top");
    }

    #[test]
    fn a_bars_crop_walks_the_song_as_it_plays() {
        let song = duet();
        let (sheets, cut) = cropped(&song, &Crop::Bars(2, 5));
        assert_eq!(sheets.iter().map(|s| (s.name.as_str(), s.label.as_str(), s.first, s.lead)).collect::<Vec<_>>(), [("a", "bar 2 of 2, grid 4", 2, 0), ("b", "2 bars, grid 4", 3, 1), ("a +2", "bar 1 of 2, grid 4", 5, 3)]);
        assert!(matches!(sheets[1].pattern, Cow::Borrowed(_)), "a whole unshifted play is drawn straight from the score");
        let notes = |s: &Sheet| s.pattern.lanes[0].notes.iter().map(|n| (n.at.to_string(), n.pitch, n.len)).collect::<Vec<_>>();
        assert_eq!(notes(&sheets[0]), [("1.1".into(), Some(64), 2), ("1.3".into(), Some(62), 2)], "the E4 ringing over the bar line keeps only its tail");
        assert_eq!(notes(&sheets[2]), [("1.1".into(), Some(62), 2), ("1.3".into(), Some(66), 2)], "the third play is a up two semitones, and its E4 is cut at the crop's edge");
        assert_eq!((cut.scope.as_str(), cut.names.join(", ")), ("song bars 2-5 of 6, as played", "a, b, a +2".into()));
        assert_eq!(cut.json, json!({ "bars": [2, 5], "song_bars": 6 }));
        assert!(cut.warnings.is_empty());
    }

    #[test]
    fn a_patterns_crop_draws_each_pattern_once_and_says_where_it_plays() {
        let song = duet();
        let (sheets, cut) = cropped(&song, &Crop::Patterns(vec!["c".into(), "a".into(), "a".into()]));
        assert_eq!(sheets.iter().map(|s| (s.name.as_str(), s.label.as_str(), s.first, s.lead)).collect::<Vec<_>>(), [("a", "2 bars, grid 4, plays at song bars 1-2, 5-6 +2", 1, 0), ("c", "1 bar, grid 4, never played", 1, 0)]);
        assert_eq!(cut.scope, "2 of 3 patterns, as written");
        assert_eq!(cut.json, json!({ "patterns": ["a", "c"] }));
        let encore = Song::parse(&format!("song \"t\" tempo 120\npattern ding bars 1 grid 4\nlane pulse1\n  1: C5 x4\nplay\n{}", "  ding\n".repeat(6))).unwrap();
        assert_eq!(cropped(&encore, &Crop::Patterns(vec!["ding".into()])).0[0].label, "1 bar, grid 4, plays at song bars 1, 2, 3, 4, and 2 more");
        let once = Song::parse("song \"t\" tempo 120\npattern ding bars 1 grid 4\nlane pulse1\n  1: C5 x4\n").unwrap();
        assert_eq!(cropped(&once, &Crop::Patterns(vec!["ding".into()])).0[0].label, "1 bar, grid 4, plays at song bar 1");
    }

    #[test]
    fn crops_are_refused_in_plain_words() {
        assert_eq!(asked(Some("a"), Some("1..2")).err().map(|b| b.json["code"].clone()), Some(json!("two-crops")));
        assert_eq!(asked(Some(" , "), None).err().map(|b| b.json["code"].clone()), Some(json!("empty-crop")));
        assert!(matches!(asked(Some("verse, chorus"), None), Ok(Some(Crop::Patterns(names))) if names == ["verse", "chorus"]));
        assert!(matches!(asked(None, None), Ok(None)));
        for (text, span) in [("17..24", (17, 24)), ("17-24", (17, 24)), ("17", (17, 17)), (" 3 .. 3 ", (3, 3))] { assert_eq!(spanned(text).ok(), Some(span), "{text}"); }
        for (text, why) in [("17:24", "isn't a span"), ("1..2..3", "isn't a span"), ("-4", "isn't a span"), ("0..4", "count from 1"), ("24..17", "like 17..24")] {
            assert!(spanned(text).is_err_and(|b| b.human.contains(why)), "{text} should be refused with {why:?}");
        }
        let song = duet();
        let strangers = snip(&song, &Crop::Patterns(vec!["a".into(), "chorus".into()])).err().unwrap();
        assert_eq!(codes(&strangers), ["unknown-pattern"]);
        assert_eq!(strangers[0].human, "no pattern is named `chorus`; the patterns are `a`, `b`, `c`");
        assert_eq!(codes(&snip(&song, &Crop::Bars(7, 9)).err().unwrap()), ["bars-past-end"]);
        let (sheets, trimmed) = cropped(&song, &Crop::Bars(5, 40));
        assert_eq!((sheets.len(), trimmed.scope.as_str()), (1, "song bars 5-6 of 6, as played"));
        assert_eq!(codes(&trimmed.warnings), ["crop-trimmed"]);
    }

    #[test]
    fn a_transpose_past_the_chip_is_refused_not_drawn() {
        let song = Song::parse("song \"t\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: B7 x4\nplay\n  p\n  p +12\n").unwrap();
        assert_eq!(cropped(&song, &Crop::Bars(1, 1)).0.len(), 1);
        let refused = snip(&song, &Crop::Bars(1, 2)).err().unwrap();
        assert_eq!(codes(&refused), ["transposed-out-of-range"]);
        assert_eq!((refused[0].json["pattern"].clone(), refused[0].json["line"].clone()), (json!("p"), json!(7)));
    }

    #[test]
    fn a_crop_of_a_four_billion_bar_song_costs_nothing() {
        let song = Song::parse("song \"t\" tempo 120\npattern endless bars 4000000000 grid 255\nlane pulse1\n  1.1: C4 x1\n  4000000000.255: E4 x1\nplay\n  endless\n  endless +12\n").unwrap();
        let (sheets, cut) = cropped(&song, &Crop::Bars(3_999_999_999, 4_000_000_002));
        assert_eq!(sheets.iter().map(|s| (s.name.as_str(), s.label.as_str(), s.first, s.lead)).collect::<Vec<_>>(), [("endless", "bars 3999999999-4000000000 of 4000000000, grid 255", 3_999_999_999, 0), ("endless +12", "bars 1-2 of 4000000000, grid 255", 4_000_000_001, 2)]);
        assert_eq!(sheets[0].pattern.lanes[0].notes.iter().map(|n| (n.at.to_string(), n.pitch)).collect::<Vec<_>>(), [("2.255".into(), Some(64))]);
        assert_eq!(sheets[1].pattern.lanes[0].notes.iter().map(|n| (n.at.to_string(), n.pitch)).collect::<Vec<_>>(), [("1.1".into(), Some(72))]);
        let canvas = roll(&song, &sheets, Some(&cut.scope)).unwrap();
        assert!(canvas.width < 2048, "four bars should draw small, got {}", canvas.width);
        let (everything, all) = cropped(&song, &Crop::Bars(1, u64::MAX));
        assert!(roll(&song, &everything, Some(&all.scope)).is_err_and(|why| why.ends_with("narrow the crop")));
        let (endless, named) = cropped(&song, &Crop::Patterns(vec!["endless".into()]));
        assert!(roll(&song, &endless, Some(&named.scope)).is_err());
    }

    #[test]
    fn a_crop_lands_under_its_own_name_and_spares_the_full_roll() {
        assert_eq!([Crop::Bars(17, 24).tag(), Crop::Bars(5, 5).tag(), Crop::Patterns(vec!["verse".into(), "a/b".into()]).tag()], ["bars-17-24", "bar-5", "verse+a_b"]);
        let shelf = std::env::temp_dir().join(format!("piro-roll-crop-{}", std::process::id()));
        std::fs::create_dir_all(&shelf).unwrap();
        let score = shelf.join("duet.piro");
        std::fs::write(&score, DUET).unwrap();
        std::fs::write(shelf.join("duet.png"), b"the full roll").unwrap();
        let args = Args { input: score.to_string_lossy().into(), output: None, patterns: None, bars: Some("3..4".into()) };
        let Rolled { target: landed, cut, .. } = unroll(&args).unwrap_or_else(|blots| panic!("{:?}", blots.iter().map(|b| &b.human).collect::<Vec<_>>()));
        assert_eq!(landed, shelf.join("duet.bars-3-4.png"));
        assert_eq!(cut.map(|c| c.names), Some(vec!["b".to_owned()]));
        assert_eq!(std::fs::read(shelf.join("duet.png")).unwrap(), b"the full roll");
        let stdin = Args { input: "-".into(), output: None, patterns: Some("a".into()), bars: None };
        assert_eq!(unroll(&stdin).err().map(|blots| codes(&blots).join(",")), Some("no-output".into()));
        std::fs::remove_dir_all(&shelf).unwrap();
    }

    #[test]
    fn fault_codes_come_from_the_one_list_in_score() {
        assert_eq!(Fault::SlotPastGrid { slot: 17, grid: 16, meant: None }.code(), "slot-past-grid");
        assert_eq!(Fault::NoName("lane").code(), "no-name");
        assert_eq!(Fault::Silent.code(), "zero-length");
    }

    #[test]
    fn publish_lands_whole_and_leaves_no_draft() {
        let shelf = std::env::temp_dir().join(format!("piro-roll-{}", std::process::id()));
        std::fs::create_dir_all(&shelf).unwrap();
        let target = shelf.join("song.png");
        publish(&target, b"first").unwrap();
        publish(&target, b"second").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        assert_eq!(std::fs::read_dir(&shelf).unwrap().count(), 1);
        assert!(publish(&shelf.join("missing").join("song.png"), b"x").is_err());
        std::fs::remove_dir_all(&shelf).unwrap();
    }

    #[test]
    fn the_font_covers_printable_ascii_once_each() {
        for (i, (c, rows)) in GLYPHS.iter().enumerate() {
            assert_eq!(u32::from(*c), 32 + u32::try_from(i).unwrap(), "glyph {i} is out of ascii order");
            assert!(rows.iter().all(|r| r.len() == 5 && r.bytes().all(|b| b == b'#' || b == b'.')), "{c:?} has a malformed row");
        }
    }
}
