//! Engine thread. It owns the decoder and the cpal stream (`cpal::Stream` is not `Send` on
//! Windows, so it must be created and used on one thread). The UI talks to it through `Player`.
//!
//! Real-time rule: the cpal callback does not allocate, lock or do I/O. It pops samples from the
//! ring buffer and applies a fade gain. Everything else happens on this thread.

use crate::convert::Converter;
use crate::decode::{scan_duration, TrackDecoder};
use crate::position::PositionTracker;
use crate::transport::{PlayState, RepeatMode, Transport};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use radiant_dsp::stretch::{TimeStretch, MAX_SPEED, MIN_SPEED};
use radiant_dsp::{AudioFxProcessor, AudioFxSettings};
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FADE_IN_SECS: f32 = 0.3;
const FADE_OUT_SECS: f32 = 0.25;
/// Ring buffer length in milliseconds of audio. The effects run before the ring buffer, so this is
/// also how long a sound-settings change takes to become audible (was 1000 ms, shortened for that).
const RING_MS: usize = 400;
/// How often the engine tops up the ring buffer while playing.
const FILL_INTERVAL: Duration = Duration::from_millis(50);
/// After pausing, keep the stream alive this long (fade-out), then pause it (0 % CPU).
const PAUSE_STREAM_AFTER: Duration = Duration::from_millis(450);
/// How often the UI gets a position update while playing.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);
/// Unreadable files in a row after which the player stops (avoids an endless loop over a broken queue).
const MAX_SKIPS: u32 = 100;
/// How often the default output device is compared with the open one (only while playing).
const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// Audio held inside the phase vocoder; used to correct the position after a speed change (estimate).
const VOCODER_LATENCY_MS: usize = 90;

/// A decoder panic on a damaged file becomes a normal error (needs `panic = "unwind"` in the profile).
fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err("decoder crashed on a damaged file".into()))
}

fn default_device_name() -> Option<String> {
    cpal::default_host().default_output_device()?.name().ok()
}

#[derive(Debug)]
pub enum Command {
    /// Replace the queue and start playing `start`.
    SetQueue { queue: Vec<PathBuf>, start: usize },
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    /// Seek to a position given as 0.0..=1.0 of the track length.
    SeekRatio(f32),
    SetRepeat(RepeatMode),
    SetShuffle(bool),
    /// 0.0..=1.0 on the volume slider (a squared curve is applied: sounds more even).
    SetVolume(f32),
    /// Replace the sound settings (EQ, bass, treble, subsonic, normalization, mono).
    SetFx(AudioFxSettings),
    /// Playback speed, pitch preserved (phase vocoder). 1.0 = normal, clamped to 0.25..=4.
    /// The UI converts its slider with `slider_to_speed`.
    SetSpeed(f32),
    /// Internal: a background scan found the exact length of `path` (sent by the engine to itself).
    #[doc(hidden)]
    DurationScanned { path: PathBuf, ms: u64 },
    Quit,
}

/// Sent to the UI callback after every change.
#[derive(Debug, Clone)]
pub struct Status {
    pub state: PlayState,
    pub current: Option<PathBuf>,
    pub position_ms: u64,
    /// `None` when the file does not report its length (seeking is disabled then).
    pub duration_ms: Option<u64>,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub error: Option<String>,
}

/// Handle used by the UI. Dropping it stops the engine thread.
pub struct Player {
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// `on_change` is called on the engine thread (forward to the UI thread yourself).
    pub fn spawn<F: Fn(Status) + Send + 'static>(on_change: F) -> Player {
        let (tx, rx) = mpsc::channel();
        let engine_tx = tx.clone(); // the engine sends scan results to itself
        let thread = thread::Builder::new()
            .name("radiant-engine".into())
            .spawn(move || run(rx, engine_tx, Box::new(on_change)))
            .expect("cannot start the engine thread");
        Player { tx, thread: Some(thread) }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Output {
    stream: cpal::Stream,
    prod: rtrb::Producer<f32>,
    capacity: usize,
    rate: u32,
    channels: usize,
    /// true = fade in / play, false = fade out / silence.
    playing: Arc<AtomicBool>,
    /// The engine bumps `flush_req`; the callback drains the ring buffer and copies it to `flush_ack`.
    flush_req: Arc<AtomicU32>,
    flush_ack: Arc<AtomicU32>,
    /// Frames given to the device since the last flush (position tracking).
    played: Arc<AtomicU64>,
    /// Linear gain as `f32` bits (set by `SetVolume`).
    volume: Arc<AtomicU32>,
    device_name: String,
    /// Set by the cpal error callback (device unplugged, stream died).
    lost: Arc<AtomicBool>,
}

fn open_output() -> Result<Output, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no audio output device".to_string())?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    // TODO: support other sample formats (i16 / u16) by converting in the callback.
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("unsupported sample format {:?}", supported.sample_format()));
    }
    let device_name = device.name().unwrap_or_default();
    let lost = Arc::new(AtomicBool::new(false));
    let err_lost = lost.clone();
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels as usize;
    let rate = config.sample_rate.0;

    // Whole frames only (a multiple of `channels`).
    let capacity = (rate as usize * RING_MS / 1000) * channels;
    let (prod, mut cons) = rtrb::RingBuffer::<f32>::new(capacity);
    let playing = Arc::new(AtomicBool::new(false));
    let flush_req = Arc::new(AtomicU32::new(0));
    let flush_ack = Arc::new(AtomicU32::new(0));
    let played = Arc::new(AtomicU64::new(0));
    let volume = Arc::new(AtomicU32::new(1.0_f32.to_bits()));

    let cb_playing = playing.clone();
    let cb_req = flush_req.clone();
    let cb_ack = flush_ack.clone();
    let cb_played = played.clone();
    let cb_volume = volume.clone();
    let up = 1.0 / (FADE_IN_SECS * rate as f32);
    let down = 1.0 / (FADE_OUT_SECS * rate as f32);
    let mut gain = 0.0_f32;
    // Volume changes are smoothed (about 20 ms) so dragging the slider does not crackle.
    let mut vol = 1.0_f32;
    let vol_coef = 1.0 - (-1.0 / (0.02 * rate as f32)).exp();
    let mut seen_req = 0_u32;

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                // --- real-time code: no allocation, no locks ---
                let req = cb_req.load(Ordering::Acquire);
                if req != seen_req {
                    while cons.pop().is_ok() {}
                    gain = 0.0;
                    cb_played.store(0, Ordering::Relaxed);
                    seen_req = req;
                    cb_ack.store(req, Ordering::Release);
                }
                let target = if cb_playing.load(Ordering::Relaxed) { 1.0_f32 } else { 0.0 };
                let target_vol = f32::from_bits(cb_volume.load(Ordering::Relaxed));
                let mut frames_out = 0_u64;
                for frame in data.chunks_mut(channels) {
                    if target == 0.0 && gain <= 0.0 {
                        frame.fill(0.0);
                        continue;
                    }
                    if gain < target {
                        gain = (gain + up).min(target);
                    } else if gain > target {
                        gain = (gain - down).max(target);
                    }
                    vol += (target_vol - vol) * vol_coef;
                    let total = gain * vol;
                    for s in frame.iter_mut() {
                        *s = cons.pop().unwrap_or(0.0) * total;
                    }
                    frames_out += 1;
                }
                if frames_out > 0 {
                    cb_played.fetch_add(frames_out, Ordering::Relaxed);
                }
            },
            move |err| {
                eprintln!("audio stream error: {err}");
                err_lost.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| e.to_string())?;
    // Some backends start the stream right away; start paused so idle costs nothing.
    let _ = stream.pause();

    Ok(Output {
        stream,
        prod,
        capacity,
        rate,
        channels,
        playing,
        flush_req,
        flush_ack,
        played,
        volume,
        device_name,
        lost,
    })
}

struct Engine {
    out: Output,
    transport: Transport,
    decoder: Option<TrackDecoder>,
    conv: Option<Converter>,
    /// Decoded chunk in the source format.
    raw: Vec<f32>,
    /// Converted samples (device format) not yet pushed to the ring buffer.
    pending: Vec<f32>,
    pending_pos: usize,
    /// The decoder has no more data.
    eof: bool,
    /// Phase vocoder (speed change), runs after the resampler and before the effects.
    stretch: TimeStretch,
    /// Resampler output waiting for the vocoder (unused while the vocoder is in pass-through).
    converted: Vec<f32>,
    fx: AudioFxProcessor,
    on_change: Box<dyn Fn(Status) + Send>,
    /// Own command channel: lets a helper thread report the scanned duration.
    tx: Sender<Command>,
    /// When set, the stream is paused after this moment (fade-out finished).
    pause_deadline: Option<Instant>,
    error: Option<String>,
    duration_ms: Option<u64>,
    /// Source-time position (stays correct while the speed changes), see `position.rs`.
    pos: PositionTracker,
    last_notify: Cell<Instant>,
    /// Last sound settings, to rebuild the effects after the output device changed.
    fx_settings: AudioFxSettings,
    /// The vocoder is in the signal path (speed != 1). Off = no vocoder work at all.
    use_voc: bool,
    /// Speed went back to 1.0: play out the vocoder tail on the next fill, then bypass it.
    voc_drain: bool,
    /// Files in a row that gave no audio.
    skip_streak: u32,
    last_device_check: Instant,
}

fn run(rx: Receiver<Command>, tx: Sender<Command>, on_change: Box<dyn Fn(Status) + Send>) {
    let out = match open_output() {
        Ok(o) => o,
        Err(e) => {
            on_change(Status {
                state: PlayState::Stopped,
                current: None,
                position_ms: 0,
                duration_ms: None,
                repeat: RepeatMode::Off,
                shuffle: false,
                error: Some(e),
            });
            // No audio: just wait until the player is dropped.
            while let Ok(cmd) = rx.recv() {
                if matches!(cmd, Command::Quit) {
                    break;
                }
            }
            return;
        }
    };
    let (out_rate, out_ch) = (out.rate, out.channels);
    let mut eng = Engine {
        out,
        transport: Transport::default(),
        decoder: None,
        conv: None,
        raw: Vec::new(),
        pending: Vec::new(),
        pending_pos: 0,
        eof: false,
        stretch: TimeStretch::new(out_ch, out_rate),
        converted: Vec::new(),
        fx: AudioFxProcessor::new(AudioFxSettings::default()),
        on_change,
        tx,
        pause_deadline: None,
        error: None,
        duration_ms: None,
        pos: PositionTracker::new(out_rate),
        last_notify: Cell::new(Instant::now()),
        fx_settings: AudioFxSettings::default(),
        use_voc: false,
        voc_drain: false,
        skip_streak: 0,
        last_device_check: Instant::now(),
    };
    loop {
        // Block without polling when there is nothing to do (0 % CPU while paused).
        let msg = match eng.timeout() {
            Some(d) => rx.recv_timeout(d),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(cmd) => {
                if !eng.handle(cmd) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        eng.tick();
    }
}

impl Engine {
    fn timeout(&self) -> Option<Duration> {
        match self.transport.state {
            PlayState::Playing => Some(FILL_INTERVAL),
            _ => self
                .pause_deadline
                .map(|d| d.saturating_duration_since(Instant::now())),
        }
    }

    fn position_ms(&self) -> u64 {
        let ms = self.pos.position_ms(self.out.played.load(Ordering::Relaxed));
        self.duration_ms.map_or(ms, |d| ms.min(d))
    }

    fn current_label(&self) -> String {
        self.transport
            .current()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Every `stretch.reset()` goes through here so the flags stay consistent.
    fn vocoder_reset(&mut self) {
        self.stretch.reset();
        self.voc_drain = false;
        self.use_voc = (self.stretch.speed() - 1.0).abs() > 1e-4;
    }

    /// Re-opens the output when the device is gone or the system default changed.
    /// false = no usable device (the error text is set).
    fn refresh_output(&mut self) -> bool {
        let gone = self.out.lost.load(Ordering::Relaxed);
        let changed = default_device_name().as_deref() != Some(self.out.device_name.as_str());
        if !gone && !changed {
            return true;
        }
        match self.reopen_output() {
            Ok(()) => true,
            Err(e) => {
                self.error = Some(format!("Audio device: {e}"));
                false
            }
        }
    }

    /// New stream on the current default device; playback continues from the same position.
    fn reopen_output(&mut self) -> Result<(), String> {
        let new = open_output()?; // on error the old output stays as it was
        let ms = self.position_ms();
        let speed = self.stretch.speed();
        new.volume.store(self.out.volume.load(Ordering::Relaxed), Ordering::Relaxed);
        let (rate, ch) = (new.rate, new.channels);
        self.out = new; // the old stream is dropped (closed) here
        // Everything that depends on the rate / channel count is rebuilt.
        self.stretch = TimeStretch::new(ch, rate);
        self.stretch.set_speed(speed);
        self.vocoder_reset();
        self.fx = AudioFxProcessor::new(self.fx_settings.clone());
        self.conv = None; // recreated on the next chunk for the new rate
        self.pending.clear();
        self.pending_pos = 0;
        self.converted.clear();
        self.pos = PositionTracker::new(rate);
        if let Some(dec) = self.decoder.as_mut() {
            if dec.seek(ms as f64 / 1000.0).is_ok() {
                self.eof = false;
            }
        }
        self.pos.reset(self.out.played.load(Ordering::Relaxed), ms, speed);
        self.fill();
        if self.transport.state == PlayState::Playing {
            let _ = self.out.stream.play();
            self.out.playing.store(true, Ordering::Release);
            self.pause_deadline = None;
        }
        Ok(())
    }

    /// Speed change: the new speed is applied to audio decoded from now on, so it is heard after the
    /// audio already queued (ring buffer + pending + what is still inside the vocoder, ~VOCODER_LATENCY_MS).
    /// The position tracker is told where that is.
    fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        if (speed - self.stretch.speed()).abs() < 1e-4 {
            return;
        }
        let ch = self.out.channels.max(1);
        let played = self.out.played.load(Ordering::Relaxed);
        let queued_ring = (self.out.capacity - self.out.prod.slots()) / ch;
        let queued_pending = self.pending.len().saturating_sub(self.pending_pos) / ch;
        let in_vocoder = if self.use_voc {
            self.out.rate as usize * VOCODER_LATENCY_MS / 1000
        } else {
            0
        };
        self.pos.settle(played);
        self.pos
            .push_speed(played + (queued_ring + queued_pending + in_vocoder) as u64, speed);
        self.stretch.set_speed(speed);
        if (speed - 1.0).abs() < 1e-4 {
            self.voc_drain = self.use_voc; // flushed and bypassed in the next fill()
        } else {
            self.use_voc = true;
            self.voc_drain = false;
        }
    }

    fn notify(&self) {
        self.last_notify.set(Instant::now());
        (self.on_change)(Status {
            state: self.transport.state,
            current: self.transport.current().map(|p| p.to_path_buf()),
            position_ms: self.position_ms(),
            duration_ms: self.duration_ms,
            repeat: self.transport.repeat,
            shuffle: self.transport.shuffle(),
            error: self.error.clone(),
        });
    }

    /// Returns false when the engine must quit.
    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::SetQueue { queue, start } => {
                self.transport.set_queue(queue, start);
                self.play_current(true);
            }
            Command::Play => match self.transport.state {
                PlayState::Playing => {}
                PlayState::Paused => self.start(),
                PlayState::Stopped => self.play_current(true),
            },
            Command::Pause => {
                if self.transport.state == PlayState::Playing {
                    self.pause();
                }
            }
            Command::Toggle => match self.transport.state {
                PlayState::Playing => self.pause(),
                PlayState::Paused => self.start(),
                PlayState::Stopped => self.play_current(true),
            },
            Command::Stop => self.stop(),
            Command::Next => {
                if self.transport.next(false).is_some() {
                    self.play_current(true);
                } else {
                    self.stop();
                }
            }
            Command::Previous => {
                if self.transport.previous().is_some() {
                    self.play_current(true);
                }
            }
            Command::SeekRatio(r) => self.seek_ratio(r),
            Command::SetRepeat(r) => self.transport.repeat = r,
            Command::SetShuffle(on) => self.transport.set_shuffle(on),
            Command::SetVolume(v) => {
                let v = v.clamp(0.0, 1.0);
                self.out.volume.store((v * v).to_bits(), Ordering::Relaxed);
            }
            Command::SetFx(settings) => {
                self.fx.set_settings(settings.clone());
                self.fx_settings = settings;
            }
            Command::SetSpeed(speed) => self.set_speed(speed),
            Command::DurationScanned { path, ms } => {
                // Ignore a late result for a track that is no longer the current one.
                if self.transport.current() == Some(path.as_path()) {
                    self.duration_ms = Some(ms);
                }
            }
            Command::Quit => return false,
        }
        self.notify();
        true
    }

    /// Called after every wake-up: top up the buffer, detect the end of the track, pause the stream.
    fn tick(&mut self) {
        match self.transport.state {
            PlayState::Playing => {
                if self.out.lost.load(Ordering::Relaxed)
                    || self.last_device_check.elapsed() >= DEVICE_CHECK_INTERVAL
                {
                    self.last_device_check = Instant::now();
                    if !self.refresh_output() {
                        self.pause();
                        self.notify();
                        return;
                    }
                }
                self.pos.settle(self.out.played.load(Ordering::Relaxed));
                self.fill();
                if self.last_notify.get().elapsed() >= POSITION_INTERVAL {
                    self.notify();
                }
                let drained = self.out.prod.slots() == self.out.capacity;
                if self.eof && self.pending_pos >= self.pending.len() && drained {
                    // Track ended by itself (or was cut by a decode error: then it is simply skipped).
                    // The ring buffer is empty, so no flush is needed.
                    let go_on = self.skip_streak < MAX_SKIPS && self.transport.next(true).is_some();
                    if go_on {
                        self.play_current(false);
                    } else {
                        self.stop();
                    }
                    self.notify();
                }
            }
            _ => {
                if let Some(d) = self.pause_deadline {
                    if Instant::now() >= d {
                        let _ = self.out.stream.pause();
                        self.pause_deadline = None;
                    }
                }
            }
        }
    }

    /// Wait (briefly) until the callback has drained the ring buffer.
    fn flush(&mut self) {
        let _ = self.out.stream.play(); // the callback must run to see the request
        let req = self.out.flush_req.fetch_add(1, Ordering::AcqRel) + 1;
        let t0 = Instant::now();
        while self.out.flush_ack.load(Ordering::Acquire) != req
            && t0.elapsed() < Duration::from_millis(250)
        {
            thread::sleep(Duration::from_millis(1));
        }
        if self.transport.state != PlayState::Playing {
            // The flush woke the stream up; let it sleep again.
            self.pause_deadline = Some(Instant::now() + PAUSE_STREAM_AFTER);
        }
    }

    fn seek_ratio(&mut self, ratio: f32) {
        let Some(duration) = self.duration_ms else { return };
        let ms = (duration as f64 * ratio.clamp(0.0, 1.0) as f64) as u64;
        let Some(dec) = self.decoder.as_mut() else { return };
        if let Err(e) = dec.seek(ms as f64 / 1000.0) {
            self.error = Some(e);
            return;
        }
        self.pending.clear();
        self.pending_pos = 0;
        self.eof = false;
        if let Some(c) = self.conv.as_mut() {
            c.reset(); // restart the resampler (keeps its filter tables)
        }
        self.vocoder_reset(); // restart the vocoder (no old audio mixed into the new position)
        self.flush();
        self.pos.reset(self.out.played.load(Ordering::Relaxed), ms, self.stretch.speed());
        self.fill();
    }

    /// Opens the current track, fills the buffer and starts output.
    /// An unreadable file is skipped (the next one is tried). `flush` = started by the user.
    fn play_current(&mut self, flush: bool) {
        if flush {
            self.error = None;
            self.skip_streak = 0;
        }
        loop {
            self.decoder = None;
            self.conv = None;
            self.pending.clear();
            self.pending_pos = 0;
            self.eof = false;
            self.duration_ms = None;
            self.vocoder_reset();
            let Some(path) = self.transport.current().map(|p| p.to_path_buf()) else {
                self.stop();
                return;
            };
            match guarded(|| TrackDecoder::open(&path)) {
                Ok(d) => {
                    self.duration_ms = d.duration_ms;
                    if d.needs_scan {
                        // Length missing or only estimated: find the exact one in the background.
                        let tx = self.tx.clone();
                        let scan_path = path.clone();
                        let _ = thread::Builder::new().name("radiant-scan".into()).spawn(move || {
                            if let Ok(ms) = scan_duration(&scan_path) {
                                let _ = tx.send(Command::DurationScanned { path: scan_path, ms });
                            }
                        });
                    }
                    self.decoder = Some(d);
                    if flush {
                        self.flush();
                    }
                    self.pos.reset(self.out.played.load(Ordering::Relaxed), 0, self.stretch.speed());
                    self.fill();
                    self.start();
                    return;
                }
                Err(e) => {
                    self.error = Some(format!("Skipped {}: {e}", self.current_label()));
                    self.skip_streak += 1;
                    if self.skip_streak >= MAX_SKIPS || self.transport.next(false).is_none() {
                        self.stop();
                        return;
                    }
                }
            }
        }
    }

    fn start(&mut self) {
        if !self.refresh_output() {
            self.pause(); // no usable device; Play tries again
            return;
        }
        let _ = self.out.stream.play();
        self.out.playing.store(true, Ordering::Release);
        self.transport.state = PlayState::Playing;
        self.pause_deadline = None;
    }

    fn pause(&mut self) {
        self.out.playing.store(false, Ordering::Release);
        self.transport.state = PlayState::Paused;
        self.pause_deadline = Some(Instant::now() + PAUSE_STREAM_AFTER);
    }

    fn stop(&mut self) {
        self.out.playing.store(false, Ordering::Release);
        self.transport.state = PlayState::Stopped;
        self.decoder = None;
        self.pending.clear();
        self.pending_pos = 0;
        self.pause_deadline = Some(Instant::now() + PAUSE_STREAM_AFTER);
    }

    /// Decode and push audio until the ring buffer is full or the track ends.
    fn fill(&mut self) {
        let ch = self.out.channels;
        loop {
            // 1. Push what is already converted. Whole frames only, so the callback
            //    never sees half a frame (that would swap the channels).
            while self.pending_pos < self.pending.len() {
                let free = self.out.prod.slots() / ch * ch;
                let left = self.pending.len() - self.pending_pos;
                let n = free.min(left);
                if n == 0 {
                    return;
                }
                let Ok(chunk) = self.out.prod.write_chunk_uninit(n) else { return };
                chunk.fill_from_iter(
                    self.pending[self.pending_pos..self.pending_pos + n].iter().copied(),
                );
                self.pending_pos += n;
                if n < left {
                    return; // ring buffer is full
                }
            }
            self.pending.clear();
            self.pending_pos = 0;
            if self.eof {
                return;
            }

            // 2. Decode, convert to the device format, run the effects.
            if self.voc_drain {
                // Speed is back at 1.0: play out the vocoder tail once, then bypass it.
                self.stretch.finish(&mut self.pending);
                self.vocoder_reset();
                if !self.pending.is_empty() {
                    self.fx.process(&mut self.pending, ch, self.out.rate);
                    continue; // step 1 pushes the tail
                }
            }
            let Some(dec) = self.decoder.as_mut() else { return };
            let res = guarded(|| dec.next_chunk(&mut self.raw));
            match res {
                Ok(Some((rate, src_ch))) => {
                    self.skip_streak = 0;
                    if self.conv.is_none() {
                        self.conv = Some(Converter::new(rate, src_ch, self.out.rate, ch));
                    }
                    if let Some(c) = self.conv.as_mut() {
                        if !self.use_voc {
                            // Speed 1: no vocoder, no extra copy.
                            c.process(&self.raw, &mut self.pending);
                        } else {
                            self.converted.clear();
                            c.process(&self.raw, &mut self.converted);
                            self.stretch.process(&self.converted, &mut self.pending);
                        }
                    }
                    self.fx.process(&mut self.pending, ch, self.out.rate);
                }
                Ok(None) => {
                    self.eof = true;
                    // Tails, in signal order: the resampler's, then the vocoder's (~90 ms), then the effects.
                    self.converted.clear();
                    if let Some(c) = self.conv.as_mut() {
                        c.finish(&mut self.converted);
                    }
                    if !self.converted.is_empty() {
                        if !self.use_voc {
                            self.pending.extend_from_slice(&self.converted);
                        } else {
                            self.stretch.process(&self.converted, &mut self.pending);
                        }
                    }
                    if self.use_voc {
                        self.stretch.finish(&mut self.pending);
                    }
                    if self.pending.is_empty() {
                        return;
                    }
                    self.fx.process(&mut self.pending, ch, self.out.rate);
                    // Loop again: step 1 pushes the tail, then `eof` ends the loop.
                }
                Err(e) => {
                    self.error = Some(format!("{}: {e}", self.current_label()));
                    self.skip_streak += 1;
                    self.eof = true; // the track ends; tick() moves on to the next one
                    self.notify();
                    return;
                }
            }
        }
    }
}
