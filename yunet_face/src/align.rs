use image::{imageops::FilterType, DynamicImage, GenericImageView, Rgba, RgbaImage};

/// 5-point facial landmarks normalized to [0.0, 1.0] image dimensions
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceLandmarks5 {
    pub right_eye: (f32, f32),
    pub left_eye: (f32, f32),
    pub nose_tip: (f32, f32),
    pub right_mouth: (f32, f32),
    pub left_mouth: (f32, f32),
}

impl FaceLandmarks5 {
    /// Calculate the eye roll angle in radians (for horizontal alignment)
    pub fn eye_angle(&self) -> f32 {
        let dx = self.left_eye.0 - self.right_eye.0;
        let dy = self.left_eye.1 - self.right_eye.1;
        dy.atan2(dx)
    }

    /// Calculate inter-ocular distance in normalized coordinates
    pub fn inter_ocular_dist(&self) -> f32 {
        let dx = self.left_eye.0 - self.right_eye.0;
        let dy = self.left_eye.1 - self.right_eye.1;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Rotates an image around its center by angle in radians with bilinear sampling
fn rotate_about_center(img: &DynamicImage, angle: f32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let rgba = img.to_rgba8();
    let mut out = RgbaImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;

            let src_x = cos_a * dx - sin_a * dy + cx;
            let src_y = sin_a * dx + cos_a * dy + cy;

            if src_x >= 0.0 && src_x < (w - 1) as f32 && src_y >= 0.0 && src_y < (h - 1) as f32 {
                let x_floor = src_x.floor() as u32;
                let y_floor = src_y.floor() as u32;
                let fx = src_x - x_floor as f32;
                let fy = src_y - y_floor as f32;

                let p00 = rgba.get_pixel(x_floor, y_floor);
                let p10 = rgba.get_pixel(x_floor + 1, y_floor);
                let p01 = rgba.get_pixel(x_floor, y_floor + 1);
                let p11 = rgba.get_pixel(x_floor + 1, y_floor + 1);

                let mut out_p = [0u8; 4];
                for c in 0..4 {
                    let v0 = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
                    let v1 = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
                    out_p[c] = (v0 * (1.0 - fy) + v1 * fy).round().clamp(0.0, 255.0) as u8;
                }
                out.put_pixel(x, y, Rgba(out_p));
            }
        }
    }

    DynamicImage::ImageRgba8(out)
}

/// Unified face alignment across all detectors.
///
/// Parameters:
/// - `img`: Original high-res or source image.
/// - `box_x`, `box_y`, `box_w`, `box_h`: Normalized bounding box in [0.0, 1.0].
/// - `landmarks`: Optional 5 keypoints (if available from YuNet or BlazeFace).
/// - `padding_ratio`: Context expansion (default 0.15 = 15% margin on each side).
/// - `target_size`: Output dimension for face chip (e.g. 112 for ArcFace/SFace or 64 for Gabor).
pub fn align_face(
    img: &DynamicImage,
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    landmarks: Option<&FaceLandmarks5>,
    padding_ratio: f32,
    target_size: u32,
) -> DynamicImage {
    let (iw, ih) = img.dimensions();

    // Expand bounding box with context margin
    let pad_w = box_w * padding_ratio;
    let pad_h = box_h * padding_ratio;

    let x0 = ((box_x - pad_w).max(0.0) * iw as f32).floor() as u32;
    let y0 = ((box_y - pad_h).max(0.0) * ih as f32).floor() as u32;
    let x1 = (((box_x + box_w + pad_w).min(1.0)) * iw as f32).ceil() as u32;
    let y1 = (((box_y + box_h + pad_h).min(1.0)) * ih as f32).ceil() as u32;

    let cw = (x1 - x0).max(1);
    let ch = (y1 - y0).max(1);

    let cropped = img.crop_imm(x0, y0, cw, ch);

    if let Some(lm) = landmarks {
        let angle = lm.eye_angle();
        // If roll angle > 2.5 degrees (~0.0436 radians), rotate chip upright
        if angle.abs() > 0.0436 {
            let rotated = rotate_about_center(&cropped, -angle);
            return rotated.resize_exact(target_size, target_size, FilterType::Triangle);
        }
    }

    cropped.resize_exact(target_size, target_size, FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eye_angle_horizontal() {
        let lm = FaceLandmarks5 {
            right_eye: (0.3, 0.4),
            left_eye: (0.7, 0.4),
            nose_tip: (0.5, 0.55),
            right_mouth: (0.35, 0.7),
            left_mouth: (0.65, 0.7),
        };
        assert!(lm.eye_angle().abs() < 1e-4);
    }

    #[test]
    fn test_eye_angle_tilted() {
        let lm = FaceLandmarks5 {
            right_eye: (0.3, 0.4),
            left_eye: (0.7, 0.5),
            nose_tip: (0.5, 0.6),
            right_mouth: (0.35, 0.75),
            left_mouth: (0.65, 0.8),
        };
        assert!(lm.eye_angle() > 0.1);
    }
}
