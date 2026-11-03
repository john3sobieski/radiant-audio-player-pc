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
- Phase vocoder (README 5.2): `radiant-dsp/src/stretch.rs`, port of TimeStretch.kt (same N, hops, peak detection, phase locking, stereo-sum phases, OLA, tail; hand-written radix-2 FFT as in Kotlin, no new crate). API: `TimeStretch::{new,set_speed,process,finish,reset,configure}`, `slider_to_speed`. 9 unit tests pass (pitch kept at 0.5..3x, length scales, no clicks, chunk-size independent, stereo image, mono/3ch, silence, on-the-fly start). NOT yet wired into the engine/UI (needs: Command::SetSpeed, speed slider, position/duration mapping by speed, reset on seek).
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

## Next (order from README section 11)
1. CI is green and the app runs (owner confirmed, incl. seek bar). Volume / shuffle / repeat are not compiled yet.
2. Investigate Windows 7/8.1 quickly and report to the owner.
3. Playback: test seek (MP3 without a length header has no seek: add a duration fallback), then skip bad files, device change, rubato resampler, measure CPU.
4. Library UI: tabs, search, sort, player bar, cover, quality badge; tag reading in `radiant-core`.
5. DSP: AudioFx chain DONE, settings wired to engine/UI (needs CI check). EQ presets DONE (see above). Next: EQ modes OFF/GLOBAL/PER_TRACK, then wire the vocoder (DSP part done in stretch.rs) into engine + UI.
6. Lyrics, track details, themes, languages (convert Android strings.xml by script, do not retranslate).
7. App lock, 8. Stems, 9. Dropbox, 10. SMTC/media keys, installer, user README.

## Open items
- Remember volume, shuffle, repeat, the music folder and the sound settings between launches (settings file, not written yet).
- Slint license: royalty-free option needs attribution; add notice.
- Opus decoder: symphonia has none; choose a solution.
- Settings persistence format not chosen yet (serde + JSON/TOML is the likely choice).

## Notes on DSP
- Normalization envelope follows spec (fast attack) so on a pure sine it settles ~15% below the 0.16 target; kept as in Android. Gain keeps adapting for ~9 s of silence until the envelope falls under the gate (1e-6), then freezes.
