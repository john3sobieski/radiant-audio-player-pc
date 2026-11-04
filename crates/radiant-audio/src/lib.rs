//! radiant-audio: playback engine.
//!
//! Layout:
//!   transport.rs  queue / index / repeat logic (no audio)
//!   decode.rs     symphonia wrapper: file -> interleaved f32 chunks
//!   position.rs   source-time position that stays correct while the speed changes
//!   convert.rs    channel remap + streaming sample-rate conversion (buffering, delay trim, tail flush)
//!   sinc_core.rs  adapter over `rubato` (the only file that uses its API)
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
//!       rubato sinc resampler (convert.rs / sinc_core.rs), exact duration for files without a length header
//!       (background scan, decode.rs), sample-accurate seek (frames before the target are dropped),
//!       skip unreadable files (open / decode errors, at most `MAX_SKIPS` in a row), output device change
//!       (stream re-opened, playback resumes at the same position), vocoder fully bypassed at 1.0x.
//! TODO:
//!   - gapless (now: queue of next file starts when the ring buffer is empty)
//!   - start without any output device (now: the engine only waits for Quit)
//!   - non-f32 device formats (now: error)
//!   - measure CPU and report it

mod convert;
mod decode;
mod engine;
mod position;
mod sinc_core;
mod transport;

pub use engine::{Command, Player, Status};
pub use radiant_dsp::presets::{EqPreset, EQ_PRESETS};
pub use radiant_dsp::stretch::{slider_to_speed, MAX_SPEED, MIN_SPEED};
pub use radiant_dsp::AudioFxSettings;
pub use transport::{PlayState, RepeatMode, Transport};
