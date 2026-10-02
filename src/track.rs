use crate::listen::{self, Remark, Take};
use crate::surface;
use piropipo::render::SAMPLE_RATE;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
    #[arg(short, long, help = "where the wav lands (default: beside the input)")]
    pub output: Option<PathBuf>,
}

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> { listen::answer("track", human, press(args)) }

fn press(args: Args) -> Result<(Option<PathBuf>, Take), Vec<Remark>> {
    let origin = (args.input != "-").then(|| Path::new(&args.input));
    let dest = surface::land_beside(origin, args.output, "wav").map_err(|why| vec![Remark::loose("needs-output", why)])?;
    if origin.is_some_and(|src| same_file(src, &dest)) { return Err(vec![Remark::loose("output-is-input", format!("the wav would land on top of the input {}; pick another -o", dest.display()))]); }
    let take = listen::take(&listen::read(&args.input)?)?;
    publish(&dest, &take.samples).map_err(|e| vec![Remark::loose("unwritable-output", format!("can't write {}: {e}", dest.display()))])?;
    Ok((Some(dest), take))
}

fn same_file(a: &Path, b: &Path) -> bool { std::fs::canonicalize(a).ok().is_some_and(|a| std::fs::canonicalize(b).ok() == Some(a)) }

fn publish(dest: &Path, samples: &[i16]) -> std::io::Result<()> {
    surface::publish(dest, &surface::pressed(samples, u32::from(SAMPLE_RATE)).map_err(std::io::Error::other)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("piro-track-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn stdin_without_o_is_refused_before_reading_anything() {
        let Err(errors) = press(Args { input: "-".into(), output: None }) else { panic!("stdin with no -o should refuse") };
        assert_eq!(errors.iter().map(|e| e.code).collect::<Vec<_>>(), ["needs-output"]);
    }

    #[test]
    fn the_wav_lands_beside_the_score_whole() {
        let dir = scratch("beside");
        let score = dir.join("ding.piro");
        std::fs::write(&score, "song \"ding\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C5 x2\n").unwrap();
        let (output, take) = press(Args { input: score.to_string_lossy().into(), output: None }).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(output.as_deref(), Some(dir.join("ding.wav").as_path()));
        let read: Vec<i16> = hound::WavReader::open(dir.join("ding.wav")).unwrap().into_samples().map(Result::unwrap).collect();
        assert_eq!(read, take.samples);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "a draft was left behind");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_broken_grid_never_touches_the_last_good_wav() {
        let dir = scratch("broken");
        let score = dir.join("ding.piro");
        std::fs::write(dir.join("ding.wav"), b"the last good take").unwrap();
        std::fs::write(&score, "song \"ding\" tempo 120\npattern p bars 1 grid 4\nlane pulse1\n  1: C5 x9\n").unwrap();
        let Err(errors) = press(Args { input: score.to_string_lossy().into(), output: None }) else { panic!("x9 runs past a 4-slot pattern") };
        assert_eq!(errors[0].code, "runs-past-end");
        assert_eq!(std::fs::read(dir.join("ding.wav")).unwrap(), b"the last good take");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_score_is_never_overwritten_by_its_own_wav() {
        let dir = scratch("self");
        let score = dir.join("ding.piro");
        std::fs::write(&score, "song \"ding\" tempo 120\n").unwrap();
        let Err(errors) = press(Args { input: score.to_string_lossy().into(), output: Some(score.clone()) }) else { panic!("-o onto the input should refuse") };
        assert_eq!(errors[0].code, "output-is-input");
        assert_eq!(std::fs::read_to_string(&score).unwrap(), "song \"ding\" tempo 120\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_failed_publish_leaves_no_draft() {
        let dir = scratch("nowhere");
        assert!(publish(&dir.join("missing").join("x.wav"), &[1, 2, 3]).is_err());
        assert!(publish(&dir, &[1, 2, 3]).is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
