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
- `radiant-dsp`: settings struct, EQ band table, bypass skeleton + test. No filters yet.
- `radiant-stems`: constants only (models, sizes, SHA-256, STFT sizes).
- `radiant-cloud`: empty placeholder.
- `radiant-ui`: Slint window, folder button, background scan, restyled dark UI (custom widgets, `Theme` global, current-row highlight, player bar with icon buttons).

## Next (order from README section 11)
1. CI is green and the app runs (owner's screenshot). Newer code (seek bar) is not compiled yet.
2. Investigate Windows 7/8.1 quickly and report to the owner.
3. Playback: test seek (MP3 without a length header has no seek: add a duration fallback), then shuffle,
   repeat button, skip bad files, device change, rubato resampler, measure CPU.
4. Library UI: tabs, search, sort, player bar, cover, quality badge; tag reading in `radiant-core`.
5. DSP: AudioFx chain (README 5.1) with tests, then phase vocoder (5.2).
6. Lyrics, track details, themes, languages (convert Android strings.xml by script, do not retranslate).
7. App lock, 8. Stems, 9. Dropbox, 10. SMTC/media keys, installer, user README.

## Open items
- Slint license: royalty-free option needs attribution; add notice.
- Opus decoder: symphonia has none; choose a solution.
- Settings persistence format not chosen yet (serde + JSON/TOML is the likely choice).
