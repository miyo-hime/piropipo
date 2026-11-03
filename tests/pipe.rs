#![cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

#[test]
fn check_into_a_closed_pipe_dies_quietly() {
    let score = std::env::temp_dir().join(format!("piro-pipe-{}.piro", std::process::id()));
    std::fs::write(&score, "song \"pipe\" tempo 120 profile nes\n\nplay\n  a\n\npattern a bars 1 grid 16\n\nlane pulse1 duty 50 vol 8\n  1.1: C4 x4\n  1.5: - x12\n").expect("temp dir should be writable");
    let mut child = Command::new(env!("CARGO_BIN_EXE_piro")).arg("check").arg(&score).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("piro should spawn");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("piro should finish");
    std::fs::remove_file(&score).ok();
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    assert!(out.status.success() || out.status.signal() == Some(libc::SIGPIPE));
}
