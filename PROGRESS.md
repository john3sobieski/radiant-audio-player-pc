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
- `radiant-audio`: `Transport` (queue / next / previous / repeat) with tests. No sound yet.
- `radiant-dsp`: settings struct, EQ band table, bypass skeleton + test. No filters yet.
- `radiant-stems`: constants only (models, sizes, SHA-256, STFT sizes).
- `radiant-cloud`: empty placeholder.
- `radiant-ui`: Slint window, "Choose music folder" button, background scan, list of titles.

## Next (order from README section 11)
1. Get CI green. Check `slint` / `rfd` versions still match (Cargo.toml says `slint = "1"`, `rfd = "0.15"`).
2. Investigate Windows 7/8.1 quickly and report to the owner.
3. Playback: symphonia decode + cpal output + ring buffer in `radiant-audio`, wire to UI
   (play/pause/next/prev), fades, then measure CPU.
4. Library UI: tabs, search, sort, player bar, cover, quality badge; tag reading in `radiant-core`.
5. DSP: AudioFx chain (README 5.1) with tests, then phase vocoder (5.2).
6. Lyrics, track details, themes, languages (convert Android strings.xml by script, do not retranslate).
7. App lock, 8. Stems, 9. Dropbox, 10. SMTC/media keys, installer, user README.

## Open items
- `LICENSE.md` has a placeholder: paste the full PolyForm Noncommercial 1.0.0 text.
- Slint license: royalty-free option needs attribution; add notice.
- Opus decoder: symphonia has none; choose a solution.
- Settings persistence format not chosen yet (serde + JSON/TOML is the likely choice).
