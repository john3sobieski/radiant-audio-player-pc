//! The ONNX Runtime wrapper: implements [`Backend`] for the htdemucs ONNX model (CPU, or the GPU through DirectML).
//!
//! The runtime library is NOT linked: `ort` is used with `load-dynamic`, the library is loaded from
//! [`crate::runtime_library_path`] (set through `ORT_DYLIB_PATH` before the first session is made). Loading a missing
//! library would panic inside `ort`, so [`OrtBackend::load`] checks the file first.

use crate::demucs::{self, Backend, IoMap, SegmentOut, TensorInfo, CHANNELS, FRAMES, FREQ_BINS, MIX_LEN, SEGMENT_SAMPLES, SPEC_CHANNELS, SPEC_LEN, STEMS};
use crate::StemError;
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use ort::value::{Tensor, ValueType};
use std::path::{Path, PathBuf};

/// How the network is run. The GPU is tried in two variants; the last resort is always the CPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// DirectML with the basic graph optimizations only.
    GpuBasic,
    /// DirectML with every graph optimization off (the fused fp16 kernels are the usual cause of a silent result).
    GpuPlain,
    Cpu,
}

impl Mode {
    fn is_gpu(self) -> bool {
        self != Mode::Cpu
    }
}

/// Names of the four tensors of the model.
struct Io {
    mix_in: String,
    spec_in: String,
    wave_out: String,
    spec_out: String,
}

pub struct OrtBackend {
    session: Session,
    model: PathBuf,
    threads: usize,
    /// The modes still to try, in order; `current` is the one `session` was made with. The last one is the CPU.
    modes: Vec<Mode>,
    current: usize,
    io: Io,
    /// The current GPU mode has produced the same answer as the CPU on its first segment.
    verified: bool,
    /// A CPU session used as the reference for that first segment (becomes the main session if the GPU is dropped).
    reference: Option<Session>,
}

fn infer(e: impl std::fmt::Display) -> StemError {
    StemError::Inference(e.to_string())
}

fn dims(t: &ValueType) -> Vec<i64> {
    match t {
        ValueType::Tensor { shape, .. } => shape.iter().copied().collect(),
        _ => Vec::new(),
    }
}

/// DirectML needs the memory pattern optimization and parallel execution switched off (ONNX Runtime docs).
#[cfg(feature = "directml")]
fn with_gpu(builder: SessionBuilder, plain: bool) -> Result<SessionBuilder, StemError> {
    use ort::execution_providers::DirectMLExecutionProvider;
    use ort::session::builder::GraphOptimizationLevel;
    let level = if plain { GraphOptimizationLevel::Disable } else { GraphOptimizationLevel::Level1 };
    builder
        .with_optimization_level(level)
        .map_err(infer)?
        .with_memory_pattern(false)
        .map_err(infer)?
        .with_parallel_execution(false)
        .map_err(infer)?
        // `error_on_failure`: when the DLL has no DirectML or there is no card, we want an error (and the CPU
        // fallback), not a silent CPU session that is reported as "GPU".
        .with_execution_providers([DirectMLExecutionProvider::default().build().error_on_failure()])
        .map_err(infer)
}

#[cfg(not(feature = "directml"))]
fn with_gpu(_: SessionBuilder, _: bool) -> Result<SessionBuilder, StemError> {
    Err(StemError::Inference("this build has no DirectML support".into()))
}

fn build_session(model: &Path, threads: usize, mode: Mode) -> Result<Session, StemError> {
    let mut builder = Session::builder().map_err(infer)?;
    if mode.is_gpu() {
        builder = with_gpu(builder, mode == Mode::GpuPlain)?;
    }
    builder.with_intra_threads(threads.max(1)).map_err(infer)?.commit_from_file(model).map_err(infer)
}

/// A GPU result that cannot be right: a non-finite number, or (for a loud input) an all-zero answer.
fn output_is_sane(mix: &[f32], out: &SegmentOut) -> bool {
    let mut peak = 0.0_f32;
    for v in out.wave.iter().chain(out.spec_out.iter()) {
        if !v.is_finite() {
            return false;
        }
        peak = peak.max(v.abs());
    }
    // The input is normalized to unit variance, so 0.05 is clearly "not silence" and the stems add up to the input.
    let input_peak = mix.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    !(input_peak > 0.05 && peak < 1e-4)
}

/// `a` is close to the reference `b` (relative L2 error up to 25 %, plus a tiny absolute allowance so that a silent
/// reference does not accept noise). fp16 differences are around 1 %; DirectML garbage (noise, zeros) is far off.
fn close_to(a: &[f32], b: &[f32]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let (mut err, mut norm) = (0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b) {
        if !x.is_finite() {
            return false;
        }
        let d = (*x - *y) as f64;
        err += d * d;
        norm += (*y as f64) * (*y as f64);
    }
    err.sqrt() <= 0.25 * norm.sqrt() + 1e-3 * (a.len() as f64).sqrt()
}

fn matches_reference(out: &SegmentOut, reference: &SegmentOut) -> bool {
    close_to(&out.wave, &reference.wave) && close_to(&out.spec_out, &reference.spec_out)
}

fn run_session(session: &mut Session, io: &Io, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError> {
    debug_assert_eq!(mix.len(), MIX_LEN);
    debug_assert_eq!(spec.len(), SPEC_LEN);
    let mix_t = Tensor::from_array(([1_i64, CHANNELS as i64, SEGMENT_SAMPLES as i64], mix.to_vec())).map_err(infer)?;
    let spec_t =
        Tensor::from_array(([1_i64, SPEC_CHANNELS as i64, FREQ_BINS as i64, FRAMES as i64], spec.to_vec())).map_err(infer)?;
    let outputs = session.run(vec![(io.mix_in.clone(), mix_t), (io.spec_in.clone(), spec_t)]).map_err(infer)?;
    let (_, wave) = outputs[io.wave_out.as_str()].try_extract_tensor::<f32>().map_err(infer)?;
    let (_, spec_out) = outputs[io.spec_out.as_str()].try_extract_tensor::<f32>().map_err(infer)?;
    if wave.len() != STEMS * MIX_LEN || spec_out.len() != STEMS * SPEC_LEN {
        return Err(StemError::BadModel(format!(
            "unexpected output sizes: {} and {} floats",
            wave.len(),
            spec_out.len()
        )));
    }
    Ok(SegmentOut { wave: wave.to_vec(), spec_out: spec_out.to_vec() })
}

impl OrtBackend {
    /// Loads the model. `threads` = intra-op threads (keep it below the core count so the music does not crackle;
    /// the CPU path only). `gpu` asks for DirectML; when it cannot be used (plain CPU `onnxruntime.dll`, no
    /// DirectX 12 card, driver problem) the model is loaded on the CPU instead and [`OrtBackend::using_gpu`] says so.
    /// While running, a GPU that returns garbage is dropped the same way (see [`OrtBackend::run`]).
    pub fn load(model: &Path, threads: usize, gpu: bool) -> Result<Self, StemError> {
        let lib = crate::runtime_library_path().filter(|p| p.is_file()).ok_or(StemError::NoRuntime)?;
        std::env::set_var("ORT_DYLIB_PATH", &lib);
        let modes = if gpu { vec![Mode::GpuBasic, Mode::GpuPlain, Mode::Cpu] } else { vec![Mode::Cpu] };
        let last = modes.len() - 1;
        let mut current = 0;
        let session = loop {
            match build_session(model, threads, modes[current]) {
                Ok(session) => break session,
                // The GPU session cannot even be made: no point trying the other GPU variant.
                Err(_) if current < last => current = last,
                Err(e) => return Err(e),
            }
        };
        let inputs: Vec<TensorInfo> =
            session.inputs.iter().map(|i| TensorInfo { name: i.name.clone(), dims: dims(&i.input_type) }).collect();
        let outputs: Vec<TensorInfo> =
            session.outputs.iter().map(|o| TensorInfo { name: o.name.clone(), dims: dims(&o.output_type) }).collect();
        let IoMap { mix_in, spec_in, wave_out, spec_out } = demucs::check_io(&inputs, &outputs).map_err(StemError::BadModel)?;
        let on_gpu = modes[current].is_gpu();
        Ok(Self {
            io: Io {
                mix_in: inputs[mix_in].name.clone(),
                spec_in: inputs[spec_in].name.clone(),
                wave_out: outputs[wave_out].name.clone(),
                spec_out: outputs[spec_out].name.clone(),
            },
            session,
            model: model.to_path_buf(),
            threads,
            modes,
            current,
            // The CPU needs no check.
            verified: !on_gpu,
            reference: None,
        })
    }

    /// The session really runs on the GPU right now (false = CPU, also when the GPU was asked for but was not
    /// available or was dropped in the middle of the job). Ask after the job, not before.
    pub fn using_gpu(&self) -> bool {
        self.modes[self.current].is_gpu()
    }

    /// Moves to the next mode (GPU variant -> CPU) and makes a new session for it (the CPU reference session is
    /// reused when there is one).
    fn step_down(&mut self) -> Result<(), StemError> {
        let last = self.modes.len() - 1;
        loop {
            if self.current >= last {
                return Err(StemError::Inference("no mode left to try".into()));
            }
            self.current += 1;
            self.verified = false;
            if self.current == last {
                self.verified = true; // the CPU is the reference itself
                self.session = match self.reference.take() {
                    Some(session) => session,
                    None => build_session(&self.model, self.threads, Mode::Cpu)?,
                };
                return Ok(());
            }
            match build_session(&self.model, self.threads, self.modes[self.current]) {
                Ok(session) => {
                    self.session = session;
                    return Ok(());
                }
                Err(_) => self.current = last - 1, // skip to the CPU
            }
        }
    }

    fn run_reference(&mut self, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError> {
        if self.reference.is_none() {
            self.reference = Some(build_session(&self.model, self.threads, Mode::Cpu)?);
        }
        let session = self.reference.as_mut().expect("just made");
        run_session(session, &self.io, mix, spec)
    }
}

impl Backend for OrtBackend {
    /// On the CPU: as it is. On the GPU the first segment of every GPU mode is also run on the CPU and the two answers
    /// are compared (DirectML with this fp16 model may return noise or zeros without any error); later segments get a
    /// cheap sanity check. A mode that fails is dropped (next GPU variant, then the CPU) and the same segment is run
    /// again, so the files never get garbage.
    fn run(&mut self, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError> {
        loop {
            let result = run_session(&mut self.session, &self.io, mix, spec);
            if !self.using_gpu() {
                return result;
            }
            if self.verified {
                match result {
                    Ok(out) if output_is_sane(mix, &out) => return Ok(out),
                    _ => self.step_down()?,
                }
                continue;
            }
            let reference = self.run_reference(mix, spec)?;
            match result {
                Ok(out) if matches_reference(&out, &reference) => {
                    self.verified = true;
                    self.reference = None; // frees the second copy of the model
                    return Ok(out);
                }
                _ => {
                    self.step_down()?;
                    if !self.using_gpu() {
                        return Ok(reference);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(value: f32) -> SegmentOut {
        SegmentOut { wave: vec![value; 8], spec_out: vec![value; 8] }
    }

    #[test]
    fn silent_or_broken_gpu_output_is_rejected() {
        let loud = vec![0.5_f32; 16];
        let quiet = vec![0.0_f32; 16];
        assert!(output_is_sane(&loud, &out(0.3)));
        assert!(!output_is_sane(&loud, &out(0.0)), "zeros for a loud input");
        assert!(!output_is_sane(&loud, &out(f32::NAN)));
        assert!(!output_is_sane(&loud, &out(f32::INFINITY)));
        assert!(output_is_sane(&quiet, &out(0.0)), "silence in, silence out is fine");
    }

    #[test]
    fn gpu_answer_is_compared_with_the_cpu() {
        let reference: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.1).sin()).collect();
        let fp16_like: Vec<f32> = reference.iter().map(|v| v * 1.01).collect();
        let noise: Vec<f32> = (0..1000).map(|i| ((i * 7919 % 1000) as f32 / 500.0) - 1.0).collect();
        assert!(close_to(&fp16_like, &reference));
        assert!(!close_to(&noise, &reference));
        assert!(!close_to(&vec![0.0; 1000], &reference));
        assert!(!close_to(&noise, &vec![0.0; 1000]), "noise is not accepted for a silent reference");
        assert!(close_to(&vec![0.0; 1000], &vec![0.0; 1000]));
    }
}
