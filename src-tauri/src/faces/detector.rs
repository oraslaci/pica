use image::DynamicImage;

use super::traits::{DetectedFace, FaceDetector};

/// High-accuracy OpenCV YuNet deep neural face detector (ONNX, ~160ms CPU, 98.9% recall)
pub struct YuNetFaceDetector {
    detector: yunet_face::YuNetDetector,
}

impl Default for YuNetFaceDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl YuNetFaceDetector {
    pub fn new() -> Self {
        Self {
            detector: yunet_face::YuNetDetector::new(),
        }
    }
}

impl FaceDetector for YuNetFaceDetector {
    fn id(&self) -> &'static str {
        "yunet"
    }

    fn name(&self) -> &'static str {
        "OpenCV YuNet Deep Neural Detector (ONNX, ~160ms CPU, 98.9% Recall)"
    }

    fn is_ready(&self) -> bool {
        self.detector.is_ready()
    }

    fn detect_faces(&self, image: &DynamicImage) -> Result<Vec<DetectedFace>, String> {
        let faces = self.detector.detect(image);
        let mapped = faces
            .into_iter()
            .map(|f| {
                let mouth_cx = (f.landmarks.right_mouth.0 + f.landmarks.left_mouth.0) * 0.5;
                let mouth_cy = (f.landmarks.right_mouth.1 + f.landmarks.left_mouth.1) * 0.5;
                let lms = [
                    f.landmarks.right_eye,
                    f.landmarks.left_eye,
                    f.landmarks.nose_tip,
                    (mouth_cx, mouth_cy),
                    f.landmarks.right_mouth,
                ];
                DetectedFace {
                    box_x: f.box_x,
                    box_y: f.box_y,
                    box_w: f.box_w,
                    box_h: f.box_h,
                    confidence: f.confidence,
                    landmarks: Some(lms),
                }
            })
            .collect();
        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detector_initializes_and_runs_on_blank_image() {
        let img = DynamicImage::new_rgb8(320, 240);

        let yunet = YuNetFaceDetector::new();
        assert!(yunet.is_ready());
        let yunet_faces = yunet.detect_faces(&img).expect("yunet detection on blank image");
        assert!(yunet_faces.is_empty());
    }
}
