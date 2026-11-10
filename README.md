# Radiant for PC — project brief

> **This file is written for an AI assistant (and any developer) who will build the PC version of Radiant.**
> Read it fully before writing code. It describes (1) the existing Android app that is the functional reference,
> (2) the goal and the fixed technology choices for the PC version, (3) exact algorithm parameters worth porting,
> and (4) how to work with the project owner.
> When the PC app is ready for users, replace this file with a normal user-facing README (the Android one is the model:
> short feature list, installation, license, translated into several languages).

---

## 1. Goal

Build a **desktop (PC) version of Radiant**: a simple, private music player for **local audio files** with the same
feature set and sound-processing quality as the Android app, written in **Rust**.

The owner's priorities, in this order:

1. **Optimization**: low CPU, low RAM, small binary, instant start, ~0 % CPU when idle or paused.
2. **Audio processing**: the DSP (equalizer, bass/treble, subsonic filter, normalization, limiter, pitch-preserving
   speed change, AI stem separation) must sound as good as on Android and must never glitch or click.
3. **Feature parity** with the Android app wherever it makes sense on a desktop (see section 4 for what does not carry over).

Principles inherited from the Android project: no ads, no accounts, no tracking, no internet needed
(the network is used only for the optional Dropbox download and for downloading the stem models), everything stays on the device.

### Target platform

**Windows** (decided by the owner). Minimum supported version: **Windows 10**, and **older versions (Windows 8.1 / 7) if that is
technically possible**. Primary architecture: **x86_64**; ARM64 Windows is optional and can come later.

- **Windows 10 and 11 are the guaranteed targets.** Everything in this brief (including stems) must work there.
- **Windows 7 / 8 / 8.1 are a best-effort stretch goal**, do not let it block the main version. As far as I know, current Rust no longer
  supports them as a normal target (the standard `windows-msvc` targets require Windows 10; Win7 exists only as a separate, lower-tier
  `x86_64-win7-windows-msvc` target that needs a nightly toolchain with `-Zbuild-std`), and other dependencies may drop them too
  (check ONNX Runtime and the Slint/windowing backend). Investigate early and report honestly what works. A sensible outcome could be:
  "Windows 10+ full version; optional legacy build for Windows 7/8.1 without stems (no `ort`) and with the software renderer".
  Do not promise legacy support to users until a real build was tested on such a system.
- Keep the code **platform-neutral** where it costs nothing (Rust, `cpal`, `symphonia`, Slint are cross-platform) and put Windows-specific
  parts (media controls, file dialogs, installer, autostart if ever needed) behind small modules, so Linux/macOS stay possible later.

---

## 2. Fixed technology choices

| Area | Choice | Notes |
|---|---|---|
| Language | **Rust** (stable, 2021+ edition) | |
| Audio output | **`cpal`** | Output only. On Windows it uses **WASAPI** (shared mode by default). Real-time callback rules in section 6. |
| Decoding | **`symphonia`** | MP3, FLAC, OGG/Vorbis, AAC, ALAC, WAV, tags. See the codec caveat below. |
| DSP | **Self-written** | Port of Android's `AudioFx.kt` and the phase vocoder in `TimeStretch.kt`. Do not replace it with a third-party effect library. |
| FFT | **`realfft` / `rustfft`** or **`rubato`** (free choice) | `rubato` is also handy for resampling (stems need 44.1 kHz). |
| AI stems | **`ort`** (ONNX Runtime) | One htdemucs (Demucs v4) ONNX model, downloaded in Settings (the Android app uses Open-Unmix; see PROGRESS.md stage 11). |
| UI | **Slint** | Keep it light: no web view, no heavy animation. |

Caveats to verify at the start (crate versions and capabilities change, check the current docs, do not trust this list blindly):

- **Opus.** As far as I know `symphonia` has no native Opus decoder. The Android app plays OGG/Opus, so find a decoder
  (libopus bindings or a pure-Rust one) and plug it in, or document the gap.
- **AAC.** `symphonia` decodes AAC-LC; HE-AAC support may be missing. Check against the owner's real files.
- **ALAC** is supported by `symphonia`, so the Android workaround (FFmpeg built only for ALAC) is **not needed** on PC. Do not add FFmpeg.
- **Slint license.** Slint is offered under several licenses (GPLv3, a royalty-free license with attribution, commercial).
  Radiant is under **PolyForm Noncommercial 1.0.0**, which is not GPL-compatible, so the project must use the
  royalty-free (attribution) option. Read the current Slint license terms and add the required attribution.
- **`ort` needs the ONNX Runtime native library** (`onnxruntime.dll`, downloaded or dynamically loaded). Decide how it is bundled in the release
  (size matters), and keep stems optional so the base app stays small. Check which Windows versions the chosen ONNX Runtime build supports
  (relevant for the Windows 7/8.1 goal).
- **Slint renderer.** Slint can use a GPU backend (OpenGL via femtovg, or Skia) or a software renderer. For a small footprint and for old/weak
  graphics drivers test the **software renderer** (or the lightest backend) first and compare memory and CPU; pick by measurement.

---

## 3. The Android app (reference implementation)

**Radiant** (formerly "Local Player") is a native Android music player written in **Kotlin + Jetpack Compose (Material 3)
+ AndroidX Media3 (ExoPlayer)**, with ONNX Runtime for stems. Package `com.example.localplayer`, ~13,000 lines.
Source (public repo of the owner, branch `main`): `https://github.com/john3sobieski/radiant-audio-player`.
The Android README in that repo is the authoritative user-level description; below is a condensed version plus the
engineering details a porter needs. **Always read the matching Kotlin file before porting a module.**

### 3.1 Features

- **Music folder.** The user picks one folder; Radiant scans it recursively and rescans on every launch. Nothing outside
  that folder is ever visible.
- **Cloud download (optional).** Dropbox (access type *App folder*, OAuth 2 with PKCE through the browser, so the app never sees
  a password). **One-way only**: music is downloaded from a cloud folder the user chooses into a `Dropbox` subfolder of the
  music folder. Nothing is uploaded, changed or deleted in the cloud, nothing local is deleted or overwritten.
- **Library.** Sections: Playlists, Albums (grouped automatically from tags), Tracks, Queues. Search and sorting in every section.
- **Playlists.** Create, rename, delete, add/remove tracks. The *Liked tracks* playlist always exists, is pinned on top and can't be
  renamed or deleted.
- **Queues.** Exactly four fixed queues (cannot be created or deleted); add/remove tracks, reorder by drag or "move up/down".
- **Reorder by drag** for playlists, albums and tracks (manual order is a sort mode of its own).
- **Sorting.** Tracks: manual, name, artist, album, duration, file date. Albums: manual, name, artist, track count, file date.
  Playlists: manual, name, created, updated, track count. Each has an ascending/descending toggle.
- **Player.** Play/pause, next/previous, seek bar, shuffle, repeat (off / all / one), cover art, quality badge (format + bitrate),
  300 ms fade-in and 250 ms fade-out on play/pause to avoid clicks.
- **Lyrics.** From tags (ID3 `USLT`/`SYLT`, Vorbis `LYRICS`, M4A `©lyr`) and from `.lrc` / `.txt` next to the track with the same
  name; rechecked on every scan. Synced (`.lrc`) lyrics scroll with the song, highlight the current line, fade lines already sung;
  tapping a line seeks to it. Plain lyrics get a compact view.
- **Playback speed.** 3x slower to 3x faster, step 0.1, **pitch preserved** (details in 5.2).
- **Sound.** Mono/stereo, volume normalization, bass boost, treble boost, subsonic filter, 12-band equalizer with 10 presets.
  EQ modes: off / one curve for all tracks / a separate curve remembered per track (the global curve and per-track curves are
  stored independently and never overwrite each other).
- **Stems (AI).** Split a track into vocals / drums / bass / other and mix any selection into **one** WAV (details in 5.3).
- **App lock** (off by default): 4-digit, 6-digit or custom code, recovery file with 12 random words (details in 5.4).
- **Track details:** tags, format, bitrate, sample rate, channels, file size, etc.
- **Themes:** light, true-black dark (OLED), follow system. **App language:** System + 10 languages (en, de, ru, uk, zh, ja, fr, cs, es, pl).
- **Power saving** (Off / Medium / Intensive): fewer animations, rarer progress updates, and in Intensive also forced black theme,
  volume capped at 40 %, mono, DSP bypassed, stems disabled. See section 4 for the desktop equivalent.
- **Formats:** MP3, FLAC, OGG/Opus, M4A/AAC, ALAC, WAV and whatever else the system can decode.

### 3.2 Data model (`Models.kt`)

- `Track { uri, title, artist, albumArtist, album, durationMs, trackNo, info: AudioInfo, modified, added, lyricsUri?, tagLyrics }`
- `Album { key, name, artist, tracks }`, `Playlist { id, name, trackUris, createdAt, updatedAt }`, `PlayQueue { id 0..3, trackUris }`
- Special values: `FAVORITES_ID = "favorites"`, `QUEUE_COUNT = 4`, `NO_ALBUM` / `NO_ARTIST` placeholders (displayed through translated strings,
  never stored translated).
- `SortKey` / `SortState(key, descending)` as listed above.

### 3.3 Source map: what to read for each module

| Android file | Content | PC plan |
|---|---|---|
| `AudioFx.kt` | Sound settings store, the `AudioFxProcessor` (EQ, bass, treble, subsonic, normalization, mono, limiter) | **Port as-is** (see 5.1) |
| `TimeStretch.kt` | `TimeStretchProcessor` and `PhaseVocoder` (pitch-preserving speed) | **Port as-is** (see 5.2) |
| `StemDsp.kt`, `Stems.kt`, `StemModels.kt`, `StemDecoder.kt`, `StemOrt.kt`, `StemService.kt` | Stem pipeline: decoding, STFT, ONNX passes, masks, WAV writing, model download/verification | **Port** (see 5.3); service becomes a background thread |
| `AppLock.kt`, `LockUi.kt` | Code hashing, brute-force delays, recovery file | **Port** the logic (see 5.4); redo the UI |
| `Lyrics.kt`, `LyricsUi.kt` | Tag/.lrc parsing, lyrics screen | Port parsing; redo UI |
| `Library.kt`, `Search.kt`, `ListTools.kt`, `Labels.kt` | Folder scan, grouping, sorting, drag reorder | Port logic; reorder UI in Slint |
| `Quality.kt`, `TrackInfo.kt`, `CoverStore.kt` | Format/bitrate detection, details, cover cache | Port logic (symphonia gives most of it) |
| `Cloud.kt`, `CloudUi.kt`, `CloudSyncService.kt` | Dropbox OAuth PKCE and one-way download | Port logic; use a loopback redirect instead of an Android URI scheme |
| `PlaybackService.kt`, `PlayerViewModel.kt` | Media3 player wiring, state, fades | **Do not port**; replaced by the cpal/symphonia engine, keep the behavior (fades, repeat, shuffle) |
| `PowerSave.kt`, `Motion.kt`, `Scrollbar.kt`, `Ui.kt`, ... | Android UI/Compose | Redo in Slint |
| `res/values*/strings.xml` | 320 strings x 10 languages | **Convert with a script**, do not retranslate (section 7) |

---

## 4. What does not carry over to PC

- **Media3 / ExoPlayer, `PlaybackService`, MediaSession, foreground services.** Replace with your own engine. On Windows use the OS media
  integration instead: media keys and the system transport controls (**SMTC**, System Media Transport Controls; check crates such as
  `souvlaki`). SMTC exists on Windows 10 and later; on older Windows fall back to plain media-key handling or skip it.
- **Storage Access Framework (SAF).** On Windows the music folder is a normal path (use a native folder dialog such as `rfd`).
  Handle Unicode paths (non-Latin names are common in this library) and long paths correctly; use `std::path`/`OsString`, never assume UTF-8 file names.
  Stems are written to `Stems/<artist - title>/` inside it; the Dropbox download goes to its `Dropbox` subfolder.
- **Android-specific UI:** Material You dynamic colors, notifications, the three-dots sheets, power-saving "Medium/Intensive" as such.
  Desktop mapping to decide with the owner: a single optional *Power saving / low-resource* mode (fewer redraws, DSP bypass,
  stems disabled) rather than three levels. The "volume capped at 40 %" and "no Bluetooth" rules are phone-specific and should be dropped.
- **FFmpeg ALAC build.** Not needed (see section 2).
- **Android signing, GitHub Actions APK workflow.** Replace with a Rust CI (section 8).

Features to keep identical in behavior: everything in 3.1 except the items above.

---

## 5. Algorithm reference (parameters from the Android code)

Everything below is already tuned by ear on Android; **keep the numbers** unless you measure a reason to change them.

### 5.1 Effects chain (`AudioFxProcessor`)

Input: 16-bit or float PCM, any channel count (stereo in practice). Internally `f64`. Order per frame:

1. **Mono downmix** (if enabled): `m = (L + R) / 2` into both channels.
2. **Normalization** (if enabled), a slow automatic gain on the frame's mean power:
   target RMS `0.16` (about -16 dBFS); power envelope follower with attack `0.25 s`, release `0.8 s`; gate at power `1e-6`
   (about -60 dBFS, below it the gain is frozen); desired gain `0.16 / sqrt(env)` clamped to `0.25 .. 2.8` (-12 dB .. +9 dB);
   gain smoothing with attack `0.3 s` (gain going down) and release `2.0 s` (going up). Coefficient: `1 - exp(-1 / (sampleRate * seconds))`.
3. **Filter chain** (all biquads, RBJ Audio EQ Cookbook formulas, direct form II transposed, state per channel per filter):
   - **12-band equalizer**, peaking filters at `32, 64, 125, 250, 500, 1000, 2000, 3500, 6000, 9000, 12500, 16000 Hz`, range ±12 dB.
     Q of each band depends on the distance to its neighbours so the bands overlap evenly:
     `lo = previous band (or f/2 for the first)`, `hi = next band (or 2f for the last)`,
     `octaves = ln(hi/lo) / ln 2 / 2`, `Q = 1 / (2 * sinh(ln2/2 * octaves))`.
     A band is skipped if `|gain| < 0.05 dB` or `f >= 0.45 * sampleRate`.
   - **Bass boost**: low shelf at `110 Hz`, slope S = 1, up to +15 dB.
   - **Treble boost**: high shelf at `6000 Hz`, slope S = 1, up to +15 dB (skipped if the corner is above `0.45 * sampleRate`).
   - **Subsonic filter**: intensity 0..100 % maps linearly to a cutoff of `12 .. 40 Hz`; two cascaded Butterworth high-pass
     sections with `Q1 = 0.5412` and `Q2 = 1.3066` (24 dB/oct). Active above 0.5 %.
   - **Preamp** before the filters: `10^(-0.5 * maxBoost / 20)` where `maxBoost` is the largest positive gain among EQ bands, bass and
     treble (half of the boost is compensated to hit the limiter less often).
   - When a filter is switched off its state is zeroed (prevents a click when it is re-enabled). Coefficients are recomputed only when a
     settings *version counter* changes (the audio thread reads atomics, settings are replaced whole, never mutated in place).
4. **Soft limiter** (only when normalization or any filter is active): linear up to `0.9`, then `0.9 + 0.1 * tanh((|x| - 0.9) / 0.1)`.
5. When nothing is active the buffer is copied through untouched (**zero cost bypass**; keep this property).

Equalizer presets: `EqPresets.kt` (10 presets). EQ modes: `OFF`, `GLOBAL`, `PER_TRACK` (per-track curves are persisted by track URI).

### 5.2 Speed change with preserved pitch (`TimeStretchProcessor`, `PhaseVocoder`)

Why self-written: the stock Sonic implementation (speech oriented) "chops" music when slowed down. The replacement is a **phase
vocoder with identity phase locking**.

- **Speed mapping.** UI slider position `t` in `-2 .. +2`, step 0.1: speed `= 1 + t` for `t >= 0`, `1 / (1 - t)` for `t < 0`
  (so `+2` = 3x faster, `-2` = 3x slower). The label shows `1..3x` or `-1..-3x`. The processor clamps speed to `0.25 .. 4`.
- **Pass-through.** At speed 1 (|speed - 1| < 0.001) and before the first change, audio goes through untouched with no delay and no CPU.
  The vocoder engine starts on the fly from the current position, so there is no gap.
- **Frame size** `N = 2^round(log2(sampleRate * 0.09))` clamped to `2048 .. 16384` (4096 at 44.1 kHz, about 90 ms), Hann window,
  synthesis hop `N / 4`, analysis hop `(N / 4) * speed`. Output frames always sit at a constant hop; input frames are taken with a variable step.
- **Phase locking.** Phases of all bins belonging to one spectral peak are advanced identically (removes the "metallic" tone of a plain vocoder);
  phases stay continuous between frames (no clicks).
- **Stereo.** The phase advance is computed from the **sum of channels** and applied to both, so the stereo image does not drift.
  Channels are processed in pairs (one forward and one inverse FFT per pair per frame). More than two channels: pairs of channels.
- **Overlap-add** normalized by the running sum of squared windows; the accumulator is shifted by one hop per frame.
- **Streaming**: bounded buffers; at end of stream append silence and flush the tail (`emitTail`).
- Cost: a few percent of one core. The FFT in Kotlin is hand-written radix-2 with tables; in Rust use `realfft`/`rustfft` with pre-planned FFTs.
- Read `PhaseVocoder.Group.process` for the exact phase-locking code before porting.

### 5.3 Stem separation (AI)

> **Superseded on PC (owner's decision):** the PC version uses Demucs (htdemucs, 4 stems, one ONNX network) instead of Open-Unmix. The requirements, the contract of the network and the status are in PROGRESS.md, stage 11. What follows describes the Android version; the output file names, the 16-bit WAV output, the temp-file idea and the progress / cancel behavior still apply.

- **Models.** Open-Unmix `umxhq` exported to ONNX (MIT, © 2019 Inria), one network per stem, four total, about 34 MB each. Not bundled: downloaded
  in Settings (SHA-256 verified) or imported from files (recognized by checksum, the file name does not matter).
  Hugging Face repos `MixDirective/open-unmix-umxhq-{vocals,drums,bass,other}-onnx`, file `model.onnx`
  (URL: `https://huggingface.co/<repo>/resolve/main/model.onnx`). Expected sizes and SHA-256:

  | Stem | Bytes | SHA-256 |
  |---|---|---|
  | vocals | 35,623,512 | `f27742bb52b24cb039614dc76d0764e279df3eb2a9d6daa82c6daee3369ec747` |
  | drums | 35,623,507 | `471628e1e48138a634f962a89f622929980cf5485ed0a4e6e648e9dfcd1145d9` |
  | bass | 35,623,502 | `8931033f04666fdcf300d610579351d74275a6c55dd4346009290c844033726f` |
  | other | 35,623,507 | `fe62c1d8fb8326c7e68540a675dfa186a2b131f763fb41050ea5e9a9859eb76e` |

- **Tensor.** Input and output of every network: magnitudes `[1, 2, 2049, frames]` (channel, bin, frame), index `(channel * 2049 + bin) * frames + frame`.
- **STFT.** 44.1 kHz stereo, `N_FFT = 4096`, `HOP = 1024` (2049 bins), centered STFT with reflection at the edges.
- **Blocks.** Core `512` hops (about 11.9 s) plus `86` hops (about 2 s) of context on each side; the context is discarded because the
  bidirectional LSTM is worse near block edges. Seamless stitching. Memory does not depend on track length (about 250 MB).
- **Masking.** All four networks always run; estimates `v` are combined with a soft mask `v^2 / sum(v^2)` so the stems add up to the original and none
  is too quiet. A selection of stems is mixed as the sum of the selected masks applied to the mix: "everything except bass" = full track without bass.
- **Decoding and resampling.** The track is decoded to 44.1 kHz stereo float (Android: system decoder; PC: `symphonia`, plus a resampler when the file
  is not 44.1 kHz, the Android code has a polyphase sinc resampler, `ZERO_CROSSINGS = 14`; `rubato` is the natural replacement). Multichannel files use the first two channels.
  Intermediate audio goes to a temporary float32 stereo file (about 85 MB per 4 minutes), removed afterwards.
- **Output.** 16-bit / 44.1 kHz stereo WAV in `Stems/<artist - title>/` inside the music folder, for example `... - No Bass.wav`, `... - Vocals + Drums.wav`.
  The name depends only on the selection (not on the language) so a repeat run overwrites the same file.
- **Behavior.** Not real-time; progress bar, remaining-time estimate, cancel; runs in the background; temp files cleaned on cancel/error.
  Known limits: plain ratio mask without Wiener filtering (slightly below reference quality); vocals/drums/bass are empty above about 16 kHz.
- **On PC:** run in a worker thread (or a few, but benchmark: ONNX Runtime already uses intra-op threads), use `ort` with CPU execution first, consider
  optional GPU execution providers later. Expect to be much faster than on a phone.

### 5.4 App lock (`AppLock.kt`)

- Code kinds: 4 digits, 6 digits, or custom (Latin letters and digits, 4..64 chars, no spaces). Asked on every launch and every return from the background.
- **No code is stored.** Store a random salt and a `PBKDF2-HMAC-SHA256` hash (**30,000 iterations**), compare in constant time.
- **Brute-force delay.** After every 5 wrong codes in a row: wait 30 s, then 1, 2, 4... minutes, up to 15 minutes. The delay survives restarts (persisted).
- **Recovery file.** On creating a code, generate **12 random words** from a built-in list of **700 English words** with a cryptographically secure RNG
  (700^12, about 113 bits), saved by the user as a `.txt`. Only a hash of the words is stored. The lock turns on only after the file has been written. Picking
  the file on the lock screen (*Forgot password*) disables the lock. Changing the code creates a new file and invalidates the old one.
- The lock protects the app UI only, not the audio files; music keeps playing while locked. Screen capture is blocked on the lock screen on Android (optional on PC).
- Keep `AppLock.kt` as the spec; the word list is in the Android source.

### 5.5 Lyrics, library and the rest

- **Lyrics parsing** (`Lyrics.kt`): ID3v2 `USLT`/`SYLT`, Vorbis comment `LYRICS`, MP4 `©lyr`, plus sidecar `.lrc`/`.txt`. `.lrc` timestamps drive the line index by binary search
  on the playback position; a small lead (`LEAD_MS`) is added so a line is highlighted slightly before it is sung.
- **Library scan** (`Library.kt`): recursive walk of the chosen folder, cached; checks lyrics availability on every scan; albums grouped by album artist + album tags with
  `NO_ALBUM`/`NO_ARTIST` fallbacks. On PC use a filesystem watcher or a rescan on start plus a manual rescan button; keep scans off the UI thread.
- **Cover art:** `CoverStore.kt` keeps a size-tiered bitmap cache. On PC cache decoded covers at a few fixed sizes and bound the cache.
- **Quality badge / details:** `Quality.kt`, `TrackInfo.kt` (format, bitrate, sample rate, channels, size).

---

## 6. Suggested architecture and engineering rules

Cargo workspace, one crate per concern (names are a suggestion):

- `radiant-core`: data model, library scan, playlists, queues, sorting, search, settings persistence, lyrics parsing, app-lock logic. No audio, no UI.
- `radiant-audio`: decoding thread (`symphonia`), ring buffer, `cpal` output stream, transport (play/pause/seek/repeat/shuffle, fades), gapless-friendly track switching.
- `radiant-dsp`: `AudioFx` chain, phase vocoder, resampler wrapper. Pure functions over `&mut [f32]`/`&mut [f64]`; unit-testable without audio devices.
- `radiant-stems`: STFT, ONNX passes, masks, WAV writer, model store (download, SHA-256, import).
- `radiant-cloud`: Dropbox OAuth PKCE and one-way download (blocking or async, but separate from the UI thread).
- `radiant-ui` (the binary): Slint UI, wiring, OS media controls.

**Real-time audio rules (the main reason for choosing Rust):**

- The `cpal` callback must not allocate, lock a mutex, do I/O or log. It only copies finished samples from a lock-free SPSC ring buffer (e.g. `rtrb` or `ringbuf`) into the device buffer.
- Decode and run the DSP on a dedicated thread that keeps the ring buffer filled; do not decode in the callback.
- Pass settings to the audio side through atomics plus a version counter (the Android pattern: settings are replaced whole, the audio thread rebuilds coefficients when the version changes).
- `f32` PCM end to end; the filter state and coefficients may stay `f64` (as on Android).
- Reuse buffers, no per-frame allocations; pre-plan FFTs; zero-cost bypass when an effect is off.
- Underrun handling: output silence and recover, never panic in the audio path.
- Handle device changes (headphones unplugged, default device switched) by re-opening the stream and resuming.

**Performance targets to measure and report (do not guess):** CPU while idle, paused and playing (plain, with full EQ chain, at 3x/-3x speed), RAM after scanning a large library,
cold start time, binary/installer size. Release profile: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
UI: redraw only on change (nothing animating while paused), update the seek bar at a modest rate, virtualize long lists, no polling loops.

**Testing the DSP:** biquad frequency responses against the RBJ formulas; phase vocoder: a sine at speed 0.5/2.0 keeps its frequency (FFT peak) and its length scales;
stems: the sum of all four stems must reproduce the input within a small error; app lock: known-answer PBKDF2 vectors and the delay schedule.

---

## 7. Localization

The Android app has **10 languages**: English (default), German, Russian, Ukrainian, Chinese, Japanese, French, Czech, Spanish, Polish,
about 320 strings each, in `app/src/main/res/values*/strings.xml` (Android XML). **Do not retranslate.** Write a conversion script from those files to the
chosen PC format (Slint supports gettext via `@tr(...)`, or use Fluent), keep the same keys where possible, and keep *System* as a language option that follows the OS locale,
falling back to English. The language can change at runtime without interrupting playback.

---

## 8. Build, CI and release

The owner builds through **GitHub Actions** and pastes build logs when something fails; there is no local toolchain assumption.

- Provide a workflow on a **`windows-latest`** runner (MSVC toolchain, target `x86_64-pc-windows-msvc`) that builds the release binary, caches `~/.cargo` and `target`, and uploads the artifact.
  Link the C runtime statically (`-C target-feature=+crt-static`) so users do not need the Visual C++ Redistributable.
- Release formats to consider: a **portable `.zip`** (exe plus `onnxruntime.dll` if stems are included) and an **installer** (MSI or NSIS, or MSIX). Keep the installer small and optional.
- The binary will be unsigned (code-signing certificates cost money), so Windows SmartScreen may warn on first run. Mention it in the final README ("More info, Run anyway") and
  consider signing later.
- If the Windows 7/8.1 stretch goal is pursued, add a separate, clearly named legacy workflow instead of complicating the main one.
- Never claim something compiles or works unless it was built; if you cannot run it, say so and give the owner the exact commands to test.
- Releases go to GitHub Releases like on Android (latest stable build there).

---

## 9. Working with the owner

- **Language:** the owner writes in **Russian**: reply in Russian, keep code, comments in English unless asked otherwise, and keep file names and identifiers in English.
- **Experience:** the owner does not write Rust or Kotlin; the code is produced with AI help. Explain decisions briefly and concretely, avoid unexplained jargon, give complete files
  or exact diffs (the owner uploads project archives and applies changes as whole files), and keep the project structure stable.
- **No feature loss.** When optimizing, remove only what plays no role at all; do not cut or change behavior without asking. When unsure whether something matters, ask.
- **Identity.** The author appears only as the nickname **john3sobieski** (GitHub `https://github.com/john3sobieski`). Do not ask for or insert a real name.
- **Support links** used in the Android README and `.github/FUNDING.yml`: Ko-fi `radiant_player`, Buy Me a Coffee `radiant_player`, GitHub Sponsors `john3sobieski`.
  Keep the same in the PC repository.

---

## 10. License and third-party components

- **Radiant is licensed under PolyForm Noncommercial 1.0.0** (`LICENSE.md`, with a *Required Notice* line naming the nickname). Free for noncommercial use; nobody may sell it or publish it for profit.
  Keep `LICENSE.md` and the notice in the PC project.
- Third-party code stays under its own licenses: `symphonia` (MPL-2.0), `cpal` (Apache-2.0), `ort` and ONNX Runtime (MIT / Apache-2.0), `rustfft`/`realfft`/`rubato` (MIT/Apache-2.0),
  Slint (see the license note in section 2), Open-Unmix (MIT, © 2019 Inria) with weights trained on MUSDB18-HQ (check the data/weights terms for any use beyond personal). Check each crate's current
  license and list them in a *Third-party components* section of the final README.

---

## 11. Suggested order of work

1. Investigate the Windows 7/8.1 question quickly (toolchain target, `ort`, Slint backend) and report what is realistic; confirm with the owner which Android features are must-have for version 1.
2. Skeleton workspace + Windows CI that produces a runnable binary.
3. Playback engine: folder scan, decode, `cpal` output, play/pause/seek/next/prev/shuffle/repeat, fades. Measure CPU.
4. Slint UI for the library (Playlists / Albums / Tracks / Queues), search, sorting, drag reorder, player view with cover and quality badge.
5. `AudioFx` chain (EQ, bass, treble, subsonic, normalization, mono, limiter) with unit tests; then the phase-vocoder speed control.
6. Lyrics (tags + `.lrc`), track details, themes, languages.
7. App lock.
8. Stems (`ort`), model manager.
9. Dropbox one-way download.
10. Windows media integration (SMTC / media keys), portable zip and installer, user-facing README, release.

Keep this file updated as decisions are made (platforms, crate versions, deviations from Android), so the next session starts from the current state.
