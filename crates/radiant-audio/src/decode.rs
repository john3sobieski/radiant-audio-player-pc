//! symphonia wrapper: opens a file and yields interleaved f32 chunks.

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

pub struct TrackDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_buf: Option<SampleBuffer<f32>>,
    /// Known only when the container reports the frame count.
    /// TODO: fallback estimate (e.g. from file size and bitrate) when it is missing.
    pub duration_ms: Option<u64>,
}

impl TrackDecoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| e.to_string())?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| "no audio track".to_string())?;
        let track_id = track.id;
        let duration_ms = match (track.codec_params.n_frames, track.codec_params.sample_rate) {
            (Some(n), Some(r)) if r > 0 => Some(n * 1000 / r as u64),
            _ => None,
        };
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| e.to_string())?;
        Ok(Self { format, decoder, track_id, sample_buf: None, duration_ms })
    }

    /// Jumps to `secs` from the start of the track.
    pub fn seek(&mut self, secs: f64) -> Result<(), String> {
        let time = Time::new(secs.trunc() as u64, secs.fract());
        self.format
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| e.to_string())?;
        self.decoder.reset();
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
                    return Ok(Some((spec.rate, channels)));
                }
                // A corrupt packet: skip it and keep going.
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}
