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
use std::path::Path;

pub struct OrtBackend {
    session: Session,
    gpu: bool,
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
fn with_gpu(builder: SessionBuilder) -> Result<SessionBuilder, StemError> {
    use ort::execution_providers::DirectMLExecutionProvider;
    builder
        .with_memory_pattern(false)
        .map_err(infer)?
        .with_parallel_execution(false)
        .map_err(infer)?
        // `error_on_failure`: when the DLL has no DirectML or there is no card, we want an error (and the CPU
        // fallback below), not a silent CPU session that is reported as "GPU".
        .with_execution_providers([DirectMLExecutionProvider::default().build().error_on_failure()])
        .map_err(infer)
}

#[cfg(not(feature = "directml"))]
fn with_gpu(_: SessionBuilder) -> Result<SessionBuilder, StemError> {
    Err(StemError::Inference("this build has no DirectML support".into()))
}

fn build_session(model: &Path, threads: usize, gpu: bool) -> Result<Session, StemError> {
    let mut builder = Session::builder().map_err(infer)?;
    if gpu {
        builder = with_gpu(builder)?;
    }
    builder.with_intra_threads(threads.max(1)).map_err(infer)?.commit_from_file(model).map_err(infer)
}

impl OrtBackend {
    /// Loads the model. `threads` = intra-op threads (keep it below the core count so the music does not crackle;
    /// the CPU path only). `gpu` asks for DirectML; when it cannot be used (plain CPU `onnxruntime.dll`, no
    /// DirectX 12 card, driver problem) the model is loaded on the CPU instead and [`OrtBackend::using_gpu`] says so.
    pub fn load(model: &Path, threads: usize, gpu: bool) -> Result<Self, StemError> {
        let lib = crate::runtime_library_path().filter(|p| p.is_file()).ok_or(StemError::NoRuntime)?;
        std::env::set_var("ORT_DYLIB_PATH", &lib);
        let (session, gpu) = if gpu {
            match build_session(model, threads, true) {
                Ok(session) => (session, true),
                Err(_) => (build_session(model, threads, false)?, false),
            }
        } else {
            (build_session(model, threads, false)?, false)
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
            gpu,
        })
    }

    /// The session really runs on the GPU (false = CPU, also when the GPU was asked for but was not available).
    pub fn using_gpu(&self) -> bool {
        self.gpu
    }
}

impl Backend for OrtBackend {
    fn run(&mut self, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError> {
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
