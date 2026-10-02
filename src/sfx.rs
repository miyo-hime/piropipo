use std::path::PathBuf;

#[derive(clap::Args)]
pub struct Args {
    #[arg(help = "coin, jump, hurt, explosion, powerup, blip, laser, or custom")]
    pub preset: String,
    #[arg(short, long, help = "where the wav lands (default: <preset>.wav in cwd)")]
    pub output: Option<PathBuf>,
}

pub fn run(_args: Args, _human: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("the sfx verb isn't strung yet (board: T2)".into())
}
