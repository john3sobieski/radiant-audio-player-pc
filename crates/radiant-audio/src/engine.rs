//! Engine thread. It owns the decoder and the cpal stream (`cpal::Stream` is not `Send` on
//! Windows, so it must be created and used on one thread). The UI talks to it through `Player`.
//!
//! Real-time rule: the cpal callback does not allocate, lock or do I/O. It pops samples from the
//! ring buffer and applies a fade gain. Everything else happens on this thread.

use crate::convert::Converter;
use crate::decode::TrackDecoder;
use crate::transport::{PlayState, RepeatMode, Transport};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use radiant_dsp::{AudioFxProcessor, AudioFxSettings};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FADE_IN_SECS: f32 = 0.3;
const FADE_OUT_SECS: f32 = 0.25;
/// Ring buffer length in seconds of audio.
const RING_SECS: usize = 1;
/// How often the engine tops up the ring buffer while playing.
const FILL_INTERVAL: Duration = Duration::from_millis(50);
/// After pausing, keep the stream alive this long (fade-out), then pause it (0 % CPU).
const PAUSE_STREAM_AFTER: Duration = Duration::from_millis(450);
/// How often the UI gets a position update while playing.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);

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
        let thread = thread::Builder::new()
            .name("radiant-engine".into())
            .spawn(move || run(rx, Box::new(on_change)))
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
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels as usize;
    let rate = config.sample_rate.0;

    let capacity = rate as usize * channels * RING_SECS;
    let (prod, mut cons) = rtrb::RingBuffer::<f32>::new(capacity);
    let playing = Arc::new(AtomicBool::new(false));
    let flush_req = Arc::new(AtomicU32::new(0));
    let flush_ack = Arc::new(AtomicU32::new(0));
    let played = Arc::new(AtomicU64::new(0));

    let cb_playing = playing.clone();
    let cb_req = flush_req.clone();
    let cb_ack = flush_ack.clone();
    let cb_played = played.clone();
    let up = 1.0 / (FADE_IN_SECS * rate as f32);
    let down = 1.0 / (FADE_OUT_SECS * rate as f32);
    let mut gain = 0.0_f32;
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
                    for s in frame.iter_mut() {
                        *s = cons.pop().unwrap_or(0.0) * gain;
                    }
                    frames_out += 1;
                }
                if frames_out > 0 {
                    cb_played.fetch_add(frames_out, Ordering::Relaxed);
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    // Some backends start the stream right away; start paused so idle costs nothing.
    let _ = stream.pause();

    Ok(Output { stream, prod, capacity, rate, channels, playing, flush_req, flush_ack, played })
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
    fx: AudioFxProcessor,
    on_change: Box<dyn Fn(Status) + Send>,
    /// When set, the stream is paused after this moment (fade-out finished).
    pause_deadline: Option<Instant>,
    error: Option<String>,
    duration_ms: Option<u64>,
    /// Where the current track's playback started (after a seek), ms.
    offset_ms: u64,
    /// Value of `out.played` when the current track (or seek) started.
    track_start_frames: u64,
    last_notify: Cell<Instant>,
}

fn run(rx: Receiver<Command>, on_change: Box<dyn Fn(Status) + Send>) {
    let out = match open_output() {
        Ok(o) => o,
        Err(e) => {
            on_change(Status {
                state: PlayState::Stopped,
                current: None,
                position_ms: 0,
                duration_ms: None,
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
    let mut eng = Engine {
        out,
        transport: Transport::default(),
        decoder: None,
        conv: None,
        raw: Vec::new(),
        pending: Vec::new(),
        pending_pos: 0,
        eof: false,
        fx: AudioFxProcessor::new(AudioFxSettings::default()),
        on_change,
        pause_deadline: None,
        error: None,
        duration_ms: None,
        offset_ms: 0,
        track_start_frames: 0,
        last_notify: Cell::new(Instant::now()),
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
        let frames = self
            .out
            .played
            .load(Ordering::Relaxed)
            .saturating_sub(self.track_start_frames);
        let ms = self.offset_ms + frames * 1000 / self.out.rate.max(1) as u64;
        self.duration_ms.map_or(ms, |d| ms.min(d))
    }

    fn notify(&self) {
        self.last_notify.set(Instant::now());
        (self.on_change)(Status {
            state: self.transport.state,
            current: self.transport.current().map(|p| p.to_path_buf()),
            position_ms: self.position_ms(),
            duration_ms: self.duration_ms,
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
            Command::Quit => return false,
        }
        self.notify();
        true
    }

    /// Called after every wake-up: top up the buffer, detect the end of the track, pause the stream.
    fn tick(&mut self) {
        match self.transport.state {
            PlayState::Playing => {
                self.fill();
                if self.last_notify.get().elapsed() >= POSITION_INTERVAL {
                    self.notify();
                }
                let drained = self.out.prod.slots() == self.out.capacity;
                if self.eof && self.pending_pos >= self.pending.len() && drained {
                    // Track ended by itself. The ring buffer is empty, so no flush is needed.
                    if self.transport.next(true).is_some() {
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
        self.conv = None; // restart the resampler
        self.flush();
        self.track_start_frames = self.out.played.load(Ordering::Relaxed);
        self.offset_ms = ms;
        self.fill();
    }

    /// Opens the current track, fills the buffer and starts output.
    fn play_current(&mut self, flush: bool) {
        self.decoder = None;
        self.conv = None;
        self.pending.clear();
        self.pending_pos = 0;
        self.eof = false;
        self.error = None;
        self.duration_ms = None;
        self.offset_ms = 0;
        let Some(path) = self.transport.current().map(|p| p.to_path_buf()) else {
            self.stop();
            return;
        };
        match TrackDecoder::open(&path) {
            Ok(d) => {
                self.duration_ms = d.duration_ms;
                self.decoder = Some(d);
                if flush {
                    self.flush();
                }
                self.track_start_frames = self.out.played.load(Ordering::Relaxed);
                self.fill();
                self.start();
            }
            Err(e) => {
                // TODO: skip unreadable files instead of stopping.
                self.error = Some(format!("{}: {e}", path.display()));
                self.stop();
            }
        }
    }

    fn start(&mut self) {
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
            let Some(dec) = self.decoder.as_mut() else { return };
            match dec.next_chunk(&mut self.raw) {
                Ok(Some((rate, src_ch))) => {
                    if self.conv.is_none() {
                        self.conv = Some(Converter::new(rate, src_ch, self.out.rate, ch));
                    }
                    if let Some(c) = self.conv.as_mut() {
                        c.process(&self.raw, &mut self.pending);
                    }
                    self.fx.process(&mut self.pending, ch, self.out.rate);
                }
                Ok(None) => {
                    self.eof = true;
                    return;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.eof = true;
                    self.notify();
                    return;
                }
            }
        }
    }
}
