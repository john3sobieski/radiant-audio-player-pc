//! symphonia wrapper: opens a file and yields interleaved f32 chunks.
//!
//! Duration: normally the container reports the frame count. When it does not (raw AAC/ADTS, streaming FLAC,
//! a Xing tag without a frame count) the duration is `None` and seeking is off. For MP3 without a Xing/Info/VBRI
//! header symphonia only *estimates* the length from the bitrate of the first frame, which is wrong for VBR.
//! In both cases `needs_scan` is set and the engine runs `scan_duration` on a helper thread: it walks the
//! packets (no decoding, exact) and the duration appears a moment after the track starts.
//!
//! Seeking is sample-accurate: symphonia lands on the packet that contains the target, we then drop the
//! few frames in front of it, so the audio really starts at the requested position (not up to ~26 ms early for
//! MP3 or longer for FLAC/OGG).

use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_MP3, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};

pub struct TrackDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_buf: Option<SampleBuffer<f32>>,
    time_base: Option<TimeBase>,
    /// Seconds of audio still to drop at the start of the next chunks (after a seek).
    skip_secs: f64,
    /// From the container; may be an estimate (see `needs_scan`). `None` = unknown (seeking disabled).
    pub duration_ms: Option<u64>,
    /// The duration is missing or only estimated: run `scan_duration` in the background.
    pub needs_scan: bool,
}

fn open_format(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| e.to_string())?;
    Ok(probed.format)
}

impl TrackDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let format = open_format(path)?;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| "no audio track".to_string())?;
        let track_id = track.id;
        let time_base = track.codec_params.time_base;
        let duration_ms = match (track.codec_params.n_frames, track.codec_params.sample_rate) {
            (Some(n), Some(r)) if r > 0 => Some(n * 1000 / r as u64),
            _ => None,
        };
        let needs_scan =
            duration_ms.is_none() || (track.codec_params.codec == CODEC_TYPE_MP3 && mp3_needs_scan(path));
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| e.to_string())?;
        Ok(Self { format, decoder, track_id, sample_buf: None, time_base, skip_secs: 0.0, duration_ms, needs_scan })
    }

    /// Jumps to `secs` from the start of the track; the next chunk starts exactly there.
    pub fn seek(&mut self, secs: f64) -> Result<(), String> {
        let secs = secs.max(0.0);
        let time = Time::new(secs.trunc() as u64, secs.fract());
        let seeked = self
            .format
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| e.to_string())?;
        self.decoder.reset();
        // The format lands on a packet boundary at or before the target: drop the difference.
        self.skip_secs = match self.time_base {
            Some(tb) if seeked.required_ts > seeked.actual_ts => {
                let t = tb.calc_time(seeked.required_ts - seeked.actual_ts);
                t.seconds as f64 + t.frac
            }
            _ => 0.0,
        };
        Ok(())
    }

    /// Decodes the next packet into `out` (cleared first) as interleaved f32.
    /// Returns `Some((sample_rate, channels))`, or `None` at the end of the stream.
    pub fn next_chunk(&mut self, out: &mut Vec<f32>) -> Result<Option<(u32, usize)>, String> {
        out.clear();
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
                Err(Error::ResetRequired) => return Ok(None),
                Err(e) => return Err(e.to_string()),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let channels = spec.channels.count();
                    let need = decoded.capacity() * channels;
                    if self.sample_buf.as_ref().map_or(true, |b| b.capacity() < need) {
                        self.sample_buf =
                            Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                    }
                    if let Some(buf) = self.sample_buf.as_mut() {
                        buf.copy_interleaved_ref(decoded);
                        out.extend_from_slice(buf.samples());
                    }
                    if self.skip_secs > 0.0 && channels > 0 {
                        let want = (self.skip_secs * spec.rate as f64).round() as usize;
                        let drop = want.min(out.len() / channels);
                        out.drain(..drop * channels);
                        self.skip_secs = (want - drop) as f64 / spec.rate as f64;
                        if out.is_empty() {
                            continue; // the whole packet was before the target
                        }
                    }
                    return Ok(Some((spec.rate, channels)));
                }
                // A corrupt packet: skip it and keep going.
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

/// Exact duration in milliseconds by walking all packets of the first audio track (no decoding).
/// Reads the whole file once, so run it off the engine thread.
pub fn scan_duration(path: &Path) -> Result<u64, String> {
    let mut format = open_format(path)?;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "no audio track".to_string())?;
    let track_id = track.id;
    let time_base = track.codec_params.time_base;
    let rate = track.codec_params.sample_rate;

    let mut end: u64 = 0;
    let mut errors_in_a_row = 0;
    loop {
        match format.next_packet() {
            Ok(p) => {
                errors_in_a_row = 0;
                if p.track_id() == track_id {
                    end = end.max(p.ts().saturating_add(p.dur()));
                }
            }
            Err(Error::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(Error::DecodeError(_)) if errors_in_a_row < 100 => errors_in_a_row += 1,
            Err(e) => {
                if end == 0 {
                    return Err(e.to_string());
                }
                break; // keep what was counted so far
            }
        }
    }
    if end == 0 {
        return Err("no packets".into());
    }
    match (time_base, rate) {
        (Some(tb), _) => {
            let t = tb.calc_time(end);
            Ok(t.seconds * 1000 + (t.frac * 1000.0) as u64)
        }
        (None, Some(r)) if r > 0 => Ok(end * 1000 / r as u64),
        _ => Err("unknown time base".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// MP3 length header detection (pure functions, unit-tested)
// ---------------------------------------------------------------------------------------------

/// Size of an ID3v2 tag including its 10-byte header (and the footer if flagged), from the first 10 bytes.
fn id3v2_len(head: &[u8; 10]) -> Option<u64> {
    if &head[..3] != b"ID3" {
        return None;
    }
    // Four "syncsafe" bytes (7 bits each).
    if head[6..10].iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    let size = head[6..10].iter().fold(0u64, |acc, b| (acc << 7) | *b as u64);
    let footer = if head[5] & 0x10 != 0 { 10 } else { 0 };
    Some(10 + size + footer)
}

/// The first MPEG frame carries a Xing/Info/VBRI header (they sit within the first ~40 bytes of the frame).
fn has_length_tag(region: &[u8]) -> bool {
    region.windows(4).any(|w| w == b"Xing" || w == b"Info" || w == b"VBRI")
}

/// True for a raw MPEG audio file (optionally behind an ID3v2 tag) whose first frame has no length header:
/// symphonia can only guess its duration then. MP3 inside other containers return false (the container knows).
fn mp3_needs_scan(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else { return false };
    let mut head = [0u8; 10];
    if f.read_exact(&mut head).is_err() {
        return false;
    }
    let start = id3v2_len(&head).unwrap_or(0);
    if start == 0 && head[0] != 0xFF {
        return false; // not a raw MPEG stream
    }
    if f.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    let mut buf = [0u8; 512];
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
    }
    n > 0 && !has_length_tag(&buf[..n])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Unique temp file path (removed by the test at the end).
    fn temp_path(name: &str) -> std::path::PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!("radiant-test-{}-{}-{name}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)))
    }

    /// 16-bit mono WAV where sample i = i % 30011 (a prime): the value tells the exact frame position.
    fn write_ramp_wav(path: &Path, rate: u32, frames: usize) {
        let data_len = (frames * 2) as u32;
        let mut f = File::create(path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * 2).to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        let mut bytes = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            bytes.extend_from_slice(&((i % 30011) as i16).to_le_bytes());
        }
        f.write_all(&bytes).unwrap();
    }

    /// Frame index encoded in a decoded sample (inverse of the 16-bit ramp, f32 = value / 32768).
    fn frame_of(sample: f32) -> usize {
        (sample * 32768.0).round() as usize
    }

    fn first_samples(dec: &mut TrackDecoder, count: usize) -> Vec<f32> {
        let mut all = Vec::new();
        let mut chunk = Vec::new();
        while all.len() < count {
            match dec.next_chunk(&mut chunk).unwrap() {
                Some(_) => all.extend_from_slice(&chunk),
                None => break,
            }
        }
        all.truncate(count);
        all
    }

    #[test]
    fn wav_duration_is_read() {
        let p = temp_path("dur.wav");
        write_ramp_wav(&p, 8000, 40_000);
        let dec = TrackDecoder::open(&p).unwrap();
        assert_eq!(dec.duration_ms, Some(5000));
        assert!(!dec.needs_scan);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn seek_is_sample_accurate_forward_and_backward() {
        let p = temp_path("seek.wav");
        write_ramp_wav(&p, 8000, 40_000);
        let mut dec = TrackDecoder::open(&p).unwrap();
        // 3.125 s = 25000 frames, 0.5 s = 4000, 1.0 s = 8000 (all exact in binary).
        for &(secs, frame) in &[(3.125, 25_000usize), (0.5, 4_000), (4.375, 35_000), (1.0, 8_000), (0.0, 0)] {
            dec.seek(secs).unwrap();
            let s = first_samples(&mut dec, 300);
            assert_eq!(s.len(), 300, "seek {secs}");
            // The ramp wraps at 30011, so compare modulo that.
            assert_eq!(frame_of(s[0]), frame % 30011, "first frame after seek to {secs}s");
            for (i, v) in s.iter().enumerate() {
                assert_eq!(frame_of(*v), (frame + i) % 30011, "continuity after seek to {secs}s, sample {i}");
            }
        }
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn seek_near_the_end_leaves_exactly_the_rest() {
        let p = temp_path("end.wav");
        write_ramp_wav(&p, 8000, 40_000);
        let mut dec = TrackDecoder::open(&p).unwrap();
        dec.seek(4.5).unwrap(); // frame 36000, 4000 frames left
        let mut total = 0;
        let mut chunk = Vec::new();
        while let Some(_) = dec.next_chunk(&mut chunk).unwrap() {
            total += chunk.len();
        }
        assert_eq!(total, 4000);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn scan_gives_the_exact_duration() {
        let p = temp_path("scan.wav");
        write_ramp_wav(&p, 8000, 40_000);
        assert_eq!(scan_duration(&p).unwrap(), 5000);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn scan_of_a_missing_file_is_an_error() {
        assert!(scan_duration(Path::new("definitely/not/here.mp3")).is_err());
    }

    // ---- MP3 header detection ----

    #[test]
    fn id3_size_is_syncsafe() {
        let mut h = [0u8; 10];
        h[..3].copy_from_slice(b"ID3");
        h[3] = 3;
        h[6..10].copy_from_slice(&[0, 0, 0x02, 0x01]); // 2*128 + 1 = 257
        assert_eq!(id3v2_len(&h), Some(10 + 257));
        h[5] = 0x10; // footer present
        assert_eq!(id3v2_len(&h), Some(10 + 257 + 10));
        h[6] = 0x80; // not syncsafe
        assert_eq!(id3v2_len(&h), None);
        assert_eq!(id3v2_len(&[0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn length_tags_are_found() {
        let mut frame = vec![0u8; 200];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        assert!(!has_length_tag(&frame));
        frame[36..40].copy_from_slice(b"Xing");
        assert!(has_length_tag(&frame));
        frame[36..40].copy_from_slice(b"Info");
        assert!(has_length_tag(&frame));
        frame[36..40].copy_from_slice(b"VBRI");
        assert!(has_length_tag(&frame));
    }

    /// Synthetic MP3 starts: [ID3 tag] + frame header + zeros [+ Xing].
    fn fake_mp3(path: &Path, id3_payload: Option<usize>, tag: Option<&[u8; 4]>) {
        let mut b = Vec::new();
        if let Some(n) = id3_payload {
            b.extend_from_slice(b"ID3\x03\x00\x00");
            b.extend_from_slice(&[0, 0, (n >> 7) as u8 & 0x7F, n as u8 & 0x7F]);
            b.extend(std::iter::repeat(0u8).take(n));
        }
        let mut frame = vec![0u8; 400];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        if let Some(t) = tag {
            frame[36..40].copy_from_slice(t);
        }
        b.extend_from_slice(&frame);
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn mp3_without_a_length_header_needs_a_scan() {
        let cases: [(Option<usize>, Option<&[u8; 4]>, bool); 6] = [
            (None, None, true),
            (None, Some(b"Xing"), false),
            (Some(300), None, true),
            (Some(300), Some(b"Info"), false),
            (Some(5000), Some(b"Xing"), false), // the tag is found behind a big ID3 tag too
            (Some(5000), None, true),
        ];
        for (i, (id3, tag, expect)) in cases.iter().enumerate() {
            let p = temp_path(&format!("case{i}.mp3"));
            fake_mp3(&p, *id3, *tag);
            assert_eq!(mp3_needs_scan(&p), *expect, "case {i}");
            std::fs::remove_file(&p).ok();
        }
    }

    #[test]
    fn non_mpeg_files_never_ask_for_an_mp3_scan() {
        let p = temp_path("not.mp3");
        write_ramp_wav(&p, 8000, 100);
        assert!(!mp3_needs_scan(&p));
        std::fs::remove_file(&p).ok();
        assert!(!mp3_needs_scan(Path::new("no/such/file.mp3")));
    }
}
