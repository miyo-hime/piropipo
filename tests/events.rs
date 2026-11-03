use std::process::Command;

const SCORE: &str = "song \"tally\" tempo 120\n\nplay\n  a\n  b +2\n  a\n\npattern a bars 1 grid 8\nlane pulse1 duty 25\n  1: C5 x2\n  3: - x2\n  5: E5 x4 vib 20ct 6hz\nlane noise\n  1: C4 x1 vol 8 fade\n  5: B4 x1\n\npattern b bars 1 beats 7 grid 14\nlane triangle\n  1: A2 x7\n  8: E3 x7 arp 0,7\n";

fn piro(name: &str, args: &[&str]) -> String {
    let score = std::env::temp_dir().join(format!("piro-events-{name}-{}.piro", std::process::id()));
    std::fs::write(&score, SCORE).expect("temp dir should be writable");
    let out = Command::new(env!("CARGO_BIN_EXE_piro")).args(args).arg(&score).output().expect("piro should run");
    std::fs::remove_file(&score).ok();
    assert!(out.status.success(), "piro {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("piro speaks utf-8")
}

fn spliced<'a>(without: &'a str, with: &'a str) -> &'a str {
    let seam = without.rfind(",\"errors\":[]}").expect("a passing check ends on its empty errors");
    let (head, tail) = without.split_at(seam);
    let inserted = with.strip_prefix(head).and_then(|rest| rest.strip_suffix(tail)).unwrap_or_else(|| panic!("--events moved bytes it had no business touching:\n{without}\n{with}"));
    inserted.strip_prefix(",\"events\":").expect("the only new bytes are the events key")
}

#[test]
fn the_flag_only_adds_bytes_and_never_moves_one() {
    for trim in [&[][..], &["--summary"][..]] {
        let plain = piro("plain", &[&["check"][..], trim].concat());
        let evented = piro("evented", &[&["check", "--events"][..], trim].concat());
        assert!(!plain.contains("\"events\""), "{trim:?}: no flag, no events key");
        let events: serde_json::Value = serde_json::from_str(spliced(&plain, &evented)).expect("the inserted events are json");
        let heard: Vec<(String, u64, String, String)> = events.as_array().expect("events is an array").iter().map(|e| (e["pattern"].as_str().unwrap_or_default().to_owned(), e["play"].as_u64().unwrap_or_default(), e["lane"].as_str().unwrap_or_default().to_owned(), e["pitch"].as_str().unwrap_or_default().to_owned())).collect();
        let strike = |p: &str, n: u64, l: &str, pitch: &str| (p.to_owned(), n, l.to_owned(), pitch.to_owned());
        assert_eq!(heard, [strike("a", 1, "pulse1", "C5"), strike("a", 1, "noise", "C"), strike("a", 1, "pulse1", "E5"), strike("a", 1, "noise", "B"), strike("b", 1, "triangle", "B2"), strike("b", 1, "triangle", "F#3"), strike("a", 2, "pulse1", "C5"), strike("a", 2, "noise", "C"), strike("a", 2, "pulse1", "E5"), strike("a", 2, "noise", "B")], "{trim:?}");
    }
    let keys: Vec<String> = serde_json::from_str::<serde_json::Value>(&piro("keys", &["check"])).expect("check speaks json").as_object().expect("an envelope").keys().cloned().collect();
    assert_eq!(keys, ["ok", "verb", "output", "stats", "warnings", "harmony", "errors"]);
}

#[test]
fn the_human_report_gains_one_counting_line() {
    let plain = piro("human", &["--human", "check"]);
    let evented = piro("human-evented", &["--human", "check", "--events"]);
    let added = evented.strip_prefix(plain.trim_end_matches('\n')).expect("the human report keeps every line it had");
    assert_eq!(added, "\nevents: 10 notes struck; the list prints in the json only, so drop --human to read it\n");
}
