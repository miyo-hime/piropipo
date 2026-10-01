use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position { pub bar: u32, pub slot: u32 }

impl Position {
    pub fn tick(self, grid: u32) -> u64 { u64::from(self.bar.saturating_sub(1)) * u64::from(grid) + u64::from(self.slot.saturating_sub(1)) }

    pub fn from_tick(tick: u64, grid: u32) -> Self {
        let grid = u64::from(grid);
        let fit = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
        Position { bar: fit(tick / grid + 1), slot: fit(tick % grid + 1) }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}.{}", self.bar, self.slot) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note { pub at: Position, pub pitch: Option<u8>, pub len: u32, pub line: usize }

impl Note {
    pub fn rings_through(&self, grid: u32) -> Position { Position::from_tick((self.at.tick(grid) + u64::from(self.len)).saturating_sub(1), grid) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane { pub name: String, pub params: Vec<(String, String)>, pub notes: Vec<Note>, pub line: usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern { pub name: String, pub bars: u32, pub grid: u32, pub lanes: Vec<Lane>, pub line: usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song { pub title: String, pub tempo: u32, pub profile: Option<String>, pub patterns: Vec<Pattern> }

impl Song {
    pub fn parse(text: &str) -> Result<Song, Vec<ScoreError>> {
        let mut copyist = Copyist::default();
        for (i, raw) in text.lines().enumerate() { copyist.read(i + 1, raw); }
        copyist.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreError { pub line: Option<usize>, pub pattern: Option<String>, pub lane: Option<String>, pub at: Option<Position>, pub fault: Fault }

impl fmt::Display for ScoreError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut place = Vec::new();
        if let Some(n) = self.line { place.push(format!("line {n}")); }
        if let Some(p) = &self.pattern { place.push(format!("pattern {p}")); }
        if let Some(l) = &self.lane { place.push(format!("lane {l}")); }
        if let Some(at) = self.at { place.push(format!("at {at}")); }
        if place.is_empty() { write!(f, "{}", self.fault) } else { write!(f, "{}: {}", place.join(", "), self.fault) }
    }
}

impl std::error::Error for ScoreError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Unreadable(String),
    NoSong,
    SecondSong { first: usize },
    NoTitle,
    UnclosedTitle,
    NoName(&'static str),
    MissingParam { directive: &'static str, key: &'static str },
    UnknownParam { directive: &'static str, key: String, takes: &'static [&'static str] },
    RepeatedParam(String),
    NoValue(String),
    NotCount { key: String, value: String },
    SecondPattern { first: usize },
    LaneOutsidePattern,
    NoteOutsideLane,
    SecondLane { first: usize },
    NoColon(String),
    BadPosition(String),
    NoPitch,
    BadPitch(String),
    NoLength,
    BadLength(String),
    Silent,
    Leftover(String),
    BarZero,
    SlotZero,
    BarPastEnd { bar: u32, bars: u32 },
    SlotPastGrid { slot: u32, grid: u32, meant: Option<Position> },
    RunsPastEnd { through: Position, last: Position, over: u64 },
    Overlap { with: Position, line: usize, through: Position },
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = |n: u64| if n == 1 { "" } else { "s" };
        match self {
            Fault::Unreadable(text) => write!(f, "can't read `{text}`; a line is a `song`, `pattern`, or `lane` directive, or a note like `1.5: C4 x2`"),
            Fault::NoSong => write!(f, "no `song` line; every file declares one, like `song \"first light\" tempo 112`"),
            Fault::SecondSong { first } => write!(f, "a second `song` line; the song was already declared on line {first}"),
            Fault::NoTitle => write!(f, "the song needs a quoted title right after `song`, like `song \"first light\"`"),
            Fault::UnclosedTitle => write!(f, "the song title's opening quote never closes"),
            Fault::NoName(directive) => write!(f, "`{directive}` needs a name first, like `{}`", if *directive == "lane" { "lane pulse1" } else { "pattern main bars 2 grid 16" }),
            Fault::MissingParam { directive, key } => write!(f, "`{directive}` is missing `{key} N` ({})", gloss(key)),
            Fault::UnknownParam { directive, key, takes } => write!(f, "`{directive}` doesn't take `{key}`; it takes {}", takes.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(" and ")),
            Fault::RepeatedParam(key) => write!(f, "`{key}` is set twice on this line"),
            Fault::NoValue(key) => write!(f, "`{key}` has no value after it"),
            Fault::NotCount { key, value } => write!(f, "`{key}` wants a whole number above 0, got `{value}`"),
            Fault::SecondPattern { first } => write!(f, "a pattern with this name already exists on line {first}; pattern names are unique"),
            Fault::LaneOutsidePattern => write!(f, "a lane has to sit inside a pattern; put a `pattern` line above it"),
            Fault::NoteOutsideLane => write!(f, "this note isn't inside a lane; put a `lane` line above it"),
            Fault::SecondLane { first } => write!(f, "this lane already appears in this pattern on line {first}; a lane appears once per pattern, so merge the notes into that block"),
            Fault::NoColon(text) => write!(f, "`{text}` has no colon after its position; a note reads `bar.slot: PITCH xLEN`, like `2.9: A5 x4`"),
            Fault::BadPosition(spot) => write!(f, "`{spot}` isn't a position; write `bar.slot` as two whole numbers, like `2.9`, or a bare slot for bar 1"),
            Fault::NoPitch => write!(f, "missing a pitch before the length; write a note like `C#4`, or `-` for a rest"),
            Fault::BadPitch(word) => write!(f, "`{word}` isn't a pitch; write a capital letter A-G, an optional `#` or `b`, then an octave 0-9, like `C#4` or `Db4`, or `-` for a rest"),
            Fault::NoLength => write!(f, "missing a length; add how many slots the note rings, like `x4`"),
            Fault::BadLength(word) => write!(f, "`{word}` isn't a length; write `x` and a slot count, like `x4`"),
            Fault::Silent => write!(f, "`x0` rings for no slots at all; the shortest note is `x1`"),
            Fault::Leftover(text) => write!(f, "unexpected `{text}` after the length; a note line is just `bar.slot: PITCH xLEN`"),
            Fault::BarZero => write!(f, "there is no bar 0; bars count from 1"),
            Fault::SlotZero => write!(f, "there is no slot 0; slots count from 1"),
            Fault::BarPastEnd { bar, bars } => write!(f, "bar {bar} is past the end; this pattern has {bars} bar{}", s(u64::from(*bars))),
            Fault::SlotPastGrid { slot, grid, meant } => {
                write!(f, "slot {slot} doesn't exist; this pattern's grid is {grid} slots per bar")?;
                match meant { Some(m) => write!(f, ". counting on from the bar's start, slot {slot} lands on {m}"), None => Ok(()) }
            }
            Fault::RunsPastEnd { through, last, over } => write!(f, "rings through {through}, {over} slot{} past the pattern's last slot {last}", s(*over)),
            Fault::Overlap { with, line, through } => write!(f, "overlaps the note at {with} (line {line}), which rings through {through}; a lane plays one note at a time"),
        }
    }
}

fn gloss(key: &str) -> &'static str {
    match key { "tempo" => "beats per minute", "bars" => "how many bars the pattern lasts", "grid" => "how many slots a bar divides into", _ => "a whole number" }
}

#[derive(Default)]
enum Pen { #[default] Lifted, Writing, Dry }

struct Head { line: usize, title: String, tempo: u32, profile: Option<String> }

struct Sheet { pattern: Pattern, sound: bool }

#[derive(Default)]
struct Copyist { head: Option<Head>, sheets: Vec<Sheet>, pen: Pen, pattern: Option<String>, lane: Option<String>, errors: Vec<ScoreError> }

impl Copyist {
    fn blame(&mut self, line: usize, at: Option<Position>, fault: Fault) {
        self.errors.push(ScoreError { line: Some(line), pattern: self.pattern.clone(), lane: self.lane.clone(), at, fault });
    }

    fn blame_song(&mut self, line: usize, fault: Fault) {
        self.errors.push(ScoreError { line: Some(line), pattern: None, lane: None, at: None, fault });
    }

    fn read(&mut self, line: usize, raw: &str) {
        let text = strip(raw).trim();
        let Some(first) = text.split_whitespace().next() else { return };
        let rest = text[first.len()..].trim_start();
        match first {
            "song" => self.song(line, rest),
            "pattern" => self.pattern(line, rest),
            "lane" => self.lane(line, rest),
            _ if text.contains(':') || first.starts_with(|c: char| c.is_ascii_digit()) => self.note(line, text),
            _ => self.blame(line, None, Fault::Unreadable(text.into())),
        }
    }

    fn song(&mut self, line: usize, rest: &str) {
        if let Some(head) = &self.head { let first = head.line; return self.blame_song(line, Fault::SecondSong { first }); }
        let (title, tail) = match rest.strip_prefix('"') {
            Some(quoted) => match quoted.split_once('"') {
                Some((title, tail)) => (title.to_owned(), tail),
                None => return self.blame_song(line, Fault::UnclosedTitle),
            },
            None => { self.blame_song(line, Fault::NoTitle); (String::new(), rest) }
        };
        let tokens: Vec<&str> = tail.split_whitespace().collect();
        let mut faults = Vec::new();
        let [tempo, profile] = fields("song", &tokens, &["tempo", "profile"], &mut faults);
        let tempo = required("song", "tempo", tempo, &mut faults);
        for fault in faults { self.blame_song(line, fault); }
        self.head = Some(Head { line, title, tempo: tempo.unwrap_or(0), profile: profile.map(Into::into) });
    }

    fn pattern(&mut self, line: usize, rest: &str) {
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let (name, tokens) = match tokens.split_first() {
            Some((name, tail)) if !matches!(*name, "bars" | "grid") => (*name, tail),
            _ => ("", &tokens[..]),
        };
        self.pattern = (!name.is_empty()).then(|| name.to_owned());
        self.lane = None;
        self.pen = Pen::Lifted;
        let mut faults = Vec::new();
        if name.is_empty() { faults.push(Fault::NoName("pattern")); }
        else if let Some(twin) = self.sheets.iter().find(|s| s.pattern.name == name) { faults.push(Fault::SecondPattern { first: twin.pattern.line }); }
        let [bars, grid] = fields("pattern", tokens, &["bars", "grid"], &mut faults);
        let bars = required("pattern", "bars", bars, &mut faults);
        let grid = required("pattern", "grid", grid, &mut faults);
        for fault in faults { self.blame(line, None, fault); }
        let sound = bars.is_some() && grid.is_some();
        self.sheets.push(Sheet { pattern: Pattern { name: name.into(), bars: bars.unwrap_or(0), grid: grid.unwrap_or(0), lanes: Vec::new(), line }, sound });
    }

    fn lane(&mut self, line: usize, rest: &str) {
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        self.lane = tokens.first().map(|n| (*n).to_owned());
        self.pen = Pen::Dry;
        let Some((name, rest)) = tokens.split_first() else { return self.blame(line, None, Fault::NoName("lane")) };
        let Some(sheet) = self.sheets.last() else { return self.blame(line, None, Fault::LaneOutsidePattern) };
        if let Some(twin) = sheet.pattern.lanes.iter().find(|l| l.name == *name) { let first = twin.line; return self.blame(line, None, Fault::SecondLane { first }); }
        let mut params = Vec::new();
        for pair in rest.chunks(2) {
            match pair {
                [key, value] => params.push(((*key).to_owned(), (*value).to_owned())),
                _ => self.blame(line, None, Fault::NoValue(pair[0].into())),
            }
        }
        if let Some(sheet) = self.sheets.last_mut() { sheet.pattern.lanes.push(Lane { name: (*name).into(), params, notes: Vec::new(), line }); }
        self.pen = Pen::Writing;
    }

    fn note(&mut self, line: usize, text: &str) {
        if let Pen::Lifted = self.pen { self.blame(line, None, Fault::NoteOutsideLane); self.pen = Pen::Dry; }
        let Some((spot, body)) = text.split_once(':') else { return self.blame(line, None, Fault::NoColon(text.into())) };
        let spot = spot.trim();
        let Some(at) = read_position(spot) else { return self.blame(line, None, Fault::BadPosition(spot.into())) };
        let mut words = body.split_whitespace();
        let mut faults = Vec::new();
        let (pitch, len) = match words.next() {
            None => { faults.push(Fault::NoPitch); (None, None) }
            Some(word) if read_length(word).is_some() => { faults.push(Fault::NoPitch); (None, read_length(word)) }
            Some(word) => {
                let pitch = if word == "-" { Some(None) } else { read_pitch(word).map(Some) };
                if pitch.is_none() { faults.push(Fault::BadPitch(word.into())); }
                let len = match words.next() {
                    None => { faults.push(Fault::NoLength); None }
                    Some(word) => read_length(word).or_else(|| { faults.push(Fault::BadLength(word.into())); None }),
                };
                (pitch, len)
            }
        };
        if len == Some(0) { faults.push(Fault::Silent); }
        let leftover: Vec<&str> = words.collect();
        if !leftover.is_empty() { faults.push(Fault::Leftover(leftover.join(" "))); }
        let clean = faults.is_empty();
        for fault in faults { self.blame(line, Some(at), fault); }
        if let (true, Pen::Writing, Some(pitch), Some(len)) = (clean, &self.pen, pitch, len)
            && let Some(lane) = self.sheets.last_mut().and_then(|s| s.pattern.lanes.last_mut())
        {
            lane.notes.push(Note { at, pitch, len, line });
        }
    }

    fn finish(mut self) -> Result<Song, Vec<ScoreError>> {
        for sheet in self.sheets.iter().filter(|s| s.sound) { lint(&sheet.pattern, &mut self.errors); }
        if self.head.is_none() { self.errors.push(ScoreError { line: None, pattern: None, lane: None, at: None, fault: Fault::NoSong }); }
        self.errors.sort_by_key(|e| e.line);
        match self.head {
            Some(head) if self.errors.is_empty() => Ok(Song { title: head.title, tempo: head.tempo, profile: head.profile, patterns: self.sheets.into_iter().map(|s| s.pattern).collect() }),
            _ => Err(self.errors),
        }
    }
}

fn lint(pattern: &Pattern, errors: &mut Vec<ScoreError>) {
    let Pattern { bars, grid, .. } = *pattern;
    let span = u64::from(bars) * u64::from(grid);
    for lane in &pattern.lanes {
        let blame = |note: &Note, fault| ScoreError { line: Some(note.line), pattern: Some(pattern.name.clone()), lane: Some(lane.name.clone()), at: Some(note.at), fault };
        let mut placed = Vec::new();
        for note in &lane.notes {
            let Position { bar, slot } = note.at;
            let misplaced = if bar == 0 { Some(Fault::BarZero) }
                else if slot == 0 { Some(Fault::SlotZero) }
                else if slot > grid { Some(Fault::SlotPastGrid { slot, grid, meant: Some(note.at.tick(grid)).filter(|t| *t < span).map(|t| Position::from_tick(t, grid)) }) }
                else if bar > bars { Some(Fault::BarPastEnd { bar, bars }) }
                else { None };
            if let Some(fault) = misplaced { errors.push(blame(note, fault)); continue; }
            let over = (note.at.tick(grid) + u64::from(note.len)).saturating_sub(span);
            if over > 0 { errors.push(blame(note, Fault::RunsPastEnd { through: note.rings_through(grid), last: Position { bar: bars, slot: grid }, over })); }
            placed.push(note);
        }
        placed.sort_by_key(|n| n.at);
        let mut ringing: Option<&Note> = None;
        for note in placed {
            let tail = |n: &Note| n.at.tick(grid) + u64::from(n.len);
            if let Some(held) = ringing && note.at.tick(grid) < tail(held) { errors.push(blame(note, Fault::Overlap { with: held.at, line: held.line, through: held.rings_through(grid) })); }
            if ringing.is_none_or(|held| tail(note) > tail(held)) { ringing = Some(note); }
        }
    }
}

fn fields<'t, const N: usize>(directive: &'static str, tokens: &[&'t str], keys: &'static [&'static str; N], faults: &mut Vec<Fault>) -> [Option<&'t str>; N] {
    let mut found = [None; N];
    for pair in tokens.chunks(2) {
        let [key, value] = pair else { faults.push(Fault::NoValue(pair[0].into())); continue };
        match keys.iter().position(|k| k == key) {
            Some(i) if found[i].is_some() => faults.push(Fault::RepeatedParam((*key).into())),
            Some(i) => found[i] = Some(*value),
            None => faults.push(Fault::UnknownParam { directive, key: (*key).into(), takes: keys }),
        }
    }
    found
}

fn required(directive: &'static str, key: &'static str, value: Option<&str>, faults: &mut Vec<Fault>) -> Option<u32> {
    let Some(value) = value else { faults.push(Fault::MissingParam { directive, key }); return None };
    let count = whole(value).filter(|n| *n > 0);
    if count.is_none() { faults.push(Fault::NotCount { key: key.into(), value: value.into() }); }
    count
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

fn read_pitch(word: &str) -> Option<u8> {
    let mut chars = word.chars();
    let class: i16 = match chars.next()? { 'C' => 0, 'D' => 2, 'E' => 4, 'F' => 5, 'G' => 7, 'A' => 9, 'B' => 11, _ => return None };
    let rest = chars.as_str();
    let (shift, octave) = match (rest.strip_prefix('#'), rest.strip_prefix('b')) {
        (Some(octave), _) => (1, octave),
        (_, Some(octave)) => (-1, octave),
        _ => (0, rest),
    };
    let &[digit] = octave.as_bytes() else { return None };
    if !digit.is_ascii_digit() { return None; }
    u8::try_from(12 * (i16::from(digit - b'0') + 1) + class + shift).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_example() -> &'static str {
        let doc = include_str!("../docs/format.md");
        let (_, opened) = doc.split_once("```\n").expect("format.md lost its example");
        opened.split_once("```").expect("format.md example fence never closes").0
    }

    fn faults(text: &str) -> Vec<Fault> { Song::parse(text).expect_err("expected the copyist to object").into_iter().map(|e| e.fault).collect() }

    fn pos(bar: u32, slot: u32) -> Position { Position { bar, slot } }

    const TWO_BARS: &str = "song \"t\" tempo 120\npattern main bars 2 grid 16\nlane triangle\n";

    #[test]
    fn spec_example_parses_verbatim() {
        let song = Song::parse(spec_example()).unwrap_or_else(|errors| panic!("the spec's own example fails:\n{}", errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")));
        assert_eq!((song.title.as_str(), song.tempo, song.profile.as_deref()), ("first light", 112, Some("nes")));
        let [main] = &song.patterns[..] else { panic!("expected one pattern") };
        assert_eq!((main.name.as_str(), main.bars, main.grid), ("main", 2, 16));
        let names: Vec<&str> = main.lanes.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["pulse1", "pulse2", "triangle"]);
        assert_eq!(main.lanes[0].params, [("duty".to_owned(), "25".to_owned())]);
        assert!(main.lanes[2].params.is_empty());
        assert_eq!(main.lanes.iter().map(|l| l.notes.len()).collect::<Vec<_>>(), [8, 3, 3]);
        let rest = &main.lanes[0].notes[5];
        assert_eq!((rest.at, rest.pitch, rest.len), (pos(1, 13), None, 4));
        assert_eq!(main.lanes[0].notes[0].pitch, Some(76));
        let bass = &main.lanes[2].notes[1];
        assert_eq!((bass.at, bass.pitch, bass.rings_through(16)), (pos(1, 9), Some(45), pos(2, 8)));
        assert_eq!(main.lanes[2].notes[2].rings_through(16), pos(2, 16));
    }

    #[test]
    fn positions_are_two_integers_not_a_decimal() {
        assert_eq!(read_position("2.1"), Some(pos(2, 1)));
        assert_eq!(read_position("2.10"), Some(pos(2, 10)));
        assert_ne!(read_position("2.1"), read_position("2.10"));
        assert!(read_position("1.5") < read_position("1.10"));
        assert!(read_position("1.16") < read_position("2.1"));
        for junk in ["1.", ".5", "1.5.2", "+1.5", "1.-5", "1,5", "", "1.5e0"] { assert_eq!(read_position(junk), None, "{junk:?} should not read"); }
    }

    #[test]
    fn bare_slot_means_bar_one() {
        let song = Song::parse("song \"jingle\" tempo 150\npattern ding bars 1 grid 8\nlane pulse1\n  1: C5 x2\n  5: G5 x4\n").unwrap();
        let notes = &song.patterns[0].lanes[0].notes;
        assert_eq!(notes.iter().map(|n| n.at).collect::<Vec<_>>(), [pos(1, 1), pos(1, 5)]);
        assert_eq!(notes[1].rings_through(8), pos(1, 8));
    }

    #[test]
    fn notes_cross_barlines_freely() {
        let song = Song::parse(&format!("{TWO_BARS}  1.9: G2 x16\n")).unwrap();
        assert_eq!(song.patterns[0].lanes[0].notes[0].rings_through(16), pos(2, 8));
    }

    #[test]
    fn seams_touch_without_overlapping() {
        assert!(Song::parse(&format!("{TWO_BARS}  1.1: C3 x8\n  1.9: G2 x16\n  2.9: - x8\n")).is_ok());
    }

    #[test]
    fn overlap_is_caught_even_past_a_short_neighbour() {
        let errors = Song::parse(&format!("{TWO_BARS}  1.1: C3 x16\n  1.5: E3 x2\n  1.9: G3 x2\n")).unwrap_err();
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|e| e.fault == Fault::Overlap { with: pos(1, 1), line: 4, through: pos(1, 16) }));
        assert_eq!(errors.iter().map(|e| e.at).collect::<Vec<_>>(), [Some(pos(1, 5)), Some(pos(1, 9))]);
    }

    #[test]
    fn rests_count_toward_overlap_and_order_does_not_hide_it() {
        assert_eq!(faults(&format!("{TWO_BARS}  1.5: - x4\n  1.1: C3 x8\n")), [Fault::Overlap { with: pos(1, 1), line: 5, through: pos(1, 8) }]);
        assert_eq!(faults(&format!("{TWO_BARS}  1.1: C3 x1\n  1.1: E3 x1\n")), [Fault::Overlap { with: pos(1, 1), line: 4, through: pos(1, 1) }]);
    }

    #[test]
    fn running_past_the_pattern_end_is_an_error() {
        let errors = Song::parse(&format!("{TWO_BARS}  2.13: C3 x8\n")).unwrap_err();
        assert_eq!(errors[0].fault, Fault::RunsPastEnd { through: pos(3, 4), last: pos(2, 16), over: 4 });
        assert_eq!(errors[0].to_string(), "line 4, pattern main, lane triangle, at 2.13: rings through 3.4, 4 slots past the pattern's last slot 2.16");
    }

    #[test]
    fn slots_and_bars_out_of_range() {
        let errors = Song::parse(&format!("{TWO_BARS}  1.0: C3 x1\n  1.17: C3 x1\n  2.17: C3 x1\n  0.3: C3 x1\n  3.1: C3 x1\n")).unwrap_err();
        let got: Vec<Fault> = errors.iter().map(|e| e.fault.clone()).collect();
        assert_eq!(got, [
            Fault::SlotZero,
            Fault::SlotPastGrid { slot: 17, grid: 16, meant: Some(pos(2, 1)) },
            Fault::SlotPastGrid { slot: 17, grid: 16, meant: None },
            Fault::BarZero,
            Fault::BarPastEnd { bar: 3, bars: 2 },
        ]);
        assert_eq!(errors[1].to_string(), "line 5, pattern main, lane triangle, at 1.17: slot 17 doesn't exist; this pattern's grid is 16 slots per bar. counting on from the bar's start, slot 17 lands on 2.1");
    }

    #[test]
    fn a_lane_appears_once_per_pattern() {
        let errors = Song::parse(&format!("{TWO_BARS}  1.1: C3 x4\nlane pulse1\nlane triangle\n  1.1: C3 x4\n")).unwrap_err();
        assert_eq!(errors.len(), 1, "the twin's notes shouldn't pile on overlap errors: {errors:?}");
        assert_eq!((errors[0].line, errors[0].lane.as_deref(), &errors[0].fault), (Some(6), Some("triangle"), &Fault::SecondLane { first: 3 }));
    }

    #[test]
    fn three_mistakes_three_errors() {
        let errors = Song::parse(&format!("{TWO_BARS}  1.0: C3 x4\n  1.5: E3 x8\n  1.9: G3 x2\n  2.9: C3 x9\n")).unwrap_err();
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [Some(4), Some(6), Some(7)], "{errors:?}");
    }

    #[test]
    fn syntax_and_lint_errors_arrive_together_in_line_order() {
        let text = "song \"t\" tempo fast\npattern main bars 1 grid 8\nlane pulse1 duty\n  1: H4 x2\n  3: C4 x0\n  5 C4 x2\n  4: C4\n  9: C4 x1\nwhat is this\n";
        assert_eq!(faults(text), [
            Fault::NotCount { key: "tempo".into(), value: "fast".into() },
            Fault::NoValue("duty".into()),
            Fault::BadPitch("H4".into()),
            Fault::Silent,
            Fault::NoColon("5 C4 x2".into()),
            Fault::NoLength,
            Fault::SlotPastGrid { slot: 9, grid: 8, meant: None },
            Fault::Unreadable("what is this".into()),
        ]);
    }

    #[test]
    fn broken_pattern_header_does_not_cascade() {
        assert_eq!(faults("song \"t\" tempo 90\npattern main bars 2\nlane noise\n  1.99: C3 x99\n"), [Fault::MissingParam { directive: "pattern", key: "grid" }]);
    }

    #[test]
    fn headers_are_checked() {
        assert_eq!(faults("pattern bars 1 grid 4\n"), [Fault::NoSong, Fault::NoName("pattern")]);
        assert_eq!(faults("song tempo 90\nsong \"again\" tempo 90\n"), [Fault::NoTitle, Fault::SecondSong { first: 1 }]);
        assert_eq!(faults("song \"t\" tempo 90 bpm 90\npattern a bars 1 grid 4 bars 2\npattern a bars 1 grid 4\n"), [
            Fault::UnknownParam { directive: "song", key: "bpm".into(), takes: &["tempo", "profile"] },
            Fault::RepeatedParam("bars".into()),
            Fault::SecondPattern { first: 2 },
        ]);
        assert_eq!(faults("song \"t\" tempo 90\nlane pulse1\n  1: C4 x1\npattern a bars 1 grid 4\n  1: C4 x1\n  2: C4 x1\n"), [Fault::LaneOutsidePattern, Fault::NoteOutsideLane]);
    }

    #[test]
    fn comments_blank_lines_and_quoted_semicolons() {
        let song = Song::parse("; a jingle\n\nsong \"a; b\" tempo 90 ; trailing\n   ; indented comment\npattern a bars 1 grid 4 ; also here\nlane pulse1 duty 12 vol 15\n  1: C4 x1 ; a note\n").unwrap();
        assert_eq!(song.title, "a; b");
        assert_eq!(song.profile, None);
        assert_eq!(song.patterns[0].lanes[0].params.len(), 2);
        assert_eq!(song.patterns[0].lanes[0].notes[0].line, 7);
    }

    #[test]
    fn pitch_is_scientific_with_middle_c_at_60() {
        assert_eq!(read_pitch("C4"), Some(60));
        assert_eq!(read_pitch("C#4"), Some(61));
        assert_eq!(read_pitch("Db4"), read_pitch("C#4"));
        assert_eq!(read_pitch("A4"), Some(69));
        assert_eq!(read_pitch("Cb4"), read_pitch("B3"));
        assert_eq!(read_pitch("C0"), Some(12));
        for junk in ["c4", "H4", "C", "C##4", "C10", "C#", "Cx4", "-"] { assert_eq!(read_pitch(junk), None, "{junk:?} should not read"); }
    }

    #[test]
    fn a_forgotten_pitch_is_named_as_such() {
        assert_eq!(faults(&format!("{TWO_BARS}  1.1: x4\n")), [Fault::NoPitch]);
    }
}
