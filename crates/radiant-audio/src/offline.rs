//! Offline decoding for jobs that need a whole track in memory (stem separation): file -> interleaved stereo f32
//! at the requested sample rate, using the same decoder and resampler as playback.

use crate::convert::Converter;
use crate::decode::TrackDecoder;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Decodes the whole file to interleaved stereo at `rate` Hz.
/// `progress` gets 0..1 (by the container's duration, which may be an estimate; never above 1).
/// `Ok(None)` = cancelled through `cancel`.
pub fn decode_stereo(
    path: &Path,
    rate: u32,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Option<Vec<f32>>, String> {
    let mut dec = TrackDecoder::open(path)?;
    let total_ms = dec.duration_ms.filter(|d| *d > 0);
    let mut out: Vec<f32> = Vec::new();
    if let Some(ms) = total_ms {
        out.reserve((ms as usize * rate as usize / 1000 + rate as usize) * 2);
    }
    let mut conv: Option<Converter> = None;
    let mut seen: Option<(u32, usize)> = None;
    let mut chunk: Vec<f32> = Vec::new();
    let mut src_frames: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Some((src_rate, channels)) = dec.next_chunk(&mut chunk)? else { break };
        match seen {
            None => {
                seen = Some((src_rate, channels));
                conv = Some(Converter::new(src_rate, channels, rate, 2));
            }
            Some(f) if f != (src_rate, channels) => {
                return Err("the sample rate or the channel count changes inside the file".into());
            }
            _ => {}
        }
        if let Some(c) = conv.as_mut() {
            c.process(&chunk, &mut out);
        }
        src_frames += (chunk.len() / channels.max(1)) as u64;
        if let Some(ms) = total_ms {
            let secs = src_frames as f64 / src_rate.max(1) as f64;
            progress(((secs * 1000.0) / ms as f64).min(1.0) as f32);
        }
    }
    let mut conv = conv.ok_or_else(|| "the file has no audio".to_string())?;
    conv.finish(&mut out);
    Ok(Some(out))
}
