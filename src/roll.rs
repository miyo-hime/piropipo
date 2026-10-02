use crate::surface::{emit, land_beside, publish, read_source};
use piropipo::score::{Lane, Pattern, ScoreError, Song};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
    #[arg(short, long, help = "where the png lands (default: beside the input)")]
    pub output: Option<PathBuf>,
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

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> {
    match unroll(&args) {
        Ok((path, song, [width, height])) => {
            let shown = path.display().to_string();
            let count = song.patterns.len();
            let sentence = format!("rolled \"{}\" onto {shown}: {count} pattern{}, {width}x{height} pixels", song.title, if count == 1 { "" } else { "s" });
            emit(human, json!({ "ok": true, "verb": "roll", "output": shown, "stats": { "png": shown, "width": width, "height": height }, "warnings": [], "errors": [] }), &sentence);
            Ok(())
        }
        Err(blots) => {
            if !human { println!("{}", json!({ "ok": false, "verb": "roll", "output": null, "stats": null, "warnings": [], "errors": blots.iter().map(|b| &b.json).collect::<Vec<_>>() })); }
            Err(blots.into_iter().map(|b| b.human).collect::<Vec<_>>().join("\npiro: ").into())
        }
    }
}

fn unroll(args: &Args) -> Result<(PathBuf, Song, [usize; 2]), Vec<Blot>> {
    let origin = (args.input != "-").then(|| Path::new(&args.input));
    let target = land_beside(origin, args.output.clone(), "png").map_err(|why| vec![blot("no-output", why)])?;
    let (text, _) = read_source(&args.input).map_err(|e| vec![blot("unreadable-input", format!("can't read {}: {e}", origin.map_or("stdin".into(), |p| p.display().to_string())))])?;
    let song = Song::parse(&text).map_err(|errors| errors.iter().map(misprint).collect::<Vec<_>>())?;
    let canvas = roll(&song).map_err(|why| vec![blot("too-large", why)])?;
    let png = canvas.png().map_err(|e| vec![blot("unencodable", format!("the png encoder refused the roll: {e}"))])?;
    publish(&target, &png).map_err(|e| vec![blot("unwritable-output", format!("can't write {}: {e}", target.display()))])?;
    Ok((target, song, [canvas.width, canvas.height]))
}

struct Blot { human: String, json: Value }

fn blot(code: &str, message: String) -> Blot {
    Blot { json: json!({ "code": code, "message": message, "pattern": null, "lane": null, "at": null, "line": null }), human: message }
}

fn misprint(e: &ScoreError) -> Blot {
    Blot { json: json!({ "code": e.fault.code(), "message": e.fault.to_string(), "pattern": e.pattern, "lane": e.lane, "at": e.at.map(|at| at.to_string()), "line": e.line }), human: e.to_string() }
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

fn roll(song: &Song) -> Result<Canvas, String> {
    let mut strays: Vec<&str> = Vec::new();
    for lane in song.patterns.iter().flat_map(|p| &p.lanes) {
        if VOICES.iter().all(|(voice, _)| *voice != lane.name) && !strays.contains(&lane.name.as_str()) { strays.push(&lane.name); }
    }
    let hue = |name: &str| VOICES.iter().find(|(voice, _)| *voice == name).map_or_else(|| STRAYS[strays.iter().position(|s| *s == name).unwrap_or(0) % STRAYS.len()], |(_, rgb)| *rgb);
    let sheets: Vec<(&Pattern, Vec<Band>)> = song.patterns.iter().map(|p| (p, p.lanes.iter().map(|l| Band::of(l, hue(&l.name))).collect())).collect();
    let widest = song.patterns.iter().map(|p| p.bars as usize).max().unwrap_or(1).max(1);
    let bar_px = (1024 / widest).clamp(64, 512);
    let gutter = sheets.iter().flat_map(|(_, bands)| bands).flat_map(|b| &b.captions).map(|c| measure(c, INK) + MARGIN).max().unwrap_or(0);
    let left = MARGIN + gutter;
    let heading = song.profile.as_ref().map_or_else(|| format!("tempo {}", song.tempo), |p| format!("tempo {}, profile {p}", song.tempo));
    let labels: Vec<String> = song.patterns.iter().map(|p| format!("{} bar{}, grid {}", p.bars, if p.bars == 1 { "" } else { "s" }, p.grid)).collect();
    let titled = MARGIN + measure(&song.title, TITLE) + MARGIN + measure(&heading, INK);
    let named_widest = sheets.iter().zip(&labels).map(|((p, _), label)| MARGIN + measure(&p.name, INK) + 12 + measure(label, INK)).max().unwrap_or(0);
    let width = (left + widest * bar_px).max(titled).max(named_widest) + MARGIN;
    let tall = |bands: &[Band]| LINE + 4 + RULER + bands.iter().map(|b| b.height + 2).sum::<usize>();
    let height = sheets.iter().fold(MARGIN + HEAD, |y, (_, bands)| y + tall(bands) + GAP).saturating_sub(GAP).max(MARGIN + HEAD) + MARGIN;
    if width > LIMIT || height > LIMIT { return Err(format!("the roll would be {width}x{height} pixels, past the {LIMIT}-pixel limit on a side; that's too many bars or lanes for one picture")); }

    let mut canvas = Canvas::new(width, height);
    let mut y = MARGIN;
    let x = canvas.write(MARGIN, y, &song.title, TITLE, CHALK);
    canvas.write(x + MARGIN, y + (TITLE - INK) * GLYPH_H, &heading, INK, SMUDGE);
    y += HEAD;
    for ((pattern, bands), label) in sheets.iter().zip(&labels) {
        let x = canvas.write(MARGIN, y, &pattern.name, INK, CHALK);
        canvas.write(x + 12, y, label, INK, SMUDGE);
        y += LINE + 4;
        let (bars, grid) = (pattern.bars as usize, pattern.grid as usize);
        let span = bars * bar_px;
        let at = |tick: usize| left + tick * bar_px / grid;
        let beats: Vec<usize> = if grid.is_multiple_of(4) { (0..bars * 4).map(|b| b * grid / 4).filter(|t| !t.is_multiple_of(grid)).collect() } else { Vec::new() };
        for bar in 0..bars { canvas.write(at(bar * grid) + 6, y, &(bar + 1).to_string(), INK, CHALK); }
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
        for row in y..bottom { self.ink[(row * self.width + x) * 3..(row * self.width + right) * 3].chunks_exact_mut(3).for_each(|px| px.copy_from_slice(&rgb)); }
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
        let doc = include_str!("../docs/format.md");
        let (_, opened) = doc.split_once("```\n").expect("format.md lost its example");
        opened.split_once("```").expect("format.md example fence never closes").0
    }

    fn rolled(text: &str) -> Canvas {
        let song = Song::parse(text).unwrap_or_else(|errors| panic!("the test score doesn't parse: {errors:?}"));
        roll(&song).unwrap_or_else(|why| panic!("{why}"))
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
        assert!(roll(&song).is_err_and(|why| why.starts_with("the roll would be 25718x")));
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
