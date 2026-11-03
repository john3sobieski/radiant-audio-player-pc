//! radiant-audio: playback engine.
//!
//! Layout:
//!   transport.rs  queue / index / repeat logic (no audio)
//!   decode.rs     symphonia wrapper: file -> interleaved f32 chunks
//!   convert.rs    channel remap + linear resampler to the device format
//!   engine.rs     engine thread (owns decoder + cpal stream), `Player` handle, commands
//!
//! Data flow: engine thread: decode -> convert -> radiant-dsp -> rtrb ring buffer -> cpal callback.
//! The cpal callback only pops samples (no alloc, no locks, no I/O) and applies the fade gain.
//!
//! DONE: seek + position (frames counted in the callback, UI gets Status every 250 ms while playing),
//!       play / pause / toggle / next / previous / repeat, 300 ms fade-in and 250 ms fade-out,
//!       auto-advance at end of track, stream paused when idle (no CPU while paused).
//! TODO:
//!   - shuffle, gapless (now: queue of next file starts when the ring buffer is empty)
//!   - replace the linear resampler with `rubato` (convert.rs)
//!   - device change (headphones unplugged) -> re-open the stream and resume
//!   - non-f32 device formats (now: error), skip unreadable files instead of stopping
//!   - hook real DSP settings (now: default = bypass), measure CPU and report it

mod convert;
mod decode;
mod engine;
mod transport;

pub use engine::{Command, Player, Status};
pub use transport::{PlayState, RepeatMode, Transport};
