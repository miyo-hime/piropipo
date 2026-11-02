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

pub fn land_beside(origin: Option<&Path>, chosen: Option<PathBuf>, ext: &str) -> Result<PathBuf, (&'static str, String)> {
    let target = match (chosen, origin) {
        (Some(p), _) => p,
        (None, Some(src)) => src.with_extension(ext),
        (None, None) => return Err(("no-output", format!("stdin input needs -o to say where the .{ext} lands"))),
    };
    if origin.is_some_and(|src| same_file(src, &target)) { return Err(("output-eats-input", format!("{} would overwrite the input score; pick another -o", target.display()))); }
    Ok(target)
}

fn same_file(origin: &Path, target: &Path) -> bool {
    let Ok(origin_real) = std::fs::canonicalize(origin) else { return origin == target };
    let target_real = std::fs::canonicalize(target).ok().or_else(|| {
        let name = target.file_name()?;
        let dir = target.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        Some(std::fs::canonicalize(dir).ok()?.join(name))
    });
    target_real.map_or(origin == target, |real| real == origin_real)
}

pub fn emit(human: bool, json: serde_json::Value, sentence: &str) {
    if human { println!("{sentence}"); } else { println!("{json}"); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("piro-surface-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dash_o_onto_the_input_is_refused() {
        let dir = scratch("dash-o");
        let score = dir.join("song.piro");
        std::fs::write(&score, "keep me").unwrap();
        let respelled = dir.join(".").join("song.piro");
        for chosen in [score.clone(), respelled] {
            let Err((code, why)) = land_beside(Some(&score), Some(chosen), "piro") else { panic!("-o onto the input should refuse") };
            assert_eq!(code, "output-eats-input");
            assert!(why.contains("overwrite the input"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_score_named_like_its_output_refuses_the_default_path() {
        let dir = scratch("default");
        let score = dir.join("song.png");
        std::fs::write(&score, "keep me").unwrap();
        let Err((code, _)) = land_beside(Some(&score), None, "png") else { panic!("the default path would clobber the input") };
        assert_eq!(code, "output-eats-input");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_fresh_target_and_stdin_rules_still_hold() {
        let dir = scratch("fresh");
        let score = dir.join("song.piro");
        std::fs::write(&score, "keep me").unwrap();
        assert_eq!(land_beside(Some(&score), None, "png").unwrap(), dir.join("song.png"));
        assert_eq!(land_beside(None, None, "png").unwrap_err().0, "no-output");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
