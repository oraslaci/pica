use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use image::{imageops::FilterType, DynamicImage};

use super::traits::FaceEmbedder;

/// High-accuracy neural face embedder using InsightFace ArcFace MobileFaceNet ONNX model (Glint360K, 512-D).
pub struct HighAccuracyFaceEmbedder {
    model_path: PathBuf,
    is_available: Arc<AtomicBool>,
    session: Arc<std::sync::Mutex<Option<ort::session::Session>>>,
}

impl HighAccuracyFaceEmbedder {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            is_available: Arc::new(AtomicBool::new(true)),
            session: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn set_available(&self, ready: bool) {
        self.is_available.store(ready, Ordering::SeqCst);
    }

    pub(crate) fn create_session(model_path: &std::path::Path) -> Result<ort::session::Session, String> {
        if model_path.exists() {
            if let Ok(sess) = ort::session::Session::builder()
                .and_then(|b| Ok(b.with_intra_threads(2)?))
                .and_then(|mut b| b.commit_from_file(model_path))
            {
                return Ok(sess);
            }
        }

        let candidates = [
            PathBuf::from("models/arcface_mbf.onnx"),
            PathBuf::from("../models/arcface_mbf.onnx"),
            PathBuf::from("src-tauri/models/arcface_mbf.onnx"),
            PathBuf::from("models/w600k_mbf.onnx"),
            PathBuf::from("../models/w600k_mbf.onnx"),
            PathBuf::from("src-tauri/models/w600k_mbf.onnx"),
        ];
        for cand in &candidates {
            if cand.exists() {
                if let Ok(sess) = ort::session::Session::builder()
                    .and_then(|b| Ok(b.with_intra_threads(2)?))
                    .and_then(|mut b| b.commit_from_file(cand))
                {
                    return Ok(sess);
                }
            }
        }

        const EMBEDDED_ARCFACE: &[u8] = include_bytes!("../../models/arcface_mbf.onnx");
        ort::session::Session::builder()
            .and_then(|b| Ok(b.with_intra_threads(2)?))
            .and_then(|mut b| b.commit_from_memory(EMBEDDED_ARCFACE))
            .map_err(|e| format!("Failed to create ArcFace ONNX session: {}", e))
    }
}

impl FaceEmbedder for HighAccuracyFaceEmbedder {
    fn id(&self) -> &'static str {
        "arcface_mbf_onnx"
    }

    fn name(&self) -> &'static str {
        "InsightFace ArcFace MobileFaceNet (Glint360K, 512-D)"
    }

    fn is_ready(&self) -> bool {
        self.is_available.load(Ordering::Relaxed)
    }

    fn compute_embedding(&self, face_chip: &DynamicImage) -> Result<Vec<f32>, String> {
        let mut session_guard = self.session.lock().map_err(|e| e.to_string())?;
        if session_guard.is_none() {
            match Self::create_session(&self.model_path) {
                Ok(s) => *session_guard = Some(s),
                Err(err) => return Err(err),
            }
        }
        let session = session_guard.as_mut().unwrap();

        // 1. Resize face chip to 112x112 for ArcFace MobileFaceNet input
        let chip = face_chip.resize_exact(112, 112, FilterType::Triangle);
        let rgb = chip.to_rgb8();

        // 2. Format as planar NCHW float tensor [1, 3, 112, 112]
        // Standard InsightFace ArcFace preprocessing: (pixel - 127.5) / 127.5
        let mut input_data = Vec::with_capacity(3 * 112 * 112);
        for p in rgb.pixels() {
            input_data.push((p[0] as f32 - 127.5) / 127.5);
        }
        for p in rgb.pixels() {
            input_data.push((p[1] as f32 - 127.5) / 127.5);
        }
        for p in rgb.pixels() {
            input_data.push((p[2] as f32 - 127.5) / 127.5);
        }

        let input_tensor = ort::value::Tensor::from_array((
            [1usize, 3, 112, 112],
            input_data.into_boxed_slice(),
        ))
        .map_err(|e| format!("Failed to create input tensor: {}", e))?;

        let outputs = session
            .run(ort::inputs![input_tensor])
            .map_err(|e| format!("Failed to run ArcFace inference: {}", e))?;

        let (_shape, output_slice) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract output tensor: {}", e))?;

        let mut embedding = output_slice.to_vec();

        // 3. L2 normalize embedding vector
        let norm_sq: f32 = embedding.iter().map(|v| v * v).sum();
        let norm = norm_sq.sqrt().max(1e-6);
        for v in &mut embedding {
            *v /= norm;
        }

        Ok(embedding)
    }
}

pub type CompositeFaceEmbedder = HighAccuracyFaceEmbedder;

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_high_accuracy_embedder() {
        let embedder = HighAccuracyFaceEmbedder::new(PathBuf::from("models/arcface_mbf.onnx"));
        assert!(embedder.is_ready());

        // Create synthetic dummy face image
        let img = DynamicImage::new_rgb8(100, 100);
        let emb = embedder.compute_embedding(&img).expect("Should compute embedding");
        assert_eq!(emb.len(), 512);

        // Vector must be L2 normalized
        let norm_sq: f32 = emb.iter().map(|v| v * v).sum();
        assert!((norm_sq - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_align_face_chip_dimensions() {
        let img = DynamicImage::new_rgb8(400, 400);
        let landmarks: [(f32, f32); 5] = [
            (0.35, 0.40),
            (0.65, 0.40),
            (0.50, 0.55),
            (0.50, 0.72),
            (0.20, 0.45),
        ];
        let aligned = align_face_chip(&img, &landmarks);
        assert_eq!(aligned.width(), 112);
        assert_eq!(aligned.height(), 112);
    }
}

/// Aligns a face image into standard ArcFace 112x112 canonical coordinates using 2D similarity transform.
pub fn align_face_chip(img: &DynamicImage, landmarks: &[(f32, f32); 5]) -> DynamicImage {
    let img_w = img.width() as f32;
    let img_h = img.height() as f32;

    // Source points from BlazeFace in image pixel coordinates:
    // 0: Right Eye (person's right, camera left)
    // 1: Left Eye (person's left, camera right)
    // 2: Nose tip
    // 3: Mouth center
    let src: [(f32, f32); 4] = [
        (landmarks[0].0 * img_w, landmarks[0].1 * img_h),
        (landmarks[1].0 * img_w, landmarks[1].1 * img_h),
        (landmarks[2].0 * img_w, landmarks[2].1 * img_h),
        (landmarks[3].0 * img_w, landmarks[3].1 * img_h),
    ];

    // Canonical ArcFace destination points in 112x112 space:
    // Left Eye: (38.2946, 51.6963)
    // Right Eye: (73.5318, 51.5014)
    // Nose: (56.0252, 71.7366)
    // Mouth center: ((41.5493 + 70.7299)/2, (92.3655 + 92.2041)/2) = (56.1396, 92.2848)
    let dst: [(f32, f32); 4] = [
        (38.2946, 51.6963),
        (73.5318, 51.5014),
        (56.0252, 71.7366),
        (56.1396, 92.2848),
    ];

    // Compute least-squares 2D similarity transform (u = a*x - b*y + c, v = b*x + a*y + d)
    let n = 4.0f32;
    let mean_x = (src[0].0 + src[1].0 + src[2].0 + src[3].0) / n;
    let mean_y = (src[0].1 + src[1].1 + src[2].1 + src[3].1) / n;
    let mean_u = (dst[0].0 + dst[1].0 + dst[2].0 + dst[3].0) / n;
    let mean_v = (dst[0].1 + dst[1].1 + dst[2].1 + dst[3].1) / n;

    let mut var_x = 0.0f32;
    let mut num_a = 0.0f32;
    let mut num_b = 0.0f32;

    for i in 0..4 {
        let dx = src[i].0 - mean_x;
        let dy = src[i].1 - mean_y;
        let du = dst[i].0 - mean_u;
        let dv = dst[i].1 - mean_v;

        var_x += dx * dx + dy * dy;
        num_a += dx * du + dy * dv;
        num_b += dx * dv - dy * du;
    }

    if var_x < 1e-4 {
        return img.resize_exact(112, 112, FilterType::Triangle);
    }

    let a = num_a / var_x;
    let b = num_b / var_x;
    let c = mean_u - (a * mean_x - b * mean_y);
    let d = mean_v - (b * mean_x + a * mean_y);

    let scale_sq = a * a + b * b;
    if scale_sq < 1e-6 {
        return img.resize_exact(112, 112, FilterType::Triangle);
    }

    // Inverse mapping: (u, v) in 112x112 -> (x, y) in source image
    // x = (a * (u - c) + b * (v - d)) / scale_sq
    // y = (-b * (u - c) + a * (v - d)) / scale_sq
    let mut out_buf = image::RgbImage::new(112, 112);
    let src_rgb = img.to_rgb8();
    let max_x = (img.width().saturating_sub(1)) as f32;
    let max_y = (img.height().saturating_sub(1)) as f32;

    for v in 0..112 {
        let vf = v as f32;
        let v_diff = vf - d;
        for u in 0..112 {
            let uf = u as f32;
            let u_diff = uf - c;

            let x = (a * u_diff + b * v_diff) / scale_sq;
            let y = (-b * u_diff + a * v_diff) / scale_sq;

            if x >= 0.0 && x <= max_x && y >= 0.0 && y <= max_y {
                let x0 = x.floor() as u32;
                let y0 = y.floor() as u32;
                let x1 = (x0 + 1).min(img.width() - 1);
                let y1 = (y0 + 1).min(img.height() - 1);

                let fx = x - x0 as f32;
                let fy = y - y0 as f32;

                let p00 = src_rgb.get_pixel(x0, y0);
                let p10 = src_rgb.get_pixel(x1, y0);
                let p01 = src_rgb.get_pixel(x0, y1);
                let p11 = src_rgb.get_pixel(x1, y1);

                let r = (1.0 - fx) * (1.0 - fy) * p00[0] as f32
                    + fx * (1.0 - fy) * p10[0] as f32
                    + (1.0 - fx) * fy * p01[0] as f32
                    + fx * fy * p11[0] as f32;
                let g = (1.0 - fx) * (1.0 - fy) * p00[1] as f32
                    + fx * (1.0 - fy) * p10[1] as f32
                    + (1.0 - fx) * fy * p01[1] as f32
                    + fx * fy * p11[1] as f32;
                let bl = (1.0 - fx) * (1.0 - fy) * p00[2] as f32
                    + fx * (1.0 - fy) * p10[2] as f32
                    + (1.0 - fx) * fy * p01[2] as f32
                    + fx * fy * p11[2] as f32;

                out_buf.put_pixel(
                    u,
                    v,
                    image::Rgb([
                        r.clamp(0.0, 255.0) as u8,
                        g.clamp(0.0, 255.0) as u8,
                        bl.clamp(0.0, 255.0) as u8,
                    ]),
                );
            }
        }
    }

    DynamicImage::ImageRgb8(out_buf)
}
