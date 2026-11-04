use crate::events;
use crate::listen::{self, Detail, Remark, Take};
use piropipo::score::{Lane, Pattern, Position, Song};
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
    #[arg(short, long, hide = true)]
    pub output: Option<PathBuf>,
    #[arg(long, help = "also refuse any slot no note or written rest covers")]
    pub strict: bool,
    #[arg(long, help = "only the headline (duration, peak, saturation) plus any warnings; skips the loudness curve, lane activity, and per-lane notes")]
    pub summary: bool,
    #[arg(long, help = "also list every note as it plays, in song order with transposes applied: pitch, bar.slot, seconds and wav samples, volume and duty as heard, effects; written rests get a list of their own. json only (--human just counts them); pairs with --summary for a lean payload, which drops the sections table")]
    pub events: bool,
}

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> { listen::answer("check", human, if args.summary { Detail::Headline } else { Detail::Everything }, rehearse(args)) }

fn rehearse(args: Args) -> Result<(Option<PathBuf>, Take), Vec<Remark>> {
    if let Some(wanted) = args.output { return Err(vec![Remark::loose("check-writes-nothing", format!("check writes no file, so -o {} has nowhere to go; `piro track` writes the wav", wanted.display()))]); }
    let (output, mut take) = inspect(&listen::read(&args.input)?, args.strict)?;
    if args.events { take.strikes = Some(events::strikes(&take)); }
    Ok((output, take))
}

fn inspect(text: &str, strict: bool) -> Result<(Option<PathBuf>, Take), Vec<Remark>> {
    let take = listen::take(text)?;
    if strict {
        let song = Song::parse(text).expect("take already parsed this text");
        let gaps: Vec<Remark> = song.patterns.iter().flat_map(|p| p.lanes.iter().flat_map(|l| unwritten(p, l))).collect();
        if !gaps.is_empty() { return Err(gaps); }
    }
    Ok((None, take))
}

fn unwritten(pattern: &Pattern, lane: &Lane) -> Vec<Remark> {
    let total = u64::from(pattern.bars) * u64::from(pattern.grid);
    let mut spans: Vec<(u64, u64)> = lane.notes.iter().map(|n| { let start = n.at.tick(pattern.grid); (start, start + u64::from(n.len)) }).collect();
    spans.sort_unstable();
    let mut gaps = Vec::new();
    let mut cursor = 0;
    for (start, end) in spans.into_iter().chain(std::iter::once((total, total))) {
        if start > cursor { gaps.push((cursor, start)); }
        cursor = cursor.max(end);
    }
    gaps.into_iter().map(|(from, to)| {
        let first = Position::from_tick(from, pattern.grid);
        let last = Position::from_tick(to - 1, pattern.grid);
        let slots = to - from;
        let message = if slots == 1 { format!("slot {first} is unwritten; nothing plays or rests there, so write a note or a rest like `{first}: - x1`") } else { format!("slots {first} through {last} are unwritten; nothing plays or rests there, so write notes or a rest like `{first}: - x{slots}`") };
        Remark { code: "unwritten-silence", message, pattern: Some(pattern.name.clone()), lane: Some(lane.name.clone()), at: Some(first), line: Some(lane.line) }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_is_refused_not_ignored() {
        let Err(errors) = rehearse(Args { input: "song.piro".into(), output: Some("song.wav".into()), strict: false, summary: false, events: false }) else { panic!("check -o should refuse") };
        assert_eq!(errors.iter().map(|e| e.code).collect::<Vec<_>>(), ["check-writes-nothing"]);
    }

    fn strict(text: &str) -> Result<(Option<PathBuf>, Take), Vec<Remark>> { inspect(text, true) }

    const HEAD: &str = "song \"s\" tempo 120\npattern p bars 1 grid 16\nlane pulse1\n";

    #[test]
    fn a_fully_written_lane_passes_strict() {
        assert!(strict(&format!("{HEAD}  1.1: C4 x8\n  1.9: - x8\n")).is_ok());
    }

    #[test]
    fn a_gap_mid_lane_errors_per_gap_with_its_span() {
        let Err(errors) = strict(&format!("{HEAD}  1.1: C4 x2\n  1.5: D4 x4\n  1.10: E4 x1\n  1.12: F4 x5\n")) else { panic!("gaps should refuse") };
        assert_eq!(errors.iter().map(|e| e.code).collect::<Vec<_>>(), ["unwritten-silence"; 3]);
        let spans: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        assert!(spans[0].starts_with("slots 1.3 through 1.4 are unwritten"), "{spans:?}");
        assert!(spans[1].starts_with("slot 1.9 is unwritten"), "{spans:?}");
        assert!(spans[2].starts_with("slot 1.11 is unwritten"), "{spans:?}");
        assert_eq!((errors[0].pattern.as_deref(), errors[0].lane.as_deref(), errors[0].at), (Some("p"), Some("pulse1"), Some(Position { bar: 1, slot: 3 })));
    }

    #[test]
    fn a_trailing_gap_runs_to_the_end_of_the_pattern() {
        let Err(errors) = strict(&format!("{HEAD}  1.1: C4 x4\n")) else { panic!("tail should refuse") };
        assert!(errors[0].message.starts_with("slots 1.5 through 1.16 are unwritten"), "{errors:?}");
    }

    #[test]
    fn an_empty_lane_is_one_gap_across_the_pattern() {
        let Err(errors) = strict("song \"s\" tempo 120\npattern p bars 2 grid 8\nlane pulse1\n  1.1: C4 x16\nlane noise\n") else { panic!("empty lane should refuse") };
        assert_eq!(errors.len(), 1);
        assert_eq!((errors[0].lane.as_deref(), errors[0].at), (Some("noise"), Some(Position { bar: 1, slot: 1 })));
        assert!(errors[0].message.starts_with("slots 1.1 through 2.8 are unwritten"), "{errors:?}");
    }

    #[test]
    fn strict_counts_slots_whatever_the_meter() {
        let Err(errors) = strict("song \"s\" tempo 120\npattern p bars 2 beats 7 grid 14\nlane pulse1\n  1.1: C4 x7\n  2.1: D4 x14\n") else { panic!("half a bar is unwritten") };
        assert_eq!(errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(), ["slots 1.8 through 1.14 are unwritten; nothing plays or rests there, so write notes or a rest like `1.8: - x7`"]);
    }

    #[test]
    fn without_strict_the_gappy_score_still_passes() {
        assert!(inspect(&format!("{HEAD}  1.1: C4 x2\n"), false).is_ok());
    }

    #[test]
    fn the_envelope_error_carries_its_place() {
        let Err(errors) = strict(&format!("{HEAD}  1.1: C4 x2\n")) else { panic!("gap should refuse") };
        let at = errors[0].at.map(|a| a.to_string());
        assert_eq!((errors[0].pattern.as_deref(), errors[0].lane.as_deref(), at.as_deref(), errors[0].line), (Some("p"), Some("pulse1"), Some("1.3"), Some(3)));
    }
}
