use image::DynamicImage;

#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub box_x: f32, // Normalized 0.0 to 1.0 (left)
    pub box_y: f32, // Normalized 0.0 to 1.0 (top)
    pub box_w: f32, // Normalized 0.0 to 1.0 (width)
    pub box_h: f32, // Normalized 0.0 to 1.0 (height)
    pub confidence: f32,
    pub landmarks: Option<[(f32, f32); 5]>, // Optional 5 keypoints (left eye, right eye, nose, mouth left, mouth right)
}

/// Abstract interface for any face detection library (ONNX, OpenCV, pure Rust, Apple Vision, etc.)
pub trait FaceDetector: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_ready(&self) -> bool;
    fn detect_faces(&self, image: &DynamicImage) -> Result<Vec<DetectedFace>, String>;
}

/// Abstract interface for computing face feature vectors/embeddings
pub trait FaceEmbedder: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_ready(&self) -> bool;
    fn compute_embedding(&self, face_chip: &DynamicImage) -> Result<Vec<f32>, String>;
}

/// Abstract interface for clustering and grouping faces into Person entities
pub trait FaceClusterer: Send + Sync {
    fn find_matching_cluster(
        &self,
        embedding: &[f32],
        existing_clusters: &[(i64, Vec<f32>)], // (person_id, representative_or_centroid_embedding)
        threshold: f32,
    ) -> Option<i64>;
}
