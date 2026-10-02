use std::path::PathBuf;

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
    #[arg(short, long, help = "where the wav lands (default: beside the input)")]
    pub output: Option<PathBuf>,
}

pub fn run(_args: Args, _human: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("the track verb isn't strung yet (board: T3)".into())
}
