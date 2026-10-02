use crate::listen::{self, Remark, Take};
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
    #[arg(short, long, hide = true)]
    pub output: Option<PathBuf>,
}

pub fn run(args: Args, human: bool) -> Result<(), Box<dyn std::error::Error>> { listen::answer("check", human, rehearse(args)) }

fn rehearse(args: Args) -> Result<(Option<PathBuf>, Take), Vec<Remark>> {
    if let Some(wanted) = args.output { return Err(vec![Remark::loose("check-writes-nothing", format!("check writes no file, so -o {} has nowhere to go; `piro track` writes the wav", wanted.display()))]); }
    Ok((None, listen::take(&listen::read(&args.input)?)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_is_refused_not_ignored() {
        let Err(errors) = rehearse(Args { input: "song.piro".into(), output: Some("song.wav".into()) }) else { panic!("check -o should refuse") };
        assert_eq!(errors.iter().map(|e| e.code).collect::<Vec<_>>(), ["check-writes-nothing"]);
    }
}
