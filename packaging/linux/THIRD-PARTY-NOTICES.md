# Third-party components

Radiant itself is under the PolyForm Noncommercial License 1.0.0 (see `LICENSE.md`). It uses the components below,
each under its own license.

| Component | Use | License |
|---|---|---|
| [Slint](https://slint.dev) | interface | Slint Royalty-free License |
| [symphonia](https://github.com/pdeljanov/Symphonia) | audio decoding | MPL-2.0 |
| [libopus](https://opus-codec.org) (through `opusic-sys`) | Opus decoding | BSD-3-Clause |
| [cpal](https://github.com/RustAudio/cpal) | audio output | Apache-2.0 |
| [rubato](https://github.com/HEnquist/rubato) | resampling | MIT |
| [rtrb](https://github.com/mgeier/rtrb) | ring buffer | MIT or Apache-2.0 |
| [ort](https://github.com/pykeio/ort) | ONNX Runtime bindings | MIT or Apache-2.0 |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) (`libonnxruntime.so`) | AI stem separation | MIT |
| Demucs (htdemucs) model, Meta AI | AI stem separation (downloaded by the user) | MIT |
| [rfd](https://github.com/PolyMeilex/rfd) | file dialogs | MIT |
| [ureq](https://github.com/algesten/ureq) | downloads | MIT or Apache-2.0 |
| [image](https://github.com/image-rs/image) | cover art | MIT or Apache-2.0 |
| [souvlaki](https://github.com/Sinono3/souvlaki) | media keys (MPRIS) | MIT |

The full license text of every Rust crate is to be generated into the packages by the release build (see
`PROGRESS_Linux.md`, stage L4). The ONNX Runtime license text is taken from its release together with the library.
