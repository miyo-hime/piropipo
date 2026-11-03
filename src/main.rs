use clap::Parser;

mod check;
mod events;
mod harmony;
mod listen;
mod roll;
mod sfx;
mod sketch;
mod surface;
mod track;
mod vu;

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
    #[command(about = "lint a grid, report render stats and a harmony advisory, no wav")]
    Check(check::Args),
    #[command(about = "draw a grid as a piano-roll png")]
    Roll(roll::Args),
    #[command(about = "place a sketch's flowing lane notes and write the positioned .piro grid")]
    Sketch(sketch::Args),
}

fn main() {
    // rust ships SIGPIPE ignored; restore the unix default so piping into head doesn't panic
    #[cfg(unix)] unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL); }
    let cli = Cli::parse();
    let played = match cli.verb {
        Verb::Track(args) => track::run(args, cli.human),
        Verb::Sfx(args) => sfx::run(args, cli.human),
        Verb::Check(args) => check::run(args, cli.human),
        Verb::Roll(args) => roll::run(args, cli.human),
        Verb::Sketch(args) => sketch::run(args, cli.human),
    };
    if let Err(e) = played { eprintln!("piro: {e}"); std::process::exit(1); }
}
