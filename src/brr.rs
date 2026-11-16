// the snes keeps its samples as brr: 9-byte blocks, each a header (a shift, one of four predictors, a loop bit and an end bit)
// and sixteen 4-bit residuals. this is our own codec: the decoder below is the one the encoder simulates, so what it measures is what plays

pub const BLOCK: usize = 16;

pub const BLOCK_BYTES: usize = 9;

const END: u8 = 0b01;

const LOOP: u8 = 0b10;

fn predict(filter: u8, p1: i32, p2: i32) -> i32 {
    match filter {
        0 => 0,
        1 => p1 + ((-p1) >> 4),
        2 => (p1 << 1) + ((-(p1 * 3)) >> 5) - p2 + (p2 >> 4),
        _ => (p1 << 1) + ((-(p1 * 13)) >> 6) - p2 + ((p2 * 3) >> 4),
    }
}

fn expand(nibble: i32, shift: u8) -> i32 { if shift <= 12 { (nibble << shift) >> 1 } else if nibble < 0 { -2048 } else { 0 } }

fn clamp(s: i32) -> i32 { s.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) }

fn signed(nibble: u8) -> i32 { i32::from(nibble as i8) << 28 >> 28 }

pub fn decode(brr: &[u8]) -> Vec<i16> {
    let (mut p1, mut p2) = (0, 0);
    let mut pcm = Vec::with_capacity(brr.len() / BLOCK_BYTES * BLOCK);
    for block in brr.chunks_exact(BLOCK_BYTES) {
        let (shift, filter) = (block[0] >> 4, (block[0] >> 2) & 3);
        for byte in &block[1..] {
            for nibble in [byte >> 4, byte & 0xF] {
                let s = clamp(expand(signed(nibble), shift) + predict(filter, p1, p2));
                (p2, p1) = (p1, s);
                pcm.push(s as i16);
            }
        }
    }
    pcm
}

// one block, every shift tried under the given predictors; each residual is rounded against the history the decoder will really have
fn fit(target: &[i32; BLOCK], p1: i32, p2: i32, filters: &[u8]) -> (u8, u8, [i32; BLOCK], i64) {
    let mut best = (0, 0, [0; BLOCK], i64::MAX);
    for &filter in filters {
        for shift in 0..=12u8 {
            let (mut q1, mut q2, mut error) = (p1, p2, 0i64);
            let mut nibbles = [0; BLOCK];
            for (k, &want) in target.iter().enumerate() {
                let guess = predict(filter, q1, q2);
                let step = f64::from(1i32 << shift) / 2.0;
                let n = (f64::from(want - guess) / step).round().clamp(-8.0, 7.0) as i32;
                let s = clamp(expand(n, shift) + guess);
                error += i64::from(want - s).pow(2);
                nibbles[k] = n;
                (q2, q1) = (q1, s);
            }
            if error < best.3 { best = (shift, filter, nibbles, error); }
        }
    }
    best
}

// `pcm` must be whole blocks; `restart` is the block a loop jumps back to, and it, like the first block, is coded without prediction so it decodes the same whatever came before it
pub fn encode(pcm: &[i16], restart: Option<usize>) -> Vec<u8> {
    assert!(pcm.len().is_multiple_of(BLOCK) && !pcm.is_empty(), "brr wants whole 16-sample blocks");
    let blocks = pcm.len() / BLOCK;
    let (mut p1, mut p2) = (0, 0);
    let mut brr = Vec::with_capacity(blocks * BLOCK_BYTES);
    for (i, chunk) in pcm.chunks_exact(BLOCK).enumerate() {
        let target: [i32; BLOCK] = std::array::from_fn(|k| i32::from(chunk[k]));
        let fresh = i == 0 || restart == Some(i);
        let (shift, filter, nibbles, _) = fit(&target, p1, p2, if fresh { &[0] } else { &[0, 1, 2, 3] });
        let last = i + 1 == blocks;
        let flags = if last { END | if restart.is_some() { LOOP } else { 0 } } else { 0 };
        brr.push(shift << 4 | filter << 2 | flags);
        for pair in nibbles.chunks_exact(2) { brr.push(((pair[0] as u8 & 0xF) << 4) | (pair[1] as u8 & 0xF)); }
        for &n in &nibbles {
            let s = clamp(expand(n, shift) + predict(filter, p1, p2));
            (p2, p1) = (p1, s);
        }
    }
    brr
}

pub fn loops(brr: &[u8]) -> bool { brr.len() >= BLOCK_BYTES && brr[brr.len() - BLOCK_BYTES] & LOOP != 0 }

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(len: usize, period: f64, peak: f64) -> Vec<i16> { (0..len).map(|i| (peak * (std::f64::consts::TAU * i as f64 / period).sin()).round() as i16).collect() }

    fn snr(a: &[i16], b: &[i16]) -> f64 {
        let signal: f64 = a.iter().map(|s| f64::from(*s).powi(2)).sum();
        let noise: f64 = a.iter().zip(b).map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2)).sum();
        10.0 * (signal / noise.max(1.0)).log10()
    }

    #[test]
    fn a_block_is_nine_bytes_for_sixteen_samples() {
        let brr = encode(&tone(64, 32.0, 12_000.0), None);
        assert_eq!(brr.len(), 4 * BLOCK_BYTES);
        assert_eq!(decode(&brr).len(), 64);
        assert_eq!(brr[27] & END, END, "the last block carries the end bit");
        assert!(brr[..27].chunks(9).all(|b| b[0] & END == 0));
        assert!(!loops(&brr));
        assert!(loops(&encode(&tone(64, 32.0, 12_000.0), Some(2))));
    }

    #[test]
    fn smooth_tones_survive_four_bits_with_prediction() {
        for period in [16.0, 61.0, 200.0, 1000.0] {
            let pcm = tone(4096, period, 14_000.0);
            let back = decode(&encode(&pcm, None));
            let quality = snr(&pcm, &back);
            assert!(quality > 30.0, "a sine of period {period} comes back at {quality:.1} db snr");
        }
    }

    #[test]
    fn the_restart_block_and_the_first_decode_without_history() {
        let pcm = tone(512, 37.0, 9_000.0);
        let brr = encode(&pcm, Some(8));
        assert_eq!((brr[0] >> 2) & 3, 0);
        assert_eq!((brr[8 * BLOCK_BYTES] >> 2) & 3, 0);
        // decoding the loop on its own gives the same samples as decoding it in place: a loop sounds the same on every pass
        let alone = decode(&brr[8 * BLOCK_BYTES..]);
        assert_eq!(alone, decode(&brr)[8 * BLOCK..]);
    }

    #[test]
    fn silence_and_the_rails_decode_exactly() {
        assert_eq!(decode(&encode(&[0; 32], None)), [0; 32]);
        let rails: Vec<i16> = (0..32).map(|i| if i % 2 == 0 { 16_000 } else { -16_000 }).collect();
        let back = decode(&encode(&rails, None));
        assert!(back.iter().zip(&rails).all(|(a, b)| (i32::from(*a) - i32::from(*b)).abs() <= 2048), "{back:?}");
        // the coarsest step is 2048, and a lone +16000 can only reach 14336: four bits are four bits
    }

    #[test]
    fn decode_matches_the_format_by_hand() {
        // shift 12, filter 0: nibble 7 is 7 << 12 >> 1 = 14336, nibble 8 (-8) is -16384
        let block = [0xC0, 0x78, 0, 0, 0, 0, 0, 0, 0x01];
        let pcm = decode(&block);
        assert_eq!(&pcm[..3], &[14_336, -16_384, 0]);
    }
}
