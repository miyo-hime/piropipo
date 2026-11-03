use std::io::Write;
use std::process::{Command, Output, Stdio};

const SKETCH: &str = "song \"pipe\" tempo 120\npattern a bars 1 grid 16\nlane pulse1\n  C4 x4\n  - x4\n  1.13: E4 x4\n";

const INKED: &str = "song \"pipe\" tempo 120\npattern a bars 1 grid 16\nlane pulse1\n  1.1:  C4 x4\n  1.5:  - x4\n  1.13: E4 x4\n";

fn piro(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_piro")).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("piro should spawn");
    // piro may refuse before it reads a byte, and then this pipe is already closed
    let _ = child.stdin.take().expect("stdin is piped").write_all(stdin);
    child.wait_with_output().expect("piro should finish")
}

fn report(bytes: &[u8]) -> serde_json::Value { serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(bytes))) }

#[test]
fn dash_o_dash_streams_the_score_alone_and_moves_the_report_to_stderr() {
    let inked = piro(&["sketch", "-", "-o", "-"], SKETCH.as_bytes());
    assert!(inked.status.success(), "{}", String::from_utf8_lossy(&inked.stderr));
    assert_eq!(String::from_utf8_lossy(&inked.stdout), INKED);
    let told = report(&inked.stderr);
    assert_eq!((told["ok"].as_bool(), told["output"].as_str(), told["stats"]["notes"].as_u64()), (Some(true), Some("-"), Some(3)));
    let checked = piro(&["check", "--summary", "-"], &inked.stdout);
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stdout));
}

#[test]
fn a_refused_stream_leaves_stdout_empty() {
    let refused = piro(&["sketch", "-", "-o", "-"], b"song \"pipe\" tempo 120\npattern a bars 1 grid 4\nlane pulse1\n  C4 x5\n");
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("\"code\":\"runs-past-end\""));
}

#[test]
fn stdin_without_o_is_refused_on_stdout_like_every_verb() {
    let refused = piro(&["sketch", "-"], SKETCH.as_bytes());
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(report(&refused.stdout)["errors"][0]["code"], "no-output");
}
