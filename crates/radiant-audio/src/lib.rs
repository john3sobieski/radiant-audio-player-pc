//! radiant-audio: playback engine.
//!
//! Layout:
//!   transport.rs  queue / index / repeat logic (no audio)
//!   decode.rs     symphonia wrapper: file -> interleaved f32 chunks
//!   position.rs   source-time position that stays correct while the speed changes
//!   convert.rs    channel remap + linear resampler to the device format
//!   engine.rs     engine thread (owns decoder + cpal stream), `Player` handle, commands
//!
//! Data flow: engine thread: decode -> convert -> phase vocoder (speed) -> effects -> rtrb ring buffer -> cpal callback.
//! The cpal callback only pops samples (no alloc, no locks, no I/O) and applies the fade gain.
//!
//! DONE: speed with preserved pitch: `Command::SetSpeed(f32)` (vocoder after the resampler, reset on seek / new track,
//!       tail played out at the end of a track, position and progress in source time),
//!       sound settings: `Command::SetFx(AudioFxSettings)` (applied on the engine thread; audible within RING_MS),
//!       shuffle (random order, current track first), volume (smoothed, squared curve),
//!       seek + position (frames counted in the callback, UI gets Status every 250 ms while playing),
//!       play / pause / toggle / next / previous / repeat, 300 ms fade-in and 250 ms fade-out,
//!       auto-advance at end of track, stream paused when idle (no CPU while paused).
//! TODO:
//!   - gapless (now: queue of next file starts when the ring buffer is empty)
//!   - replace the linear resampler with `rubato` (convert.rs)
//!   - device change (headphones unplugged) -> re-open the stream and resume
//!   - non-f32 device formats (now: error), skip unreadable files instead of stopping
//!   - measure CPU and report it

mod convert;
mod decode;
mod engine;
mod position;
mod transport;

pub use engine::{Command, Player, Status};
pub use radiant_dsp::presets::{EqPreset, EQ_PRESETS};
pub use radiant_dsp::stretch::{slider_to_speed, MAX_SPEED, MIN_SPEED};
pub use radiant_dsp::AudioFxSettings;
pub use transport::{PlayState, RepeatMode, Transport};
