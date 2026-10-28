use std::path::PathBuf;

/// Total size in megabytes of the built-in neural models (YuNet 0.23 MB + ArcFace MobileFaceNet 13.6 MB)
pub const TOTAL_MODEL_SIZE_MB: f64 = 13.8;

#[derive(Clone)]
pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new() -> Self {
        let models_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pica")
            .join("models")
            .join("faces");

        Self { models_dir }
    }

    pub fn get_embedder_path(&self) -> PathBuf {
        self.models_dir.join("embedder.onnx")
    }

    pub fn are_models_installed(&self) -> bool {
        // Models are embedded directly in the binary via include_bytes!
        true
    }

    pub fn is_downloading(&self) -> bool {
        false
    }

    pub fn get_download_progress(&self) -> f32 {
        1.0
    }
}
