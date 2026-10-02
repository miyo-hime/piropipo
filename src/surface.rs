use std::io::Read;
use std::path::{Path, PathBuf};

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
