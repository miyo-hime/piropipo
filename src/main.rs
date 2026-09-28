const HELLO: &str = r#"
#TITLE victory fanfare (ff7, archeage mml transcription)
@v0 = { 12 14 15 14 13 13 12 12 12 }
A t107 @2 @v0 o5 c16.c16c16.c4 <g+4a+4> c8&c32 <a+16.> c2.
B t107 v6 o4 c16.e16g16.>c16.<g16e16.c+16.e16g+16.a+16.>d16f16.<f16.g+16a+16.>c16.
C t107 o3 c4c4 g+4a+4 f4 c2.
"#;

fn write_wav(path: &str, samples: impl Iterator<Item = i16>) -> Result<(), Box<dyn std::error::Error>> {
    let spec = hound::WavSpec { channels: 1, sample_rate: 44100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in samples { w.write_sample(s)?; }
    w.finalize()?;
    println!("piropipo: wrote {path}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let music = HELLO.parse::<ffmml::Music>()?;
    let mut player = music.play(44100);
    let tune: Vec<i16> = (&mut player).map(|s| s.to_i16()).collect();
    if let Some(e) = player.take_last_error() { return Err(e.to_string().into()); }
    write_wav("/tmp/piropipo-hello.wav", tune.into_iter())?;

    let mut coin = vec![0f32; 44100];
    sfxr::Generator::new(sfxr::Sample::pickup(Some(9))).generate(&mut coin);
    write_wav("/tmp/piropipo-coin.wav", coin.into_iter().map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16))?;
    Ok(())
}
