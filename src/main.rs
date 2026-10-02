use clap::Parser;

mod check;
mod roll;
mod sfx;
#[allow(dead_code)] mod surface; // ※ nothing reaches it until the verbs are strung; wave 1 deletes this allow
mod track;

#[derive(Parser)]
#[command(name = "piro", version, about = "a chiptune instrument for agents")]
struct Cli {
    #[arg(long, global = true, help = "sentences instead of json")]
    human: bool,
    #[command(subcommand)]
    verb: Verb,
}

#[derive(clap::Subcommand)]
enum Verb {
    #[command(about = "render a grid to wav")]
    Track(track::Args),
    #[command(about = "render sound-effect params to wav")]
    Sfx(sfx::Args),
    #[command(about = "lint a grid and report render stats, no wav")]
    Check(check::Args),
    #[command(about = "draw a grid as a piano-roll png")]
    Roll(roll::Args),
}

fn main() {
    let cli = Cli::parse();
    let played = match cli.verb {
        Verb::Track(args) => track::run(args, cli.human),
        Verb::Sfx(args) => sfx::run(args, cli.human),
        Verb::Check(args) => check::run(args, cli.human),
        Verb::Roll(args) => roll::run(args, cli.human),
    };
    if let Err(e) = played { eprintln!("piro: {e}"); std::process::exit(1); }
}
