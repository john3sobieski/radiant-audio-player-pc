//! 16-bit PCM stereo WAV writer (44.1 kHz stems). Written to `<name>.part` and renamed at the end, so a cancelled or
//! failed run never leaves a half-written `.wav` that looks finished.

use crate::StemError;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Frames per block while writing.
const BLOCK: usize = 65_536;

fn io(path: &Path, e: std::io::Error) -> StemError {
    StemError::Io(format!("{}: {e}", path.display()))
}

/// Header of a canonical 44-byte PCM WAV (`data_bytes` of samples follow).
pub fn header(rate: u32, channels: u16, data_bytes: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    let block = channels * 2;
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * block as u32).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// Float sample -> 16-bit (clipped, rounded).
pub fn to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Writes `frames` stereo frames. `fill(start, block)` must fill `block` (interleaved L, R; `block.len() / 2`
/// frames) with the audio that starts at frame `start`.
pub fn write_stereo16(
    path: &Path,
    rate: u32,
    frames: usize,
    cancel: &AtomicBool,
    mut fill: impl FnMut(usize, &mut [f32]),
) -> Result<(), StemError> {
    let data_bytes = u32::try_from(frames as u64 * 4).map_err(|_| StemError::Io("the track is too long for a WAV file".into()))?;
    let part = path.with_extension("wav.part");
    let result = (|| {
        let mut w = BufWriter::new(File::create(&part).map_err(|e| io(&part, e))?);
        w.write_all(&header(rate, 2, data_bytes)).map_err(|e| io(&part, e))?;
        let mut block = vec![0.0_f32; BLOCK * 2];
        let mut bytes: Vec<u8> = Vec::with_capacity(BLOCK * 4);
        let mut start = 0;
        while start < frames {
            if cancel.load(Ordering::Relaxed) {
                return Err(StemError::Cancelled);
            }
            let n = BLOCK.min(frames - start);
            fill(start, &mut block[..n * 2]);
            bytes.clear();
            for v in &block[..n * 2] {
                bytes.extend_from_slice(&to_i16(*v).to_le_bytes());
            }
            w.write_all(&bytes).map_err(|e| io(&part, e))?;
            start += n;
        }
        w.flush().map_err(|e| io(&part, e))?;
        Ok(())
    })();
    match result {
        Ok(()) => fs::rename(&part, path).map_err(|e| io(path, e)),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_canonical() {
        let h = header(44_100, 2, 400);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 436);
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 44_100);
        assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 176_400);
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 400);
    }

    #[test]
    fn samples_are_clipped_and_rounded() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), 32767);
        assert_eq!(to_i16(-2.0), -32767);
        assert_eq!(to_i16(0.5), 16384);
    }

    #[test]
    fn file_has_the_right_size_and_no_part_left() {
        let dir = std::env::temp_dir().join(format!("radiant-wav-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        let cancel = AtomicBool::new(false);
        let frames = BLOCK + 123;
        write_stereo16(&path, 44_100, frames, &cancel, |start, out| {
            for (i, f) in out.chunks_exact_mut(2).enumerate() {
                let v = ((start + i) % 100) as f32 / 200.0;
                f[0] = v;
                f[1] = -v;
            }
        })
        .unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 44 + frames as u64 * 4);
        assert!(!path.with_extension("wav.part").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_leaves_nothing() {
        let dir = std::env::temp_dir().join(format!("radiant-wav-cancel-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.wav");
        let cancel = AtomicBool::new(true);
        let r = write_stereo16(&path, 44_100, 1000, &cancel, |_, _| {});
        assert!(matches!(r, Err(StemError::Cancelled)));
        assert!(!path.exists() && !path.with_extension("wav.part").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
