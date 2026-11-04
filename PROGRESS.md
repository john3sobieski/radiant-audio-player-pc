# PROGRESS (notes for the next session — read this first, then README.md)

README.md = the project brief (keep it until the user-facing README is written).
Language with the owner: Russian. Code and comments: English.

## Status
Base workspace created. **Nothing here has been compiled yet** (no Rust toolchain in the session
where it was written). First thing to do: push to GitHub, run the `build` workflow, fix errors
from the log the owner pastes.

## Done
- Cargo workspace, 6 crates (layout from README section 6), release profile, static CRT, Windows CI.
- `radiant-core`: models, recursive folder scan (file names only), album grouping, track sorting + tests.
- `radiant-audio`: playback engine written (symphonia decode -> convert -> DSP -> rtrb ring -> cpal),
  play/pause/next/prev/repeat, fades, auto-advance, stream paused when idle. **Untested: never compiled or heard.**
  Modules: transport, decode, convert, engine (see header of `lib.rs`).
- `radiant-dsp`: AudioFx chain per README 5.1 (RBJ biquads, 12-band EQ with Q formula, bass/treble shelves, subsonic 24 dB/oct, preamp, normalization, soft limiter, mono, zero-cost bypass). 13 unit tests pass (run standalone, `cargo test` on this crate alone; no network in that session). Engine still passes `AudioFxSettings::default()` (bypass): settings are not wired to UI/engine yet.
- EQ presets: `radiant-dsp/src/presets.rs` (10 presets, checked identical to EqPresets.kt by script, 3 unit tests pass). UI: two rows of preset chips in the Sound panel (`eq-preset-selected` callback in `main.rs`). Names are English for now (keys `rock`, `pop`... = Android `preset_*`), translate with localization. UI part NOT compiled yet. EQ modes OFF/GLOBAL/PER_TRACK still to do.
- Phase vocoder (README 5.2): `radiant-dsp/src/stretch.rs`, port of TimeStretch.kt (same N, hops, peak detection, phase locking, stereo-sum phases, OLA, tail; hand-written radix-2 FFT as in Kotlin, no new crate). API: `TimeStretch::{new,set_speed,process,finish,reset,configure}`, `slider_to_speed`. 9 unit tests pass (pitch kept at 0.5..3x, length scales, no clicks, chunk-size independent, stereo image, mono/3ch, silence, on-the-fly start). WIRED (NOT compiled yet): `Command::SetSpeed(f32)` in `radiant-audio`; vocoder sits after the resampler and before the effects in `Engine::fill` (pass-through at speed 1, no extra copy); reset on seek / new track; tail played out at end of track; `position.rs` (`PositionTracker`, 3 unit tests) keeps position/progress in SOURCE time while the speed changes (a change is registered at the output frame where it becomes audible). UI: "Speed" row at the top of the Sound panel (slider -2..+2, step 0.1, label 1.0x..3.0x / -1.0x..-3.0x), `speed-changed` -> `slider_to_speed` -> `SetSpeed`. Speed is NOT reset by the panel's Reset button and NOT saved between launches yet. Known: after returning to 1.0x mid-track the vocoder keeps running until the next seek/track (costs a little CPU); position can be off by the vocoder latency (~90 ms) right after a change.
- CI result for the code below (session 5): it COMPILED with rubato 5 / symphonia / cpal; 28 of 30 tests passed. The 2 failures were test mistakes, fixed: (1) the ramp WAV wraps at 30011, expectations are now taken modulo that (seek itself was exact); (2) rubato reports its filter delay in whole frames, so the resampled sine can be off by a fraction of a frame (error 0.0228 = 0.35 frame at 1 kHz/48 kHz), the test now allows a slide of up to 1 frame and checks the residual shape error < 0.005. Fixed tests not yet re-run in CI.
- Resampler + seek + duration (session 4, was NOT compiled with the real crates: crates.io was blocked in that session, so `rubato`, `symphonia`, `cpal` could not be built; syntax checked with rustfmt, logic tested with stand-ins):
  - `radiant-audio/src/sinc_core.rs`: the only file that uses `rubato` (**version 5.x API**, read from the 5.0.1 source; `Async::new_sinc`, fixed input chunk 1024, sinc 128 taps, Blackman-Harris^2, cubic). `Cargo.toml`: `rubato = { version = "5", default-features = false }` (no FFT resampler, no realfft). If CI complains about the rubato API, fix only this file.
  - `convert.rs`: streaming wrapper around the core (collects whole chunks, trims the filter delay at the start, `finish()` flushes the tail and cuts to the exact length `round(frames * ratio)`, `reset()` after a seek keeps the filter tables). Falls back to the old linear interpolator if rubato cannot be created. 13 unit tests: the bookkeeping part (fake core, 5 tests) was run and passes; the tests that use the real core (sine 44.1->48 kHz must match an ideal sine within 0.01, exact length, piece-size independence, reset) will run for the first time in CI.
  - `engine.rs`: resampler tail is played out at the end of a track (before the vocoder tail), seek calls `Converter::reset` instead of rebuilding.
  - `decode.rs`: seek is now sample-accurate (symphonia lands on a packet boundary; the frames before the target are dropped). Duration fallback: `needs_scan` is set when the container gives no length, or for a raw MP3 without Xing/Info/VBRI header (symphonia 0.5.4 only ESTIMATES the length from the first frame's bitrate then, wrong for VBR). The engine runs `scan_duration` (walks packets, no decoding) on a helper thread and receives `Command::DurationScanned` (hidden variant, sent to itself); a result for a track that is no longer current is ignored. Until it arrives the seek bar uses the estimate or is off (duration `None`).
  - Tests added in `decode.rs` (run in CI): ramp WAV where every sample encodes its frame number, so seek accuracy is checked exactly (forward, backward, near the end), exact scan duration. The pure MP3-header detection tests (ID3 size, Xing/Info/VBRI) were run here and pass. NOT tested on real MP3 files: to check by hand with ffmpeg: `ffmpeg -i in.wav -c:a libmp3lame -b:a 192k -write_xing 0 headerless.mp3` (CBR, no header) and `-q:a 2 -write_xing 0` (VBR, no header), then play and drag the seek bar.

- `radiant-stems`: constants only (models, sizes, SHA-256, STFT sizes).
- `radiant-cloud`: empty placeholder.
- Volume (smoothed, squared curve, mute by clicking the speaker), shuffle, repeat button: written, not compiled yet.
- Sound settings wired end to end (NOT compiled yet): `Command::SetFx(AudioFxSettings)` in `radiant-audio`
  (applied on the engine thread, `AudioFxSettings` re-exported from `radiant-audio`); UI "Sound" button opens an
  overlay panel (Normalize, Mono, Reset, Bass, Treble, Subsonic, 12 EQ sliders). Rust (`main.rs`) owns the settings
  and sends the whole set on every change. Slider steps: 0.5 dB (EQ/bass/treble), 1 % (subsonic).
  The ring buffer was shortened from 1000 ms to `RING_MS = 400` so a change is audible within 0.4 s
  (the DSP runs before the ring buffer). If crackling appears on weak machines, raise `RING_MS` in `engine.rs`.
- Slint: `VolumeBar` callback renamed `changed` -> `edited` (avoids a possible clash with Slint's `changed` keyword).
- `radiant-ui`: Slint window, folder button, background scan, restyled dark UI (custom widgets, `Theme` global, current-row highlight, player bar with icon buttons).

- UI fixes (NOT compiled yet): player-bar control row now uses absolute positions (title elides in the space left of the centered transport block, volume pinned right), so a long track title no longer shifts Shuffle/Prev/Play/Next/Repeat; window min-width raised 480 -> 640 so blocks cannot overlap. Mouse wheel over any slider (seek, volume, all sound sliders): `scroll-event` on each slider's TouchArea; 1 notch (= 60 px in Slint) = 5 s seek / 5 % volume / `wheel-step` (0.5 dB EQ/bass/treble, 0.1 speed, 1 % subsonic). `duration-secs` property fed from Rust.

## Next (order from README section 11)
1. Playback: CI check of the new resampler/seek code first (read the CI log: rubato API, the real-core tests). Then skip bad files, device change, measure CPU (resampler 44.1->48 kHz included). Listen to a 44.1 kHz file on a 48 kHz device (ends of tracks, seek).
2. Library UI: tabs, search, sort, player bar, cover, quality badge; tag reading in `radiant-core`.
3. DSP: AudioFx chain DONE, settings wired to engine/UI (needs CI check). EQ presets DONE (see above). Next: EQ modes OFF/GLOBAL/PER_TRACK. Vocoder is wired (check by ear in CI build: 0.5x, 2x, 3x, seek while slowed, end of track at 0.5x).
4. Lyrics, track details, themes, languages (convert Android strings.xml by script, do not retranslate).
5. App lock, 
6. Stems, 
7. Dropbox, 
8. SMTC/media keys, 
9. installer, 
10. user README.

## Open items
- Remember volume, shuffle, repeat, the music folder and the sound settings between launches (settings file, not written yet).
- Slint license: royalty-free option needs attribution; add notice.
- Opus decoder: symphonia has none; choose a solution.
- Settings persistence format not chosen yet (serde + JSON/TOML is the likely choice).

## Notes on DSP
- Normalization envelope follows spec (fast attack) so on a pure sine it settles ~15% below the 0.16 target; kept as in Android. Gain keeps adapting for ~9 s of silence until the envelope falls under the gate (1e-6), then freezes.
