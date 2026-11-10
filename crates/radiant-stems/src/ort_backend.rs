//! The ONNX Runtime wrapper: implements [`Backend`] for the htdemucs ONNX model (CPU).
//!
//! The runtime library is NOT linked: `ort` is used with `load-dynamic`, the library is loaded from
//! [`crate::runtime_library_path`] (set through `ORT_DYLIB_PATH` before the first session is made). Loading a missing
//! library would panic inside `ort`, so [`OrtBackend::load`] checks the file first.

use crate::demucs::{self, Backend, IoMap, SegmentOut, TensorInfo, CHANNELS, FRAMES, FREQ_BINS, MIX_LEN, SEGMENT_SAMPLES, SPEC_CHANNELS, SPEC_LEN, STEMS};
use crate::StemError;
use ort::session::Session;
use ort::value::{Tensor, ValueType};
use std::path::Path;

pub struct OrtBackend {
    session: Session,
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

impl OrtBackend {
    /// Loads the model. `threads` = intra-op threads (keep it below the core count so the music does not crackle).
    pub fn load(model: &Path, threads: usize) -> Result<Self, StemError> {
        let lib = crate::runtime_library_path().filter(|p| p.is_file()).ok_or(StemError::NoRuntime)?;
        std::env::set_var("ORT_DYLIB_PATH", &lib);
        let session = Session::builder()
            .map_err(infer)?
            .with_intra_threads(threads.max(1))
            .map_err(infer)?
            .commit_from_file(model)
            .map_err(infer)?;
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
        })
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
