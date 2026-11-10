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

pub struct OrtBackend {
    session: Session,
    model: PathBuf,
    threads: usize,
    /// The modes still to try, in order; `current` is the one `session` was made with. The last one is the CPU.
    modes: Vec<Mode>,
    current: usize,
    mix_in: String,
    spec_in: String,
    wave_out: String,
    spec_out: String,
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

/// A GPU result that cannot be right: a non-finite number, or (for a loud input) an all-zero answer. DirectML with
/// fp16 weights sometimes does this without any error; the file would then hold silence (only the track's mean level).
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
        Ok(Self {
            mix_in: inputs[mix_in].name.clone(),
            spec_in: inputs[spec_in].name.clone(),
            wave_out: outputs[wave_out].name.clone(),
            spec_out: outputs[spec_out].name.clone(),
            session,
            model: model.to_path_buf(),
            threads,
            modes,
            current,
        })
    }

    /// The session really runs on the GPU right now (false = CPU, also when the GPU was asked for but was not
    /// available or was dropped in the middle of the job). Ask after the job, not before.
    pub fn using_gpu(&self) -> bool {
        self.modes[self.current].is_gpu()
    }

    /// Moves to the next mode (GPU variant -> CPU) and makes a new session for it.
    fn step_down(&mut self) -> Result<(), StemError> {
        let last = self.modes.len() - 1;
        loop {
            if self.current >= last {
                return Err(StemError::Inference("no mode left to try".into()));
            }
            self.current += 1;
            match build_session(&self.model, self.threads, self.modes[self.current]) {
                Ok(session) => {
                    self.session = session;
                    return Ok(());
                }
                Err(_) if self.current < last => self.current = last - 1, // skip to the CPU
                Err(e) => return Err(e),
            }
        }
    }

    fn run_once(&mut self, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError> {
        debug_assert_eq!(mix.len(), MIX_LEN);
        debug_assert_eq!(spec.len(), SPEC_LEN);
        let mix_t = Tensor::from_array(([1_i64, CHANNELS as i64, SEGMENT_SAMPLES as i64], mix.to_vec())).map_err(infer)?;
        let spec_t = Tensor::from_array((
            [1_i64, SPEC_CHANNELS as i64, FREQ_BINS as i64, FRAMES as i64],
            spec.to_vec(),
        ))
        .map_err(infer)?;
        let outputs = self.session.run(vec![(self.mix_in.clone(), mix_t), (self.spec_in.clone(), spec_t)]).map_err(infer)?;
        let (_, wave) = outputs[self.wave_out.as_str()].try_extract_tensor::<f32>().map_err(infer)?;
        let (_, spec_out) = outputs[self.spec_out.as_str()].try_extract_tensor::<f32>().map_err(infer)?;
        if wave.len() != STEMS * MIX_LEN || spec_out.len() != STEMS * SPEC_LEN {
            return Err(StemError::BadModel(format!(
                "unexpected output sizes: {} and {} floats",
                wave.len(),
                spec_out.len()
            )));
        }
        Ok(SegmentOut { wave: wave.to_vec(), spec_out: spec_out.to_vec() })
    }
}

impl Backend for OrtBackend {
    /// On the GPU every answer is checked; a failed call or a garbage answer drops the GPU for the rest of the job
    /// (next GPU variant, then the CPU) and the same segment is run again, so the result is never silence.
    fn run(&mut self, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError> {
        loop {
            let result = self.run_once(mix, spec);
            if !self.using_gpu() {
                return result;
            }
            match result {
                Ok(out) if output_is_sane(mix, &out) => return Ok(out),
                _ => self.step_down()?,
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
}
