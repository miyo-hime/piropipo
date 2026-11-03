use crate::profile::{self, Breach};
use std::fmt;

const MARKS: [&str; 6] = ["vib", "arp", "from", "duty", "vol", "fade"];

const BENDS: [&str; 3] = ["vib", "arp", "from"];

pub const COMMON_TIME: u32 = 4;

pub const MOST_BEATS: u32 = 32;

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

#[derive(Debug, Clone, PartialEq)]
pub struct Note { pub at: Position, pub pitch: Option<u8>, pub len: u32, pub line: usize, pub ornaments: Ornaments }

impl Note {
    pub fn rings_through(&self, grid: u32) -> Position { Position::from_tick((self.at.tick(grid) + u64::from(self.len)).saturating_sub(1), grid) }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ornaments { pub bend: Option<Bend>, pub duty: Option<u8>, pub vol: Option<u8>, pub fade: bool }

#[derive(Debug, Clone, PartialEq)]
pub enum Bend { Vibrato(Vibrato), Arpeggio(Vec<i16>), Slide(u8) }

impl Bend {
    pub fn word(&self) -> &'static str {
        match self { Bend::Vibrato(_) => "vib", Bend::Arpeggio(_) => "arp", Bend::Slide(_) => "from" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vibrato { pub cents: f64, pub speed: u8, pub delay: u8 }

impl Vibrato {
    pub fn hertz(self) -> f64 { 20.0 / f64::from(self.speed) }

    pub fn delay_ms(self) -> f64 { f64::from(self.delay) * 1000.0 / 60.0 }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lane { pub name: String, pub params: Vec<(String, String)>, pub notes: Vec<Note>, pub line: usize }

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern { pub name: String, pub bars: u32, pub beats: u32, pub grid: u32, pub lanes: Vec<Lane>, pub line: usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue { pub pattern: String, pub shift: i8, pub line: usize }

#[derive(Debug, Clone, PartialEq)]
pub struct Song { pub title: String, pub tempo: u32, pub profile: Option<String>, pub patterns: Vec<Pattern>, pub setlist: Vec<Cue> }

impl Song {
    pub fn parse(text: &str) -> Result<Song, Vec<ScoreError>> {
        let mut copyist = Copyist::default();
        for (i, raw) in text.lines().enumerate() { copyist.read(i + 1, raw); }
        copyist.finish()
    }

    pub fn pattern(&self, name: &str) -> Option<&Pattern> { self.patterns.iter().find(|p| p.name == name) }
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
    BeatsUnreachable(u32),
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
    BareVibrato,
    VibratoMeasure(String),
    VibratoMissing(&'static str),
    VibratoTwice(String),
    RateUnreachable(String),
    RateBetween { word: String, faster: u16, slower: u16 },
    DelayUnreachable(String),
    BadArp(String),
    LoneArp(String),
    BadDeparture(String),
    Standstill(String),
    BadDuty(String),
    BadVolume(String),
    SecondBend { first: &'static str, second: &'static str },
    RestEffect(&'static str),
    BarZero,
    SlotZero,
    BarPastEnd { bar: u32, bars: u32 },
    SlotPastGrid { slot: u32, grid: u32, meant: Option<Position> },
    RunsPastEnd { through: Position, last: Position, over: u64 },
    Overlap { with: Position, line: usize, through: Position },
    SecondPlay { first: usize },
    CrowdedPlay(String),
    EmptyPlay,
    BadShift(String),
    FarShift(String),
    CueLeftover(String),
    UnknownCue { name: String, known: Vec<String> },
}

impl Fault {
    pub fn code(&self) -> &'static str {
        match self {
            Fault::Unreadable(_) => "unreadable-line",
            Fault::NoSong => "no-song",
            Fault::SecondSong { .. } => "second-song",
            Fault::NoTitle => "no-title",
            Fault::UnclosedTitle => "unclosed-title",
            Fault::NoName(_) => "no-name",
            Fault::MissingParam { .. } => "missing-param",
            Fault::UnknownParam { .. } => "unknown-param",
            Fault::RepeatedParam(_) => "repeated-param",
            Fault::NoValue(_) => "no-value",
            Fault::NotCount { .. } => "not-a-count",
            Fault::BeatsUnreachable(_) => "beats-out-of-reach",
            Fault::SecondPattern { .. } => "second-pattern",
            Fault::LaneOutsidePattern => "lane-outside-pattern",
            Fault::NoteOutsideLane => "note-outside-lane",
            Fault::SecondLane { .. } => "second-lane",
            Fault::NoColon(_) => "no-colon",
            Fault::BadPosition(_) => "bad-position",
            Fault::NoPitch => "no-pitch",
            Fault::BadPitch(_) => "bad-pitch",
            Fault::NoLength => "no-length",
            Fault::BadLength(_) => "bad-length",
            Fault::Silent => "zero-length",
            Fault::Leftover(_) => "leftover",
            Fault::BareVibrato | Fault::VibratoMeasure(_) | Fault::VibratoMissing(_) | Fault::VibratoTwice(_) => "bad-vibrato",
            Fault::RateUnreachable(_) | Fault::RateBetween { .. } => "vibrato-rate",
            Fault::DelayUnreachable(_) => "vibrato-delay",
            Fault::BadArp(_) | Fault::LoneArp(_) => "bad-arp",
            Fault::BadDeparture(_) => "bad-slide",
            Fault::Standstill(_) => "slide-goes-nowhere",
            Fault::BadDuty(_) => "bad-duty",
            Fault::BadVolume(_) => "bad-volume",
            Fault::SecondBend { .. } => "two-pitch-effects",
            Fault::RestEffect(_) => "effect-on-rest",
            Fault::BarZero => "bar-zero",
            Fault::SlotZero => "slot-zero",
            Fault::BarPastEnd { .. } => "bar-past-end",
            Fault::SlotPastGrid { .. } => "slot-past-grid",
            Fault::RunsPastEnd { .. } => "runs-past-end",
            Fault::Overlap { .. } => "overlap",
            Fault::SecondPlay { .. } => "second-play",
            Fault::CrowdedPlay(_) => "crowded-play",
            Fault::EmptyPlay => "empty-play",
            Fault::BadShift(_) => "bad-transpose",
            Fault::FarShift(_) => "transpose-too-far",
            Fault::CueLeftover(_) => "leftover",
            Fault::UnknownCue { .. } => "unknown-pattern",
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = |n: u64| if n == 1 { "" } else { "s" };
        match self {
            Fault::Unreadable(text) => write!(f, "can't read `{text}`; a line is a `song`, `pattern`, `lane`, or `play` directive, or a note like `1.5: C4 x2`"),
            Fault::NoSong => write!(f, "no `song` line; every file declares one, like `song \"first light\" tempo 112`"),
            Fault::SecondSong { first } => write!(f, "a second `song` line; the song was already declared on line {first}"),
            Fault::NoTitle => write!(f, "the song needs a quoted title right after `song`, like `song \"first light\"`"),
            Fault::UnclosedTitle => write!(f, "the song title's opening quote never closes"),
            Fault::NoName(directive) => write!(f, "`{directive}` needs a name first, like `{}`", if *directive == "lane" { "lane pulse1" } else { "pattern main bars 2 grid 16" }),
            Fault::MissingParam { directive, key } => write!(f, "`{directive}` is missing `{key} N` ({})", gloss(key)),
            Fault::UnknownParam { directive, key, takes } => write!(f, "`{directive}` doesn't take `{key}`; it takes {}", profile::roster(takes.iter().copied(), "and")),
            Fault::RepeatedParam(key) => write!(f, "`{key}` is set twice on this line"),
            Fault::NoValue(key) => write!(f, "`{key}` has no value after it"),
            Fault::NotCount { key, value } => write!(f, "`{key}` wants a whole number above 0, got `{value}`"),
            Fault::BeatsUnreachable(beats) => write!(f, "beats {beats} is out of reach; keep it between 1 and {MOST_BEATS} beats per bar"),
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
            Fault::Leftover(text) => write!(f, "unexpected `{text}` after the length; a note line is `bar.slot: PITCH xLEN`, then any of the effects {}", MARKS.map(|m| format!("`{m}`")).join(", ")),
            Fault::BareVibrato => write!(f, "`vib` needs a depth and a rate, like `vib 20ct 6hz`, and takes an optional delay, like `250ms`"),
            Fault::VibratoMeasure(word) => write!(f, "`{word}` isn't a vibrato measure; write a number and its unit: `20ct` for depth, `6hz` for rate, `250ms` for delay"),
            Fault::VibratoMissing("ct") => write!(f, "`vib` needs a depth in cents, like `20ct`"),
            Fault::VibratoMissing(_) => write!(f, "`vib` needs a rate in hertz, like `6hz`"),
            Fault::VibratoTwice(word) => write!(f, "`{word}` is a second measure of the same kind; `vib` takes one depth (`ct`), one rate (`hz`), and one delay (`ms`)"),
            Fault::RateUnreachable(word) => write!(f, "`{word}` is outside the chip's vibrato; its rates run from 20 Hz down to 20/255 Hz (about 0.0785)"),
            Fault::RateBetween { word, faster, slower } => write!(f, "`{word}` falls between the chip's vibrato rates {} Hz and {} Hz, more than 25% from either; ask for one of those", hertz(*faster), hertz(*slower)),
            Fault::DelayUnreachable(word) => write!(f, "`{word}` is past the vibrato's longest delay, 4250 ms (255 frames)"),
            Fault::BadArp(word) => write!(f, "`{word}` isn't an arp; write signed semitone offsets as one comma-joined word, like `arp 0,4,7`"),
            Fault::LoneArp(word) => write!(f, "`arp {word}` names one offset; an arp cycles through at least two, like `arp 0,{word}`"),
            Fault::BadDeparture(word) => write!(f, "`from {word}` needs a departure pitch, like `from A4`; the note's own pitch is where the slide lands"),
            Fault::Standstill(word) => write!(f, "`from {word}` departs from the note's own pitch, so there's nothing to slide; drop the `from` or name another departure"),
            Fault::BadDuty(word) => write!(f, "{}", Breach::Duty(word.clone())),
            Fault::BadVolume(word) => write!(f, "`vol {word}` isn't a volume the chip has; pick a whole number from 0 to 15"),
            Fault::SecondBend { first, second } => write!(f, "`{first}` and `{second}` both bend this note's pitch; a note takes one pitch effect at most"),
            Fault::RestEffect(word) => write!(f, "`{word}` on a rest; a rest has nothing to bend, shake, or fade"),
            Fault::BarZero => write!(f, "there is no bar 0; bars count from 1"),
            Fault::SlotZero => write!(f, "there is no slot 0; slots count from 1"),
            Fault::BarPastEnd { bar, bars } => write!(f, "bar {bar} is past the end; this pattern has {bars} bar{}", s(u64::from(*bars))),
            Fault::SlotPastGrid { slot, grid, meant } => {
                write!(f, "slot {slot} doesn't exist; this pattern's grid is {grid} slots per bar")?;
                match meant { Some(m) => write!(f, ". counting on from the bar's start, slot {slot} lands on {m}"), None => Ok(()) }
            }
            Fault::RunsPastEnd { through, last, over } => write!(f, "rings through {through}, {over} slot{} past the pattern's last slot {last}", s(*over)),
            Fault::Overlap { with, line, through } => write!(f, "overlaps the note at {with} (line {line}), which rings through {through}; a lane plays one note at a time"),
            Fault::SecondPlay { first } => write!(f, "a second `play` block; the song order already started on line {first}, and one block lists the whole song"),
            Fault::CrowdedPlay(text) => write!(f, "`play` stands alone on its line, so `{text}` has nowhere to go; list the patterns under it, one per line"),
            Fault::EmptyPlay => write!(f, "the `play` block lists no patterns; name them under it, one per line, in the order they play"),
            Fault::BadShift(word) => write!(f, "`{word}` isn't a transpose; after the pattern name a play line takes only `+N` or `-N` semitones, and a repeat is the name written again on its own line"),
            Fault::FarShift(word) => write!(f, "`{word}` semitones is further than any chip reaches; keep a transpose between -127 and +127"),
            Fault::CueLeftover(text) => write!(f, "unexpected `{text}` after the transpose; a play line is just a pattern name and an optional `+N` or `-N`"),
            Fault::UnknownCue { name, known: none } if none.is_empty() => write!(f, "no pattern is named `{name}`; this file declares no patterns to play"),
            Fault::UnknownCue { name, known } => write!(f, "no pattern is named `{name}`; the patterns are {}", known.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")),
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
struct Copyist { head: Option<Head>, sheets: Vec<Sheet>, setlist: Option<(usize, Vec<Cue>)>, cueing: bool, pen: Pen, pattern: Option<String>, lane: Option<String>, errors: Vec<ScoreError> }

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
            "play" => self.play(line, rest),
            _ if self.cueing && matches!(self.pen, Pen::Lifted) => self.cue(line, text),
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
            // `beats` stays a legal pattern name from before meters existed; it's only the key when a count follows it
            Some((name, tail)) if !matches!(*name, "bars" | "grid") && !(*name == "beats" && tail.first().is_some_and(|t| whole(t).is_some())) => (*name, tail),
            _ => ("", &tokens[..]),
        };
        self.pattern = (!name.is_empty()).then(|| name.to_owned());
        self.lane = None;
        self.pen = Pen::Lifted;
        self.cueing = false;
        let mut faults = Vec::new();
        if name.is_empty() { faults.push(Fault::NoName("pattern")); }
        else if let Some(twin) = self.sheets.iter().find(|s| s.pattern.name == name) { faults.push(Fault::SecondPattern { first: twin.pattern.line }); }
        let [bars, beats, grid] = fields("pattern", tokens, &["bars", "beats", "grid"], &mut faults);
        let bars = required("pattern", "bars", bars, &mut faults);
        let beats = beats.map_or(Some(COMMON_TIME), |value| counted("beats", value, &mut faults));
        if let Some(beats) = beats.filter(|b| *b > MOST_BEATS) { faults.push(Fault::BeatsUnreachable(beats)); }
        let grid = required("pattern", "grid", grid, &mut faults);
        for fault in faults { self.blame(line, None, fault); }
        let sound = bars.is_some() && grid.is_some();
        self.sheets.push(Sheet { pattern: Pattern { name: name.into(), bars: bars.unwrap_or(0), beats: beats.unwrap_or(COMMON_TIME), grid: grid.unwrap_or(0), lanes: Vec::new(), line }, sound });
    }

    fn lane(&mut self, line: usize, rest: &str) {
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        self.lane = tokens.first().map(|n| (*n).to_owned());
        self.pen = Pen::Dry;
        let Some((name, rest)) = tokens.split_first() else { return self.blame(line, None, Fault::NoName("lane")) };
        let Some(sheet) = self.sheets.last().filter(|_| !self.cueing) else { return self.blame(line, None, Fault::LaneOutsidePattern) };
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

    fn play(&mut self, line: usize, rest: &str) {
        self.pattern = None;
        self.lane = None;
        self.pen = Pen::Lifted;
        self.cueing = true;
        if let Some(&(first, _)) = self.setlist.as_ref() { self.blame(line, None, Fault::SecondPlay { first }); } else { self.setlist = Some((line, Vec::new())); }
        if !rest.is_empty() { self.blame(line, None, Fault::CrowdedPlay(rest.into())); }
    }

    fn cue(&mut self, line: usize, text: &str) {
        let mut words = text.split_whitespace();
        let Some(name) = words.next() else { return };
        let mut faults = Vec::new();
        let shift = words.next().map_or(0, |word| read_shift(word).unwrap_or_else(|fault| { faults.push(fault); 0 }));
        let leftover: Vec<&str> = words.collect();
        if !leftover.is_empty() { faults.push(Fault::CueLeftover(leftover.join(" "))); }
        for fault in faults { self.blame(line, None, fault); }
        if let Some((_, cues)) = self.setlist.as_mut() { cues.push(Cue { pattern: name.into(), shift, line }); }
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
        let (ornaments, marks) = adorn(words, &mut faults);
        if let (Some(None), Some(&first)) = (pitch, marks.first()) { faults.push(Fault::RestEffect(first)); }
        if let (Some(Some(target)), Some(Bend::Slide(from))) = (pitch, &ornaments.bend) && target == *from { faults.push(Fault::Standstill(profile::named(*from))); }
        let clean = faults.is_empty();
        for fault in faults { self.blame(line, Some(at), fault); }
        if let (true, Pen::Writing, Some(pitch), Some(len)) = (clean, &self.pen, pitch, len)
            && let Some(lane) = self.sheets.last_mut().and_then(|s| s.pattern.lanes.last_mut())
        {
            lane.notes.push(Note { at, pitch, len, line, ornaments });
        }
    }

    fn finish(mut self) -> Result<Song, Vec<ScoreError>> {
        for sheet in self.sheets.iter().filter(|s| s.sound) { lint(&sheet.pattern, &mut self.errors); }
        let setlist = match self.setlist.take() {
            Some((line, cues)) => {
                if cues.is_empty() { self.blame_song(line, Fault::EmptyPlay); }
                let known: Vec<String> = self.sheets.iter().map(|s| s.pattern.name.clone()).filter(|n| !n.is_empty()).collect();
                for cue in cues.iter().filter(|c| !known.contains(&c.pattern)) { self.blame_song(cue.line, Fault::UnknownCue { name: cue.pattern.clone(), known: known.clone() }); }
                cues
            }
            None => self.sheets.iter().map(|s| Cue { pattern: s.pattern.name.clone(), shift: 0, line: s.pattern.line }).collect(),
        };
        if self.head.is_none() { self.errors.push(ScoreError { line: None, pattern: None, lane: None, at: None, fault: Fault::NoSong }); }
        self.errors.sort_by_key(|e| e.line);
        match self.head {
            Some(head) if self.errors.is_empty() => Ok(Song { title: head.title, tempo: head.tempo, profile: head.profile, patterns: self.sheets.into_iter().map(|s| s.pattern).collect(), setlist }),
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
    counted(key, value, faults)
}

fn counted(key: &str, value: &str, faults: &mut Vec<Fault>) -> Option<u32> {
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

fn read_shift(word: &str) -> Result<i8, Fault> {
    let steps = word.strip_prefix('+').or_else(|| word.strip_prefix('-')).and_then(whole).ok_or_else(|| Fault::BadShift(word.into()))?;
    let steps = i8::try_from(steps).map_err(|_| Fault::FarShift(word.into()))?;
    Ok(if word.starts_with('-') { -steps } else { steps })
}

fn adorn<'w>(words: impl Iterator<Item = &'w str>, faults: &mut Vec<Fault>) -> (Ornaments, Vec<&'static str>) {
    let mut words = words.peekable();
    let mut ornaments = Ornaments::default();
    let mut marks: Vec<&'static str> = Vec::new();
    while let Some(word) = words.next() {
        let Some(&mark) = MARKS.iter().find(|m| **m == word) else { faults.push(Fault::Leftover(word.into())); continue };
        if marks.contains(&mark) { faults.push(Fault::RepeatedParam(mark.into())); }
        else if let Some(first) = marks.iter().copied().find(|m| BENDS.contains(m)).filter(|_| BENDS.contains(&mark)) { faults.push(Fault::SecondBend { first, second: mark }); }
        marks.push(mark);
        if mark == "fade" { ornaments.fade = true; continue; }
        if mark == "vib" {
            let measures: Vec<&str> = std::iter::from_fn(|| words.next_if(|w| w.starts_with(|c: char| c.is_ascii_digit() || c == '.'))).collect();
            match shake(&measures) { Ok(vib) => ornaments.bend = Some(Bend::Vibrato(vib)), Err(fault) => faults.push(fault) }
            continue;
        }
        let Some(value) = words.next_if(|w| !MARKS.contains(w)) else { faults.push(Fault::NoValue(mark.into())); continue };
        let read = match mark {
            "arp" => read_offsets(value).map(|offsets| ornaments.bend = Some(Bend::Arpeggio(offsets))),
            "from" => read_pitch(value).map(|from| ornaments.bend = Some(Bend::Slide(from))).ok_or_else(|| Fault::BadDeparture(value.into())),
            "duty" => profile::width(value).map(|duty| ornaments.duty = Some(duty)).ok_or_else(|| Fault::BadDuty(value.into())),
            _ => profile::level(value).map(|vol| ornaments.vol = Some(vol)).ok_or_else(|| Fault::BadVolume(value.into())),
        };
        if let Err(fault) = read { faults.push(fault); }
    }
    (ornaments, marks)
}

fn shake(measures: &[&str]) -> Result<Vibrato, Fault> {
    if measures.is_empty() { return Err(Fault::BareVibrato); }
    let mut found: [Option<(&str, f64)>; 3] = [None; 3];
    for word in measures {
        let units = ["ct", "hz", "ms"];
        let cut = word.len().saturating_sub(2);
        let (Some(number), Some(unit)) = (word.get(..cut), word.get(cut..)) else { return Err(Fault::VibratoMeasure((*word).into())) };
        let (Some(slot), Some(value)) = (units.iter().position(|u| u.eq_ignore_ascii_case(unit)), measure(number)) else { return Err(Fault::VibratoMeasure((*word).into())) };
        if found[slot].is_some() { return Err(Fault::VibratoTwice((*word).into())); }
        found[slot] = Some((*word, value));
    }
    let [Some((_, cents)), Some((rate, hz)), delay] = found else { return Err(Fault::VibratoMissing(if found[0].is_none() { "ct" } else { "hz" })) };
    let delay = match delay {
        Some((word, ms)) if ms > 4250.0 => return Err(Fault::DelayUnreachable(word.into())),
        Some((_, ms)) => (ms * 60.0 / 1000.0).round() as u8,
        None => 0,
    };
    Ok(Vibrato { cents, speed: lfo_speed(rate, hz)?, delay })
}

fn lfo_speed(word: &str, hz: f64) -> Result<u8, Fault> {
    if !(20.0 / 255.0..=20.0).contains(&hz) { return Err(Fault::RateUnreachable(word.into())); }
    let faster = (20.0 / hz).floor().clamp(1.0, 255.0) as u16;
    let slower = faster + 1;
    let off = |speed: u16| (20.0 / f64::from(speed) - hz).abs();
    let speed = if slower <= 255 && off(slower) <= off(faster) { slower } else { faster };
    if off(speed) > hz * 0.25 { return Err(Fault::RateBetween { word: word.into(), faster, slower }); }
    Ok(u8::try_from(speed).expect("a speed past 255 is out of the domain checked above"))
}

fn hertz(speed: u16) -> String { format!("{:.3}", 20.0 / f64::from(speed)).trim_end_matches('0').trim_end_matches('.').to_owned() }

fn measure(text: &str) -> Option<f64> {
    let (int, frac) = text.split_once('.').unwrap_or((text, "0"));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(int) || !digits(frac) { return None; }
    text.parse().ok()
}

fn read_offsets(word: &str) -> Result<Vec<i16>, Fault> {
    let offsets = word.split(',').map(|o| {
        let digits = o.strip_prefix(['+', '-']).unwrap_or(o);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) { return None; }
        o.parse().ok()
    }).collect::<Option<Vec<i16>>>().ok_or_else(|| Fault::BadArp(word.into()))?;
    if offsets.len() < 2 { return Err(Fault::LoneArp(word.into())); }
    Ok(offsets)
}

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
        let doc = include_str!("../skills/piropipo/references/format.md");
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
    fn beats_sits_between_bars_and_grid_and_defaults_to_common_time() {
        let song = Song::parse("song \"t\" tempo 120\npattern verse bars 2 beats 7 grid 14\npattern chorus bars 1 grid 16\npattern bridge grid 12 beats 3 bars 4\n").unwrap();
        assert_eq!(song.patterns.iter().map(|p| (p.name.as_str(), p.bars, p.beats, p.grid)).collect::<Vec<_>>(), [("verse", 2, 7, 14), ("chorus", 1, 4, 16), ("bridge", 4, 3, 12)]);
        assert_eq!(Song::parse("song \"t\" tempo 120\npattern p bars 1 beats 4 grid 16\n").unwrap().patterns, Song::parse("song \"t\" tempo 120\npattern p bars 1 grid 16\n").unwrap().patterns, "an explicit beats 4 is the pattern the default writes");
        assert_eq!(Song::parse("song \"t\" tempo 120\npattern p bars 1 beats 32 grid 64\npattern q bars 1 beats 1 grid 1\n").unwrap().patterns.iter().map(|p| p.beats).collect::<Vec<_>>(), [32, 1]);
    }

    #[test]
    fn beats_out_of_reach_are_refused_at_the_pattern() {
        let refused = |beats: &str| Song::parse(&format!("song \"t\" tempo 120\npattern verse bars 2 beats {beats} grid 14\nlane pulse1\n  1.1: C4 x1\n")).unwrap_err();
        assert_eq!(refused("0").iter().map(|e| e.fault.clone()).collect::<Vec<_>>(), [Fault::NotCount { key: "beats".into(), value: "0".into() }]);
        assert_eq!(refused("33").iter().map(|e| e.fault.clone()).collect::<Vec<_>>(), [Fault::BeatsUnreachable(33)]);
        assert_eq!(refused("seven").iter().map(|e| e.fault.clone()).collect::<Vec<_>>(), [Fault::NotCount { key: "beats".into(), value: "seven".into() }]);
        assert_eq!(refused("7.5")[0].fault, Fault::NotCount { key: "beats".into(), value: "7.5".into() });
        assert_eq!(refused("33")[0].to_string(), "line 2, pattern verse: beats 33 is out of reach; keep it between 1 and 32 beats per bar");
        assert_eq!(refused("0")[0].to_string(), "line 2, pattern verse: `beats` wants a whole number above 0, got `0`");
        assert_eq!(Fault::BeatsUnreachable(33).code(), "beats-out-of-reach");
        assert_eq!(faults("song \"t\" tempo 120\npattern p bars 1 beats 3 grid 4 beats 5\n"), [Fault::RepeatedParam("beats".into())]);
        assert_eq!(faults("song \"t\" tempo 120\npattern p bars 1 beats\n"), [Fault::NoValue("beats".into()), Fault::MissingParam { directive: "pattern", key: "grid" }]);
    }

    #[test]
    fn a_pattern_named_beats_still_parses_and_a_forgotten_name_is_still_caught() {
        let named = Song::parse("song \"t\" tempo 120\npattern beats bars 2 grid 16\nlane noise\n  1.1: C4 x1\npattern fills bars 1 grid 16\nplay\n  beats\n  fills\n").unwrap();
        assert_eq!((named.patterns[0].name.as_str(), named.patterns[0].beats), ("beats", 4), "a file from before meters keeps its drum pattern");
        assert_eq!(Song::parse("song \"t\" tempo 120\npattern beats beats 7 bars 1 grid 14\n").unwrap().patterns[0].beats, 7);
        assert_eq!(faults("song \"t\" tempo 120\npattern beats 7 bars 1 grid 14\n"), [Fault::NoName("pattern")]);
        let said = Song::parse("song \"t\" tempo 120\npattern p bars 1 meter 7 grid 14\n").unwrap_err()[0].to_string();
        assert_eq!(said, "line 2, pattern p: `pattern` doesn't take `meter`; it takes `bars`, `beats`, and `grid`");
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

    const PAIR: &str = "song \"t\" tempo 120\npattern intro bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern main bars 1 grid 4\nlane triangle\n  1: E2 x4\n";

    fn cues(song: &Song) -> Vec<(&str, i8, usize)> { song.setlist.iter().map(|c| (c.pattern.as_str(), c.shift, c.line)).collect() }

    #[test]
    fn the_play_block_is_the_running_order_repeats_and_all() {
        let song = Song::parse(&format!("{PAIR}play\n  intro\n  main\n  main\n  intro +3\n  main -12\n")).unwrap();
        assert_eq!(cues(&song), [("intro", 0, 9), ("main", 0, 10), ("main", 0, 11), ("intro", 3, 12), ("main", -12, 13)]);
        assert_eq!(song.patterns.len(), 2, "a repeat is a second cue, never a second pattern");
    }

    #[test]
    fn no_play_block_means_file_order() {
        assert_eq!(cues(&Song::parse(PAIR).unwrap()), [("intro", 0, 2), ("main", 0, 5)]);
    }

    #[test]
    fn the_play_block_can_open_the_file_and_name_patterns_written_later() {
        let song = Song::parse("song \"t\" tempo 120\nplay ; the order\n  b\n  a +7 ; up a fifth\npattern a bars 1 grid 4\nlane pulse1\n  1: C4 x4\npattern b bars 1 grid 4\n").unwrap();
        assert_eq!(cues(&song), [("b", 0, 3), ("a", 7, 4)]);
        assert_eq!(song.patterns[0].lanes[0].notes.len(), 1);
    }

    #[test]
    fn count_sugar_is_refused_with_the_way_to_repeat() {
        let errors = Song::parse(&format!("{PAIR}play\n  main x2\n")).unwrap_err();
        assert_eq!((errors.len(), errors[0].line, &errors[0].fault), (1, Some(9), &Fault::BadShift("x2".into())));
        assert_eq!(errors[0].to_string(), "line 9: `x2` isn't a transpose; after the pattern name a play line takes only `+N` or `-N` semitones, and a repeat is the name written again on its own line");
    }

    #[test]
    fn play_lines_are_checked() {
        assert_eq!(faults(&format!("{PAIR}play\n  intro +\n  main +3 again\n  mian\n  outro +128\nplay\n")), [
            Fault::BadShift("+".into()),
            Fault::CueLeftover("again".into()),
            Fault::UnknownCue { name: "mian".into(), known: vec!["intro".into(), "main".into()] },
            Fault::FarShift("+128".into()),
            Fault::UnknownCue { name: "outro".into(), known: vec!["intro".into(), "main".into()] },
            Fault::SecondPlay { first: 8 },
        ]);
        assert_eq!(faults(&format!("{PAIR}play\n")), [Fault::EmptyPlay]);
        assert_eq!(faults(&format!("{PAIR}play intro main\n  intro\n")), [Fault::CrowdedPlay("intro main".into())]);
        assert_eq!(faults(&format!("{PAIR}play\n  main\nlane pulse2\n  1: C4 x4\n")), [Fault::LaneOutsidePattern]);
        let lonely = Song::parse("song \"t\" tempo 90\nplay\n  main\n").unwrap_err();
        assert_eq!(lonely[0].to_string(), "line 3: no pattern is named `main`; this file declares no patterns to play");
    }

    #[test]
    fn transposes_are_signed_semitones_within_reach() {
        for (word, steps) in [("+3", 3), ("-12", -12), ("+0", 0), ("-0", 0), ("+127", 127), ("-127", -127)] { assert_eq!(read_shift(word), Ok(steps), "{word}"); }
        for junk in ["3", "x2", "+", "+-3", "++3", "+3.5", "-x"] { assert_eq!(read_shift(junk), Err(Fault::BadShift(junk.into())), "{junk:?} should not read"); }
        assert_eq!(read_shift("-128"), Err(Fault::FarShift("-128".into())));
    }

    fn adorned(note: &str) -> Ornaments { Song::parse(&format!("{TWO_BARS}  1.1: {note}\n")).unwrap_or_else(|e| panic!("{note}: {e:?}")).patterns[0].lanes[0].notes[0].ornaments.clone() }

    fn refused(note: &str) -> Vec<Fault> { faults(&format!("{TWO_BARS}  1.1: {note}\n")) }

    #[test]
    fn effects_ride_after_the_length_in_any_order() {
        assert_eq!(adorned("C4 x4"), Ornaments::default());
        assert_eq!(adorned("G5 x12 vib 20ct 6hz 250ms").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 3, delay: 15 })));
        assert_eq!(adorned("G5 x12 vib 250MS 6Hz 20CT").bend, adorned("G5 x12 vib 20ct 6hz 250ms").bend, "units are case-blind and self-describing");
        assert_eq!(adorned("G5 x12 vib 12.5ct 0.08hz").bend, Some(Bend::Vibrato(Vibrato { cents: 12.5, speed: 250, delay: 0 })));
        assert_eq!(adorned("A3 x16 arp 0,3,7").bend, Some(Bend::Arpeggio(vec![0, 3, 7])));
        assert_eq!(adorned("A3 x16 arp 4,-5,+12,4").bend, Some(Bend::Arpeggio(vec![4, -5, 12, 4])));
        assert_eq!(adorned("C5 x8 from A4").bend, Some(Bend::Slide(69)));
        assert_eq!(adorned("B4 x8 duty 12.5").duty, Some(0));
        assert_eq!(adorned("C5 x4 vol 5").vol, Some(5));
        let stacked = adorned("A4 x16 fade duty 75 vol 12 vib 10ct 5hz");
        assert_eq!(stacked, Ornaments { bend: Some(Bend::Vibrato(Vibrato { cents: 10.0, speed: 4, delay: 0 })), duty: Some(3), vol: Some(12), fade: true });
        assert_eq!(adorned("A4 x16 arp 0,4 vol 0").vol, Some(0), "vol stacks with any pitch effect");
    }

    #[test]
    fn malformed_effects_teach_the_spelling() {
        assert_eq!(refused("C4 x4 vib"), [Fault::BareVibrato]);
        assert_eq!(refused("C4 x4 vib 20 6hz"), [Fault::VibratoMeasure("20".into())]);
        assert_eq!(refused("C4 x4 vib 2.5.5ct 6hz"), [Fault::VibratoMeasure("2.5.5ct".into())]);
        assert_eq!(refused("C4 x4 vib 20cents 6hz"), [Fault::VibratoMeasure("20cents".into())]);
        assert_eq!(refused("C4 x4 vib 6hz"), [Fault::VibratoMissing("ct")]);
        assert_eq!(refused("C4 x4 vib 20ct 250ms"), [Fault::VibratoMissing("hz")]);
        assert_eq!(refused("C4 x4 vib 20ct 6hz 30ct"), [Fault::VibratoTwice("30ct".into())]);
        assert_eq!(refused("C4 x4 arp"), [Fault::NoValue("arp".into())]);
        assert_eq!(refused("C4 x4 arp 0, 4, 7"), [Fault::BadArp("0,".into()), Fault::Leftover("4,".into()), Fault::Leftover("7".into())]);
        for junk in ["0,,7", "0,4,", "a,b", "0.4", "+-3,4", "0,99999"] { assert_eq!(refused(&format!("C4 x4 arp {junk}")), [Fault::BadArp(junk.into())], "{junk}"); }
        assert_eq!(refused("C4 x4 arp 7"), [Fault::LoneArp("7".into())]);
        assert_eq!(refused("C4 x4 from"), [Fault::NoValue("from".into())]);
        assert_eq!(refused("C4 x4 from a4"), [Fault::BadDeparture("a4".into())]);
        assert_eq!(refused("C4 x4 from C4"), [Fault::Standstill("C4".into())]);
        assert_eq!(refused("C4 x4 duty 33"), [Fault::BadDuty("33".into())]);
        assert_eq!(refused("C4 x4 vol 16"), [Fault::BadVolume("16".into())]);
        assert_eq!(refused("C4 x4 vol fade"), [Fault::NoValue("vol".into())]);
        assert_eq!(refused("C4 x4 vol 3 vol 4"), [Fault::RepeatedParam("vol".into())]);
        assert_eq!(refused("C4 x4 fade fade"), [Fault::RepeatedParam("fade".into())]);
        assert_eq!(refused("C4 x4 wobble"), [Fault::Leftover("wobble".into())]);
        let said: Vec<String> = Song::parse(&format!("{TWO_BARS}  1.1: C4 x4 arp 0, 4\n")).unwrap_err().iter().map(ToString::to_string).collect();
        assert_eq!(said[0], "line 4, pattern main, lane triangle, at 1.1: `0,` isn't an arp; write signed semitone offsets as one comma-joined word, like `arp 0,4,7`");
        assert_eq!(Fault::BadDuty("33".into()).to_string(), "duty `33` isn't a pulse width the chip has; pick `12.5`, `25`, `50`, or `75`");
    }

    #[test]
    fn vibrato_rates_and_delays_stay_inside_the_engine() {
        assert_eq!(refused("C4 x4 vib 20ct 21hz"), [Fault::RateUnreachable("21hz".into())]);
        assert_eq!(refused("C4 x4 vib 20ct 0.07hz"), [Fault::RateUnreachable("0.07hz".into())]);
        assert_eq!(refused("C4 x4 vib 20ct 15hz"), [Fault::RateBetween { word: "15hz".into(), faster: 1, slower: 2 }]);
        assert_eq!(Fault::RateBetween { word: "15hz".into(), faster: 1, slower: 2 }.to_string(), "`15hz` falls between the chip's vibrato rates 20 Hz and 10 Hz, more than 25% from either; ask for one of those");
        assert_eq!(adorned("C4 x4 vib 20ct 12.5hz").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 2, delay: 0 })), "10 Hz is 20% under the ask, inside the band");
        assert_eq!(adorned("C4 x4 vib 20ct 8.5hz").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 2, delay: 0 })), "8.5 sits nearer 10 than 6.67");
        assert_eq!(adorned("C4 x4 vib 20ct 8hz").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 3, delay: 0 })), "8 is 1.33 from 6.67 and 2 from 10");
        assert_eq!(adorned("C4 x4 vib 20ct 4.5hz").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 5, delay: 0 })), "a tie between 5 and 4 goes slower");
        assert_eq!(adorned("C4 x4 vib 20ct 20hz 4250ms").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 1, delay: 255 })));
        assert_eq!(refused("C4 x4 vib 20ct 6hz 4251ms"), [Fault::DelayUnreachable("4251ms".into())]);
        assert_eq!(adorned("C4 x4 vib 20ct 6hz 8ms").bend, Some(Bend::Vibrato(Vibrato { cents: 20.0, speed: 3, delay: 0 })), "8 ms rounds to frame 0");
    }

    #[test]
    fn composition_laws_are_checked_at_the_line() {
        assert_eq!(refused("C4 x4 vib 20ct 6hz arp 0,4"), [Fault::SecondBend { first: "vib", second: "arp" }]);
        assert_eq!(refused("C4 x4 from A3 vib 20ct 6hz"), [Fault::SecondBend { first: "from", second: "vib" }]);
        assert_eq!(adorned("C4 x4 arp 0,4,7 fade"), Ornaments { bend: Some(Bend::Arpeggio(vec![0, 4, 7])), fade: true, ..Ornaments::default() });
        assert_eq!(adorned("C4 x4 fade from G3"), Ornaments { bend: Some(Bend::Slide(55)), fade: true, ..Ornaments::default() });
        assert!(Song::parse(&format!("{TWO_BARS}  1.1: C4 x4 fade vib 10ct 5hz duty 25 vol 9\n")).is_ok(), "fade rides any pitch effect, and duty and vol ride anything");
        assert_eq!(Fault::SecondBend { first: "vib", second: "arp" }.to_string(), "`vib` and `arp` both bend this note's pitch; a note takes one pitch effect at most");
    }

    #[test]
    fn effects_on_rests_are_refused() {
        assert_eq!(refused("- x4 vib 20ct 6hz"), [Fault::RestEffect("vib")]);
        assert_eq!(refused("- x4 vol 3 fade"), [Fault::RestEffect("vol")]);
        assert_eq!(Fault::RestEffect("fade").to_string(), "`fade` on a rest; a rest has nothing to bend, shake, or fade");
        assert_eq!(Fault::RestEffect("fade").code(), "effect-on-rest");
    }
}
