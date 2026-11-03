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
- `radiant-ui`: Slint window, folder button, background scan, track list (click = play), Prev/Play/Next buttons.

## Next (order from README section 11)
1. Get CI green. Check `slint` / `rfd` versions still match (Cargo.toml says `slint = "1"`, `rfd = "0.15"`).
2. Investigate Windows 7/8.1 quickly and report to the owner.
3. Playback: verify it compiles and plays (rtrb 0.3 `write_chunk_uninit`, cpal 0.15, symphonia 0.5 APIs
   were written from memory). Then: seek + position, shuffle, repeat button, skip bad files,
   device change, rubato resampler, measure CPU.
4. Library UI: tabs, search, sort, player bar, cover, quality badge; tag reading in `radiant-core`.
5. DSP: AudioFx chain (README 5.1) with tests, then phase vocoder (5.2).
6. Lyrics, track details, themes, languages (convert Android strings.xml by script, do not retranslate).
7. App lock, 8. Stems, 9. Dropbox, 10. SMTC/media keys, installer, user README.

## Open items
- Slint license: royalty-free option needs attribution; add notice.
- Opus decoder: symphonia has none; choose a solution.
- Settings persistence format not chosen yet (serde + JSON/TOML is the likely choice).
