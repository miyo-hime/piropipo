use piropipo::{render, score::Song};

const SPEC: &str = include_str!("../docs/format.md");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let score = SPEC.split_once("```\n").and_then(|(_, opened)| opened.split_once("```")).ok_or("format.md lost its example")?.0;
    let song = Song::parse(score).map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))?;
    let path = "/tmp/first-light.wav";
    let spec = hound::WavSpec { channels: 1, sample_rate: u32::from(render::SAMPLE_RATE), bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut wav = hound::WavWriter::create(path, spec)?;
    for pattern in &song.patterns {
        for s in render::pattern(&song, pattern)? { wav.write_sample(s)?; }
    }
    let seconds = f64::from(wav.duration()) / f64::from(render::SAMPLE_RATE);
    wav.finalize()?;
    println!("\"{}\", {seconds:.3}s of it, is waiting at {path}", song.title);
    println!("go listen. i can only count the samples");
    Ok(())
}
