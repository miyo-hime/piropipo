use crate::listen::{self, Remark};
use crate::surface;
use piropipo::score::{Fault, Position, Song};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
#[command(after_help = "a sketch is a .piro grid whose lane notes leave out their positions. each lane starts at 1.1 and every note moves the flow on by its own length, so `E5 x2` then `- x4` land at 1.1 and 1.3. a positioned line like `2.5: C4 x2` anchors the flow there, forward only. words after the length pass through untouched, and every sketch line keeps its line number in the .piro, so `piro check` points straight back into the sketch.")]
pub struct Args {
    #[arg(help = "a sketch, or - for stdin")]
    pub input: String,
    #[arg(short, long, help = "where the .piro lands (default: beside the input, so song.sketch writes song.piro); - streams it to stdout and moves the report to stderr")]
    pub output: Option<PathBuf>,
}

const FLOW: &str = "a flow note is a pitch and a length, then any effects, like `E5 x2 vib 25ct 5hz` or `- x4` for a rest; an anchored note puts its place first, like `2.5: C4 x2`";

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> {
    let streaming = args.output.as_ref().is_some_and(|o| o.as_os_str() == "-");
    let say = |said: String| if streaming { eprintln!("{said}"); } else { println!("{said}"); };
    match draft(args) {
        Ok((dest, inked)) => {
            if streaming { print!("{}", inked.text); }
            let shown = dest.as_ref().map_or_else(|| "-".to_owned(), |p| p.display().to_string());
            say(if human { inked.tell(dest.as_deref()) } else { json!({ "ok": true, "verb": "sketch", "output": shown, "stats": inked.json(), "warnings": [], "errors": [] }).to_string() });
            Ok(())
        }
        Err(errors) => {
            if !human { say(json!({ "ok": false, "verb": "sketch", "output": null, "stats": null, "warnings": [], "errors": errors.iter().map(card).collect::<Vec<_>>() }).to_string()); }
            Err(match &errors[..] {
                [one] => one.to_string(),
                many => format!("{} errors\n{}", many.len(), many.iter().map(|e| format!("  {e}")).collect::<Vec<_>>().join("\n")),
            }.into())
        }
    }
}

fn draft(args: Args) -> Result<(Option<PathBuf>, Inked), Vec<Remark>> {
    let Args { input, output } = args;
    let origin = (input != "-").then(|| Path::new(&input));
    let dest = match output {
        Some(dash) if dash.as_os_str() == "-" => None,
        chosen => Some(surface::land_beside(origin, chosen, "piro").map_err(|(code, why)| vec![Remark::loose(code, if code == "no-output" { "stdin input needs -o: name the .piro it lands in, or -o - to stream it to stdout".to_owned() } else { why })])?),
    };
    let inked = ink(&listen::read(&input)?)?;
    if let Some(dest) = &dest { surface::publish(dest, inked.text.as_bytes()).map_err(|e| vec![Remark::loose("unwritable-output", format!("can't write {}: {e}", dest.display()))])?; }
    Ok((dest, inked))
}

struct Inked { text: String, voices: Vec<Voice> }

impl Inked {
    fn notes(&self) -> usize { self.voices.iter().map(|v| v.notes).sum() }

    fn json(&self) -> Value { json!({ "notes": self.notes(), "lanes": self.voices.iter().map(Voice::json).collect::<Vec<_>>() }) }

    fn tell(&self, dest: Option<&Path>) -> String {
        let head = dest.map_or("streamed to stdout".to_owned(), |p| format!("wrote {}", p.display()));
        let (notes, lanes) = (self.notes(), self.voices.len());
        let headline = format!("{head}: {notes} note{} placed in {lanes} lane{}", s(notes as u64), s(lanes as u64));
        std::iter::once(headline).chain(self.voices.iter().map(Voice::tell)).collect::<Vec<_>>().join("\n")
    }
}

struct Anchor { line: usize, at: Position, skipped: u64 }

struct Spill { line: usize, at: Position, note: String, len: u32, left: u64 }

#[derive(Default)]
struct Voice { pattern: String, lane: String, bars: u32, grid: u32, cursor: u64, reach: u64, last: Option<usize>, notes: usize, anchors: Vec<Anchor>, spill: Option<Spill>, snags: Vec<Remark>, widest: usize }

impl Voice {
    fn span(&self) -> u64 { u64::from(self.bars) * u64::from(self.grid) }

    fn spot(&self, tick: u64) -> Position { Position::from_tick(tick, self.grid) }

    fn through(&self) -> Option<Position> { self.reach.checked_sub(1).map(|t| self.spot(t)) }

    fn balk(&mut self, line: usize, at: Option<Position>, code: &'static str, message: String) -> Option<Position> {
        self.snags.push(Remark { code, message, pattern: Some(self.pattern.clone()), lane: Some(self.lane.clone()), at, line: Some(line) });
        None
    }

    fn fault(&mut self, line: usize, at: Option<Position>, fault: Fault) -> Option<Position> { self.balk(line, at, fault.code(), fault.to_string()) }

    fn place(&mut self, line: usize, code: &str) -> Option<Position> {
        let text = code.trim();
        let (spot, words) = match code.split_once(':') { Some((spot, words)) => (Some(spot.trim()), words), None => (None, code) };
        if spot.is_none() && text.starts_with(|c: char| c.is_ascii_digit()) { return self.balk(line, None, "no-colon", format!("`{text}` starts with a place but has no colon; an anchored note reads `bar.slot: PITCH xLEN`, like `2.5: C4 x2`, and a flow note leaves the place out, like `C4 x2`")); }
        let anchor = match spot.map(|spot| read_position(spot).ok_or(spot)) {
            None => None,
            Some(Err(spot)) => return self.fault(line, None, Fault::BadPosition(spot.into())),
            Some(Ok(at)) => match misplaced(at, self.bars, self.grid) { Some(fault) => return self.fault(line, Some(at), fault), None => Some(at) },
        };
        let mut said = words.split_whitespace();
        let (pitch, length) = match (said.next(), said.next()) {
            (Some(pitch), Some(length)) if read_length(pitch).is_none() => (pitch, length),
            (Some(pitch), None) if read_length(pitch).is_none() => return self.balk(line, anchor, "no-length", format!("`{text}` has no length after its pitch; {FLOW}")),
            _ => return self.balk(line, anchor, "no-pitch", format!("`{text}` has no pitch before its length; {FLOW}")),
        };
        let Some(len) = read_length(length) else { return self.balk(line, anchor, "bad-length", format!("`{length}` isn't a length; {FLOW}")) };
        let start = match anchor {
            None => self.cursor,
            Some(at) => {
                let tick = at.tick(self.grid);
                if tick < self.cursor { self.rewound(line, at, tick); } else { self.anchors.push(Anchor { line, at, skipped: tick - self.cursor }); }
                tick
            }
        };
        let (at, end) = (self.spot(start), start.saturating_add(u64::from(len)));
        if end > self.span() && self.spill.is_none() { self.spill = Some(Spill { line, at, note: format!("{pitch} {length}"), len, left: self.span().saturating_sub(start) }); }
        self.cursor = end;
        self.reach = self.reach.max(end);
        self.last = Some(line);
        self.notes += 1;
        self.widest = self.widest.max(at.to_string().len() + 1);
        Some(at)
    }

    fn rewound(&mut self, line: usize, at: Position, tick: u64) {
        let behind = self.cursor - tick;
        let (rung, free) = (self.spot(self.cursor - 1), self.spot(self.cursor));
        let held = self.last.map_or(String::new(), |l| format!(" on line {l}"));
        self.balk(line, Some(at), "anchor-rewinds", format!("anchor {at} would rewind this lane {behind} slot{}: the note{held} already rings through {rung}, so the next free slot is {free}. a lane only flows forward, so either the notes above run {behind} slot{} long or this anchor belongs at {free} or later", s(behind), s(behind)));
    }

    fn overflow(&self) -> Option<Remark> {
        let Spill { line, at, note, len, left } = self.spill.as_ref()?;
        let last = self.spot(self.span().saturating_sub(1));
        let measure = format!("{} bar{} of {} slots", self.bars, s(u64::from(self.bars)), self.grid);
        let room = match left {
            0 => format!("`{note}` starts with no slots left: the notes before it already reach the pattern's end after {last} ({measure})"),
            _ => format!("`{note}` needs {len} slot{} but only {left} {} left before the pattern ends after {last} ({measure})", s(u64::from(*len)), if *left == 1 { "is" } else { "are" }),
        };
        let over = self.reach.saturating_sub(self.span());
        let trail = if over > u64::from(*len).saturating_sub(*left) { format!("; counting the notes after it, the lane runs {over} slot{} past the end", s(over)) } else { String::new() };
        Some(Remark { code: "runs-past-end", message: format!("{room}{trail}"), pattern: Some(self.pattern.clone()), lane: Some(self.lane.clone()), at: Some(*at), line: Some(*line) })
    }

    fn json(&self) -> Value {
        let anchors: Vec<Value> = self.anchors.iter().map(|a| json!({ "line": a.line, "at": a.at.to_string(), "skipped_slots": a.skipped })).collect();
        json!({ "pattern": self.pattern, "lane": self.lane, "notes": self.notes, "through": self.through().map(|p| p.to_string()), "span_slots": self.span(), "trailing_slots": self.span().saturating_sub(self.reach), "anchors": anchors })
    }

    fn tell(&self) -> String {
        let (span, tail, notes) = (self.span(), self.span().saturating_sub(self.reach), self.notes);
        let fill = match self.through() {
            None => format!("no notes, all {span} slots silent"),
            Some(through) if tail == 0 => format!("{notes} note{} through {through}, the pattern's last slot", s(notes as u64)),
            Some(through) => format!("{notes} note{} through {through}, then {tail} of {span} slots unwritten to the end", s(notes as u64)),
        };
        let anchors: String = self.anchors.iter().map(|a| match a.skipped {
            0 => format!("\n  anchor {} (line {}) lands right where the flow was", a.at, a.line),
            k => format!("\n  anchor {} (line {}) skips {k} slot{} of silence", a.at, a.line, s(k)),
        }).collect();
        format!("pattern {}, lane {}: {fill}{anchors}", self.pattern, self.lane)
    }
}

#[derive(Default)]
enum Page { #[default] Blank, Setlist, Smudged, Ruled { name: String, bars: u32, grid: u32 } }

#[derive(Default, Clone, Copy)]
enum Pen { #[default] Lifted, Drawing(usize), Dry }

enum Stroke<'t> { Plain(&'t str), Placed { voice: usize, at: Position, body: &'t str } }

#[derive(Default)]
struct Inker<'t> { page: Page, pen: Pen, voices: Vec<Voice>, strokes: Vec<Stroke<'t>>, errors: Vec<Remark> }

fn ink(text: &str) -> Result<Inked, Vec<Remark>> {
    let mut inker = Inker::default();
    for (i, raw) in text.lines().enumerate() { inker.read(i + 1, raw); }
    inker.finish()
}

impl<'t> Inker<'t> {
    fn read(&mut self, line: usize, raw: &'t str) {
        let code = strip(raw);
        let mut words = code.split_whitespace();
        match (words.next(), self.pen) {
            (None | Some("song"), _) => {}
            (Some("pattern"), _) => self.pattern(line, raw),
            (Some("lane"), _) => self.lane(line, words.next()),
            (Some("play"), _) => { self.page = Page::Setlist; self.pen = Pen::Lifted; }
            (Some(_), Pen::Drawing(v)) => return self.note(v, line, raw, code),
            (Some(_), Pen::Dry) => {}
            (Some(_), Pen::Lifted) if matches!(self.page, Page::Setlist) => {}
            (Some(_), Pen::Lifted) => { self.errors.push(faulted(line, None, Fault::NoteOutsideLane)); self.pen = Pen::Dry; }
        }
        self.strokes.push(Stroke::Plain(raw));
    }

    fn pattern(&mut self, line: usize, raw: &str) {
        self.pen = Pen::Lifted;
        // check's own copyist reads the header, so sketch and check can never disagree about a span
        self.page = match Song::parse(&format!("song \"\" tempo 1\n{raw}\n")) {
            Ok(song) => {
                let ruled = song.patterns.into_iter().next().expect("a pattern line that parses is a pattern");
                Page::Ruled { name: ruled.name, bars: ruled.bars, grid: ruled.grid }
            }
            Err(faults) => { self.errors.extend(faults.into_iter().map(|e| Remark { line: Some(line), ..Remark::from(e) })); Page::Smudged }
        };
    }

    fn lane(&mut self, line: usize, name: Option<&str>) {
        self.pen = Pen::Dry;
        let fault = match (&self.page, name) {
            (_, None) => Fault::NoName("lane"),
            (Page::Smudged, _) => return,
            (Page::Blank | Page::Setlist, _) => Fault::LaneOutsidePattern,
            (Page::Ruled { name: pattern, bars, grid }, Some(lane)) => {
                self.voices.push(Voice { pattern: pattern.clone(), lane: lane.to_owned(), bars: *bars, grid: *grid, ..Voice::default() });
                self.pen = Pen::Drawing(self.voices.len() - 1);
                return;
            }
        };
        self.errors.push(Remark { lane: name.map(Into::into), ..faulted(line, None, fault) });
    }

    fn note(&mut self, v: usize, line: usize, raw: &'t str, code: &str) {
        let body = if code.contains(':') { raw.split_once(':').map_or("", |(_, body)| body) } else { raw }.trim();
        let stroke = match self.voices[v].place(line, code) { Some(at) => Stroke::Placed { voice: v, at, body }, None => Stroke::Plain(raw) };
        self.strokes.push(stroke);
    }

    fn finish(mut self) -> Result<Inked, Vec<Remark>> {
        for voice in &mut self.voices { self.errors.append(&mut voice.snags); self.errors.extend(voice.overflow()); }
        if !self.errors.is_empty() { self.errors.sort_by_key(|e| e.line); return Err(self.errors); }
        let text = self.strokes.iter().map(|stroke| match stroke {
            Stroke::Plain(raw) => format!("{}\n", raw.trim_end()),
            Stroke::Placed { voice, at, body } => format!("  {:<width$} {body}\n", format!("{at}:"), width = self.voices[*voice].widest),
        }).collect();
        Ok(Inked { text, voices: self.voices })
    }
}

fn faulted(line: usize, at: Option<Position>, fault: Fault) -> Remark { Remark { at, line: Some(line), ..Remark::loose(fault.code(), fault.to_string()) } }

fn card(remark: &Remark) -> Value { json!({ "code": remark.code, "message": remark.message, "pattern": remark.pattern, "lane": remark.lane, "at": remark.at.map(|at| at.to_string()), "line": remark.line }) }

fn s(n: u64) -> &'static str { if n == 1 { "" } else { "s" } }

fn misplaced(at: Position, bars: u32, grid: u32) -> Option<Fault> {
    let Position { bar, slot } = at;
    if bar == 0 { Some(Fault::BarZero) }
    else if slot == 0 { Some(Fault::SlotZero) }
    else if slot > grid { Some(Fault::SlotPastGrid { slot, grid, meant: Some(at.tick(grid)).filter(|t| *t < u64::from(bars) * u64::from(grid)).map(|t| Position::from_tick(t, grid)) }) }
    else if bar > bars { Some(Fault::BarPastEnd { bar, bars }) }
    else { None }
}

fn strip(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c { '"' => quoted = !quoted, ';' if !quoted => return &line[..i], _ => {} }
    }
    line
}

fn whole(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) { return None; }
    text.parse().ok()
}

fn read_position(spot: &str) -> Option<Position> {
    match spot.split_once('.') {
        Some((bar, slot)) => Some(Position { bar: whole(bar)?, slot: whole(slot)? }),
        None => Some(Position { bar: 1, slot: whole(spot)? }),
    }
}

fn read_length(word: &str) -> Option<u32> { word.strip_prefix('x').and_then(whole) }

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "song \"s\" tempo 120\npattern p bars 2 grid 4\nlane pulse1\n";

    fn inked(text: &str) -> Inked { ink(text).unwrap_or_else(|e| panic!("{e:?}")) }

    fn refusal(text: &str) -> Vec<Remark> {
        let Err(errors) = ink(text) else { panic!("expected sketch to refuse:\n{text}") };
        errors
    }

    fn lone(text: &str) -> Remark {
        let errors = refusal(text);
        let [one] = &errors[..] else { panic!("expected one error, got {errors:?}") };
        one.clone()
    }

    fn placed(text: &str) -> Vec<String> { inked(text).text.lines().skip(3).map(str::to_owned).collect() }

    fn pos(bar: u32, slot: u32) -> Position { Position { bar, slot } }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("piro-sketch-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn each_note_lands_where_the_last_one_let_go_barlines_and_all() {
        assert_eq!(placed(&format!("{HEAD}  C4 x3\n  D4 x3\n  - x1\n  E4 x1 vol 9\n")), ["  1.1: C4 x3", "  1.4: D4 x3", "  2.3: - x1", "  2.4: E4 x1 vol 9"]);
        assert_eq!(placed(&format!("{HEAD}  G2 x8\nlane noise\n  - x2\n  C4 x6\n")), ["  1.1: G2 x8", "lane noise", "  1.1: - x2", "  1.3: C4 x6"], "every lane starts its own flow at 1.1");
    }

    #[test]
    fn an_anchor_skips_silence_forward_or_confirms_the_flow() {
        let sketched = inked(&format!("{HEAD}  C4 x2\n  2.1: D4 x1\n  E4 x1\n  2.3: F4 x2\n"));
        assert_eq!(sketched.text.lines().skip(3).collect::<Vec<_>>(), ["  1.1: C4 x2", "  2.1: D4 x1", "  2.2: E4 x1", "  2.3: F4 x2"]);
        assert_eq!(sketched.voices[0].anchors.iter().map(|a| (a.line, a.at, a.skipped)).collect::<Vec<_>>(), [(5, pos(2, 1), 2), (7, pos(2, 3), 0)]);
        assert_eq!(sketched.voices[0].tell(), "pattern p, lane pulse1: 4 notes through 2.4, the pattern's last slot\n  anchor 2.1 (line 5) skips 2 slots of silence\n  anchor 2.3 (line 7) lands right where the flow was");
        assert_eq!(placed(&format!("{HEAD}  3: C4 x1\n  D4 x1\n")), ["  1.3: C4 x1", "  1.4: D4 x1"], "a bare-slot anchor means bar 1 and inks in full");
    }

    #[test]
    fn an_anchor_that_rewinds_is_refused_with_how_far_the_flow_ran() {
        let rewind = lone(&format!("{HEAD}  C4 x4\n  D4 x2\n  1.3: E4 x1\n  2.1: F4 x4\n"));
        assert_eq!((rewind.code, rewind.line, rewind.at, rewind.pattern.as_deref(), rewind.lane.as_deref()), ("anchor-rewinds", Some(6), Some(pos(1, 3)), Some("p"), Some("pulse1")), "the honest anchor after it raises nothing");
        assert_eq!(rewind.message, "anchor 1.3 would rewind this lane 4 slots: the note on line 5 already rings through 2.2, so the next free slot is 2.3. a lane only flows forward, so either the notes above run 4 slots long or this anchor belongs at 2.3 or later");
        let gap = lone(&format!("{HEAD}  C4 x1\n  2.1: D4 x1\n  1.3: E4 x1\n"));
        assert_eq!((gap.code, gap.at), ("anchor-rewinds", Some(pos(1, 3))), "a rewind is refused even into silence nobody wrote");
    }

    #[test]
    fn an_overflowing_lane_names_the_note_and_the_room_it_had() {
        let over = lone(&format!("{HEAD}  C4 x6\n  D4 x4\n"));
        assert_eq!((over.code, over.line, over.at, over.pattern.as_deref(), over.lane.as_deref()), ("runs-past-end", Some(5), Some(pos(2, 3)), Some("p"), Some("pulse1")));
        assert_eq!(over.message, "`D4 x4` needs 4 slots but only 2 are left before the pattern ends after 2.4 (2 bars of 4 slots)");
        assert_eq!(lone(&format!("{HEAD}  C4 x7\n  D4 x2\n")).message, "`D4 x2` needs 2 slots but only 1 is left before the pattern ends after 2.4 (2 bars of 4 slots)");
        let full = lone(&format!("{HEAD}  C4 x8\n  D4 x1 vol 3\n"));
        assert_eq!((full.at, full.message.as_str()), (Some(pos(3, 1)), "`D4 x1` starts with no slots left: the notes before it already reach the pattern's end after 2.4 (2 bars of 4 slots)"));
        let spilled = lone(&format!("{HEAD}  C4 x6\n  D4 x4\n  E4 x2\n  F4 x1\n"));
        assert_eq!((spilled.line, spilled.message.as_str()), (Some(5), "`D4 x4` needs 4 slots but only 2 are left before the pattern ends after 2.4 (2 bars of 4 slots); counting the notes after it, the lane runs 5 slots past the end"), "one error per lane, with the whole overshoot");
        assert_eq!(lone(&format!("{HEAD}  C4 x2\n  2.3: D4 x4\n")).message, "`D4 x4` needs 4 slots but only 2 are left before the pattern ends after 2.4 (2 bars of 4 slots)", "an anchored note answers to the same span");
    }

    #[test]
    fn trailing_silence_is_legal_and_counted() {
        let sketched = inked(&format!("{HEAD}  C4 x3\nlane noise\n"));
        let lanes = &sketched.json()["lanes"];
        assert_eq!(lanes[0], json!({ "pattern": "p", "lane": "pulse1", "notes": 1, "through": "1.3", "span_slots": 8, "trailing_slots": 5, "anchors": [] }));
        assert_eq!((&lanes[1]["through"], &lanes[1]["trailing_slots"]), (&Value::Null, &json!(8)));
        assert_eq!(sketched.tell(Some(Path::new("song.piro"))), "wrote song.piro: 1 note placed in 2 lanes\npattern p, lane pulse1: 1 note through 1.3, then 5 of 8 slots unwritten to the end\npattern p, lane noise: no notes, all 8 slots silent");
    }

    #[test]
    fn the_span_counts_slots_whatever_the_meter() {
        let seven = "song \"s\" tempo 120\npattern p bars 2 beats 7 grid 14\nlane pulse1\n";
        assert_eq!(placed(&format!("{seven}  C4 x20\n  D4 x8\n")), ["  1.1: C4 x20", "  2.7: D4 x8"]);
        assert!(lone(&format!("{seven}  C4 x29\n")).message.starts_with("`C4 x29` needs 29 slots but only 28 are left before the pattern ends after 2.14"));
    }

    #[test]
    fn comments_blank_lines_and_line_numbers_survive_the_inking() {
        let sketch = "; a jingle\nsong \"a; b\" tempo 90 ; trailing\n\npattern a bars 1 grid 16\nlane pulse1   ; lead\n\t; intro\n  C4 x2 ; first\n\n  D4 x10     ; held\n  1.13:E4 x4\n";
        let out = inked(sketch).text;
        assert_eq!(out, "; a jingle\nsong \"a; b\" tempo 90 ; trailing\n\npattern a bars 1 grid 16\nlane pulse1   ; lead\n\t; intro\n  1.1:  C4 x2 ; first\n\n  1.3:  D4 x10     ; held\n  1.13: E4 x4\n");
        let lines: Vec<usize> = Song::parse(&out).unwrap().patterns[0].lanes[0].notes.iter().map(|n| n.line).collect();
        assert_eq!(lines, [7, 9, 10], "check's line numbers point straight back into the sketch");
        assert_eq!(placed(&format!("{HEAD}  C4 x1 ; a colon: in a comment is still a comment\n")), ["  1.1: C4 x1 ; a colon: in a comment is still a comment"]);
    }

    #[test]
    fn words_after_the_length_pass_through_and_check_stays_the_judge() {
        let out = inked(&format!("{HEAD}  H9 x2 wobble 99\n  C4 x2 vib 999ct 1000hz\n")).text;
        assert!(out.ends_with("  1.1: H9 x2 wobble 99\n  1.3: C4 x2 vib 999ct 1000hz\n"), "{out}");
        let judged: Vec<(Option<usize>, &str)> = Song::parse(&out).unwrap_err().iter().map(|e| (e.line, e.fault.code())).collect();
        assert_eq!(judged, [(Some(4), "bad-pitch"), (Some(4), "leftover"), (Some(4), "leftover"), (Some(5), "vibrato-rate")]);
    }

    #[test]
    fn a_malformed_line_teaches_the_flow_shape() {
        let errors = refusal(&format!("{HEAD}  C4\n  C4 2\n  x4 vol 3\n  2 C4 x2\n  2.x: C4 x2\n  2.1: C4\n  D4 x8\n"));
        assert_eq!(errors.iter().map(|e| (e.code, e.line)).collect::<Vec<_>>(), [("no-length", Some(4)), ("bad-length", Some(5)), ("no-pitch", Some(6)), ("no-colon", Some(7)), ("bad-position", Some(8)), ("no-length", Some(9))], "broken lines never move the flow, so D4 x8 still fits");
        assert_eq!(errors[1].message, "`2` isn't a length; a flow note is a pitch and a length, then any effects, like `E5 x2 vib 25ct 5hz` or `- x4` for a rest; an anchored note puts its place first, like `2.5: C4 x2`");
        assert_eq!(errors[3].message, "`2 C4 x2` starts with a place but has no colon; an anchored note reads `bar.slot: PITCH xLEN`, like `2.5: C4 x2`, and a flow note leaves the place out, like `C4 x2`");
    }

    #[test]
    fn a_misplaced_anchor_is_refused_in_check_s_own_words() {
        for spot in ["0.1", "1.0", "1.5", "2.9", "3.1"] {
            let text = format!("{HEAD}  {spot}: C4 x1\n");
            let sketched = lone(&text);
            let checked = Song::parse(&text).unwrap_err();
            assert_eq!((sketched.code, sketched.message.as_str(), sketched.line), (checked[0].fault.code(), checked[0].fault.to_string().as_str(), Some(4)), "{spot}");
        }
        assert!(lone(&format!("{HEAD}  1.5: C4 x1\n")).message.ends_with("slot 5 lands on 2.1"));
    }

    #[test]
    fn notes_need_a_lane_and_lanes_need_a_readable_pattern() {
        let refused = |text: &str| refusal(text).iter().map(|e| (e.code, e.line)).collect::<Vec<_>>();
        assert_eq!(refused("song \"s\" tempo 120\npattern p bars 1 grid 4\n  C4 x4\n  D4 x4\nlane pulse1\n  C4 x4\n"), [("note-outside-lane", Some(3))]);
        assert_eq!(refused("song \"s\" tempo 120\nlane pulse1\n  C4 x4\npattern p bars 1 grid 4\n"), [("lane-outside-pattern", Some(2))]);
        assert_eq!(refused("song \"s\" tempo 120\npattern p bars 1 grid 4\nplay\n  p\nlane pulse1\n  C4 x4\n"), [("lane-outside-pattern", Some(5))]);
        assert_eq!(refused("song \"s\" tempo 120\npattern p bars 1 grid 4\nlane\n  C4 x4\n"), [("no-name", Some(3))]);
        let smudged = refusal("song \"s\" tempo 120\npattern p bars 1\nlane pulse1\n  C4 x99\n");
        assert_eq!(smudged.iter().map(|e| (e.code, e.line, e.pattern.as_deref())).collect::<Vec<_>>(), [("missing-param", Some(2), Some("p"))], "check's own verdict on the header, and no cascade from the notes under it");
        assert_eq!(smudged[0].message, "`pattern` is missing `grid N` (how many slots a bar divides into)");
    }

    #[test]
    fn the_play_block_passes_through_untouched() {
        let sketch = "song \"s\" tempo 120\nplay\n  a\n  a +5 ; up a fourth\npattern a bars 1 grid 4\nlane pulse1\n  C4 x4\n";
        assert_eq!(inked(sketch).text, "song \"s\" tempo 120\nplay\n  a\n  a +5 ; up a fourth\npattern a bars 1 grid 4\nlane pulse1\n  1.1: C4 x4\n");
    }

    #[test]
    fn a_vast_pattern_costs_nothing_to_sketch() {
        let vast = "song \"s\" tempo 120\npattern endless bars 4000000000 grid 255\nlane pulse1\n  C4 x1\n  4000000000.250: D4 x5\n";
        let sketched = inked(vast);
        assert_eq!(sketched.text.lines().skip(3).collect::<Vec<_>>(), [format!("  {:<15} C4 x1", "1.1:"), "  4000000000.250: D4 x5".to_owned()]);
        let lane = &sketched.voices[0];
        assert_eq!((lane.anchors[0].skipped, lane.through(), lane.span() - lane.reach), (3_999_999_999 * 255 + 248, Some(pos(4_000_000_000, 254)), 1));
        let over = lone(&format!("{vast}  E4 x2\n"));
        assert_eq!((over.at, over.message.as_str()), (Some(pos(4_000_000_000, 255)), "`E4 x2` needs 2 slots but only 1 is left before the pattern ends after 4000000000.255 (4000000000 bars of 255 slots)"));
    }

    #[test]
    fn honey_without_its_positions_inks_back_into_the_same_song() {
        let score = include_str!("../docs/honey-in-the-wine.piro");
        let kept = [182, 398];
        let sketch: String = score.lines().enumerate().map(|(i, line)| match line.split_once(':') {
            Some((spot, note)) if read_position(spot.trim()).is_some() && !kept.contains(&(i + 1)) => format!("  {}\n", note.trim()),
            _ => format!("{line}\n"),
        }).collect();
        assert_eq!(sketch.matches(':').count(), kept.len(), "every other position was stripped");
        let sketched = inked(&sketch);
        assert_eq!(Song::parse(&sketched.text).unwrap(), Song::parse(score).unwrap(), "same notes, same places, same line numbers, so the same wav");
        assert_eq!(sketched.voices.iter().flat_map(|v| &v.anchors).map(|a| (a.line, a.skipped)).collect::<Vec<_>>(), [(182, 0), (398, 0)]);
    }

    #[test]
    fn stdin_without_o_is_refused_before_reading_anything() {
        let Err(errors) = draft(Args { input: "-".into(), output: None }) else { panic!("stdin with no -o should refuse") };
        assert_eq!(errors.iter().map(|e| e.code).collect::<Vec<_>>(), ["no-output"]);
        assert!(errors[0].message.ends_with("or -o - to stream it to stdout"), "{}", errors[0].message);
    }

    #[test]
    fn the_score_lands_beside_the_sketch_whole() {
        let dir = scratch("beside");
        let sketch = dir.join("ding.sketch");
        std::fs::write(&sketch, format!("{HEAD}  C4 x8\n")).unwrap();
        let (output, sketched) = draft(Args { input: sketch.to_string_lossy().into(), output: None }).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(output.as_deref(), Some(dir.join("ding.piro").as_path()));
        assert_eq!(std::fs::read_to_string(dir.join("ding.piro")).unwrap(), sketched.text);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "a draft was left behind");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dash_o_streams_and_lands_nothing_beside() {
        let dir = scratch("stream");
        let sketch = dir.join("ding.sketch");
        std::fs::write(&sketch, format!("{HEAD}  C4 x8\n")).unwrap();
        let (output, _) = draft(Args { input: sketch.to_string_lossy().into(), output: Some("-".into()) }).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(output, None);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_broken_sketch_never_touches_the_last_good_score() {
        let dir = scratch("broken");
        let sketch = dir.join("ding.sketch");
        std::fs::write(dir.join("ding.piro"), "the last good score").unwrap();
        std::fs::write(&sketch, format!("{HEAD}  C4 x9\n")).unwrap();
        let Err(errors) = draft(Args { input: sketch.to_string_lossy().into(), output: None }) else { panic!("x9 overflows an 8-slot pattern") };
        assert_eq!(errors[0].code, "runs-past-end");
        assert_eq!(std::fs::read_to_string(dir.join("ding.piro")).unwrap(), "the last good score");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_sketch_named_like_its_score_is_never_overwritten() {
        let dir = scratch("self");
        let sketch = dir.join("ding.piro");
        std::fs::write(&sketch, format!("{HEAD}  C4 x8\n")).unwrap();
        let Err(errors) = draft(Args { input: sketch.to_string_lossy().into(), output: None }) else { panic!("the default .piro is the input itself") };
        assert_eq!(errors[0].code, "output-eats-input");
        assert_eq!(std::fs::read_to_string(&sketch).unwrap(), format!("{HEAD}  C4 x8\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
