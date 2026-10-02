#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "a .piro grid, or - for stdin")]
    pub input: String,
}

pub fn run(_args: Args, _human: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("the check verb isn't strung yet (board: T4)".into())
}
