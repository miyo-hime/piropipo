use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn pressed(samples: &[i16], rate: u32) -> Result<Vec<u8>, hound::Error> {
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut disc = std::io::Cursor::new(Vec::new());
    let mut wav = hound::WavWriter::new(&mut disc, spec)?;
    for &s in samples { wav.write_sample(s)?; }
    wav.finalize()?;
    Ok(disc.into_inner())
}

pub fn publish(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = dest.file_name().ok_or_else(|| std::io::Error::other("the output path names no file"))?;
    let draft = dest.with_file_name(format!(".{}.{}.part", name.to_string_lossy(), std::process::id()));
    let landed = std::fs::File::create(&draft).and_then(|mut f| { f.write_all(bytes)?; f.sync_all() }).and_then(|()| std::fs::rename(&draft, dest));
    if landed.is_err() { let _ = std::fs::remove_file(&draft); }
    landed
}

pub fn read_source(input: &str) -> Result<(String, Option<PathBuf>), std::io::Error> {
    if input == "-" {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        Ok((text, None))
    } else {
        Ok((std::fs::read_to_string(input)?, Some(PathBuf::from(input))))
    }
}

pub fn land_beside(origin: Option<&Path>, chosen: Option<PathBuf>, ext: &str) -> Result<PathBuf, String> {
    match (chosen, origin) {
        (Some(p), _) => Ok(p),
        (None, Some(src)) => Ok(src.with_extension(ext)),
        (None, None) => Err(format!("stdin input needs -o to say where the .{ext} lands")),
    }
}

pub fn emit(human: bool, json: serde_json::Value, sentence: &str) {
    if human { println!("{sentence}"); } else { println!("{json}"); }
}
