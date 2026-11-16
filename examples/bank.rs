// writes the snes bank: every recipe in src/recipes.rs, brr-coded into src/bank/. run it after changing a recipe; the bank test
// refuses any file that isn't exactly what its recipe makes
use piropipo::{brr, recipes};

fn main() -> std::io::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bank");
    std::fs::create_dir_all(&dir)?;
    let mut total = 0;
    for made in recipes::all() {
        let bytes = brr::encode(&made.pcm, made.restart.map(|r| r / brr::BLOCK));
        total += bytes.len();
        let peak = made.pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        println!("{:<10} {:>6} bytes  {:>6} samples  restart {:<6}  peak {peak:>5}  first 250 ms rms {:>5.0}  {:?}", made.name, bytes.len(), made.pcm.len(), made.restart.map_or("-".into(), |r| r.to_string()), recipes::body_rms(&made.pcm, made.restart), made.tune);
        std::fs::write(dir.join(format!("{}.brr", made.name)), bytes)?;
    }
    println!("{total} bytes in all");
    Ok(())
}
