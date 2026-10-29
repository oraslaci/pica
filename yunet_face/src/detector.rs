use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ort::session::Session;
use ort::value::Tensor;

use crate::align::FaceLandmarks5;

const EMBEDDED_YUNET: &[u8] = include_bytes!("../models/yunet.onnx");

#[derive(Debug, Clone)]
pub struct YuNetFace {
    pub box_x: f32, // Normalized 0.0 to 1.0
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub confidence: f32,
    pub landmarks: FaceLandmarks5,
}

pub struct YuNetDetector {
    session: Arc<Mutex<Option<Session>>>,
    input_width: u32,
    input_height: u32,
    score_threshold: f32,
    nms_threshold: f32,
}

impl YuNetDetector {
    pub fn new() -> Self {
        Self::with_options(640, 640, 0.50, 0.30)
    }

    pub fn with_options(
        input_width: u32,
        input_height: u32,
        score_threshold: f32,
        nms_threshold: f32,
    ) -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
            input_width,
            input_height,
            score_threshold,
            nms_threshold,
        }
    }

    fn create_session() -> Result<Session, String> {
        let candidates = [
            PathBuf::from("models/yunet.onnx"),
            PathBuf::from("../models/yunet.onnx"),
            PathBuf::from("yunet_face/models/yunet.onnx"),
        ];

        for path in &candidates {
            if path.exists() {
                if let Ok(sess) = Session::builder()
                    .and_then(|b| Ok(b.with_intra_threads(2)?))
                    .and_then(|mut b| b.commit_from_file(path))
                {
                    return Ok(sess);
                }
            }
        }

        // Fallback to embedded bytes
        Session::builder()
            .and_then(|b| Ok(b.with_intra_threads(2)?))
            .and_then(|mut b| b.commit_from_memory(EMBEDDED_YUNET))
            .map_err(|e| format!("Failed to create YuNet session: {:?}", e))
    }

    pub fn is_ready(&self) -> bool {
        true
    }

    /// Detect faces on an image.
    /// Returns decoded bounding boxes (normalized [0, 1]) and 5 facial landmarks.
    pub fn detect(&self, img: &DynamicImage) -> Vec<YuNetFace> {
        let mut sess_guard = match self.session.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        if sess_guard.is_none() {
            *sess_guard = Self::create_session().ok();
        }

        let session = match sess_guard.as_mut() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let (orig_w, orig_h) = img.dimensions();
        if orig_w == 0 || orig_h == 0 {
            return Vec::new();
        }

        // Target size: 640x640
        let tw = self.input_width;
        let th = self.input_height;

        let resized = img.resize_exact(tw, th, FilterType::Triangle);
        let rgb = resized.to_rgb8();

        // Convert to BGR planar tensor: [1, 3, th, tw] as float32 in [0.0, 255.0]
        let mut bgr_vec = Vec::with_capacity((3 * th * tw) as usize);

        // Plane 0: Blue
        for p in rgb.pixels() {
            bgr_vec.push(p[2] as f32);
        }
        // Plane 1: Green
        for p in rgb.pixels() {
            bgr_vec.push(p[1] as f32);
        }
        // Plane 2: Red
        for p in rgb.pixels() {
            bgr_vec.push(p[0] as f32);
        }

        let input_tensor = match Tensor::from_array(([1usize, 3usize, th as usize, tw as usize], bgr_vec.into_boxed_slice())) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let outputs = match session.run(ort::inputs![input_tensor]) {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        // Outputs in order:
        // 0..3: cls_8, cls_16, cls_32
        // 3..6: obj_8, obj_16, obj_32
        // 6..9: bbox_8, bbox_16, bbox_32
        // 9..12: kps_8, kps_16, kps_32
        let strides = [8usize, 16usize, 32usize];
        let mut candidates = Vec::new();

        for i in 0..3 {
            let s = strides[i];
            let cols = tw as usize / s;
            let rows = th as usize / s;
            let count = rows * cols;

            let (_cls_shape, cls_data) = match outputs[i].try_extract_tensor::<f32>() {
                Ok(r) => r,
                Err(_) => continue,
            };
            let (_obj_shape, obj_data) = match outputs[i + 3].try_extract_tensor::<f32>() {
                Ok(r) => r,
                Err(_) => continue,
            };
            let (_bbox_shape, bbox_data) = match outputs[i + 6].try_extract_tensor::<f32>() {
                Ok(r) => r,
                Err(_) => continue,
            };
            let (_kps_shape, kps_data) = match outputs[i + 9].try_extract_tensor::<f32>() {
                Ok(r) => r,
                Err(_) => continue,
            };

            for r in 0..rows {
                for c in 0..cols {
                    let idx = r * cols + c;
                    if idx >= count { break; }

                    let cls_val = cls_data[idx].clamp(0.0, 1.0);
                    let obj_val = obj_data[idx].clamp(0.0, 1.0);
                    let score = (cls_val * obj_val).sqrt();

                    if score < self.score_threshold {
                        continue;
                    }

                    let b_off = idx * 4;
                    let k_off = idx * 10;
                    if b_off + 3 >= bbox_data.len() || k_off + 9 >= kps_data.len() {
                        continue;
                    }

                    // Decode box:
                    let cx = (c as f32 + bbox_data[b_off + 0]) * s as f32;
                    let cy = (r as f32 + bbox_data[b_off + 1]) * s as f32;
                    let bw = bbox_data[b_off + 2].exp() * s as f32;
                    let bh = bbox_data[b_off + 3].exp() * s as f32;

                    let x1 = cx - bw / 2.0;
                    let y1 = cy - bh / 2.0;

                    // Normalize to [0.0, 1.0]
                    let norm_x = (x1 / tw as f32).clamp(0.0, 1.0);
                    let norm_y = (y1 / th as f32).clamp(0.0, 1.0);
                    let norm_w = (bw / tw as f32).clamp(0.0, 1.0 - norm_x);
                    let norm_h = (bh / th as f32).clamp(0.0, 1.0 - norm_y);

                    // Decode landmarks (5 points):
                    // In YuNet output: right_eye, left_eye, nose_tip, right_mouth, left_mouth
                    let lm = FaceLandmarks5 {
                        right_eye: (
                            ((kps_data[k_off + 0] + c as f32) * s as f32 / tw as f32).clamp(0.0, 1.0),
                            ((kps_data[k_off + 1] + r as f32) * s as f32 / th as f32).clamp(0.0, 1.0),
                        ),
                        left_eye: (
                            ((kps_data[k_off + 2] + c as f32) * s as f32 / tw as f32).clamp(0.0, 1.0),
                            ((kps_data[k_off + 3] + r as f32) * s as f32 / th as f32).clamp(0.0, 1.0),
                        ),
                        nose_tip: (
                            ((kps_data[k_off + 4] + c as f32) * s as f32 / tw as f32).clamp(0.0, 1.0),
                            ((kps_data[k_off + 5] + r as f32) * s as f32 / th as f32).clamp(0.0, 1.0),
                        ),
                        right_mouth: (
                            ((kps_data[k_off + 6] + c as f32) * s as f32 / tw as f32).clamp(0.0, 1.0),
                            ((kps_data[k_off + 7] + r as f32) * s as f32 / th as f32).clamp(0.0, 1.0),
                        ),
                        left_mouth: (
                            ((kps_data[k_off + 8] + c as f32) * s as f32 / tw as f32).clamp(0.0, 1.0),
                            ((kps_data[k_off + 9] + r as f32) * s as f32 / th as f32).clamp(0.0, 1.0),
                        ),
                    };

                    candidates.push(YuNetFace {
                        box_x: norm_x,
                        box_y: norm_y,
                        box_w: norm_w,
                        box_h: norm_h,
                        confidence: score,
                        landmarks: lm,
                    });
                }
            }
        }

        apply_nms(candidates, self.nms_threshold)
    }
}

pub fn apply_nms(mut boxes: Vec<YuNetFace>, iou_threshold: f32) -> Vec<YuNetFace> {
    boxes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    let mut picked: Vec<YuNetFace> = Vec::new();

    for current in boxes {
        let mut keep = true;
        for p in &picked {
            let x1 = current.box_x.max(p.box_x);
            let y1 = current.box_y.max(p.box_y);
            let x2 = (current.box_x + current.box_w).min(p.box_x + p.box_w);
            let y2 = (current.box_y + current.box_h).min(p.box_y + p.box_h);

            let inter_w = (x2 - x1).max(0.0);
            let inter_h = (y2 - y1).max(0.0);
            let inter_area = inter_w * inter_h;

            let area1 = current.box_w * current.box_h;
            let area2 = p.box_w * p.box_h;
            let union_area = area1 + area2 - inter_area;

            let iou = if union_area > 0.0 { inter_area / union_area } else { 0.0 };
            if iou > iou_threshold {
                keep = false;
                break;
            }
        }
        if keep {
            picked.push(current);
        }
    }

    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_initialization() {
        let detector = YuNetDetector::new();
        assert!(detector.is_ready());
    }
}
