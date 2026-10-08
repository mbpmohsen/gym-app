//! 5-point face alignment to the standard ArcFace 112x112 template.
//!
//! The similarity transform (rotation + uniform scale + translation) is the
//! least-squares fit from the detected landmarks to the template. In 2D this has
//! a closed form, identical to Umeyama's method when no reflection is involved,
//! which is what OpenCV's FaceRecognizerSF::alignCrop computes.

use image::{Rgb, RgbImage};

pub const ALIGNED_SIZE: u32 = 112;

/// ArcFace reference landmarks for a 112x112 crop.
pub const TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

/// 2x3 affine matrix [[a, -b, tx], [b, a, ty]] mapping src -> dst.
pub type Affine = [[f32; 3]; 2];

pub fn similarity(src: &[[f32; 2]; 5], dst: &[[f32; 2]; 5]) -> Affine {
    let n = src.len() as f32;
    let mean = |p: &[[f32; 2]; 5]| {
        let (sx, sy) = p.iter().fold((0.0, 0.0), |(x, y), q| (x + q[0], y + q[1]));
        [sx / n, sy / n]
    };
    let (ms, md) = (mean(src), mean(dst));

    let (mut num_a, mut num_b, mut den) = (0.0f32, 0.0f32, 0.0f32);
    for (s, d) in src.iter().zip(dst) {
        let (sx, sy) = (s[0] - ms[0], s[1] - ms[1]);
        let (dx, dy) = (d[0] - md[0], d[1] - md[1]);
        num_a += sx * dx + sy * dy;
        num_b += sx * dy - sy * dx;
        den += sx * sx + sy * sy;
    }
    let a = num_a / den;
    let b = num_b / den;
    let tx = md[0] - (a * ms[0] - b * ms[1]);
    let ty = md[1] - (b * ms[0] + a * ms[1]);
    [[a, -b, tx], [b, a, ty]]
}

/// Warps `img` with `m` (src -> dst) into a w x h image, bilinear, black border.
/// Pixel-coordinate convention matches cv::warpAffine.
pub fn warp_affine(img: &RgbImage, m: &Affine, w: u32, h: u32) -> RgbImage {
    // invert the similarity: src = A^-1 (dst - t)
    let (a, b) = (m[0][0], m[1][0]);
    let det = a * a + b * b;
    let (ia, ib) = (a / det, b / det); // A^-1 = [[ia, ib], [-ib, ia]]
    let (tx, ty) = (m[0][2], m[1][2]);

    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let px = |x: i64, y: i64, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= iw || y >= ih {
            0.0
        } else {
            img.get_pixel(x as u32, y as u32)[c] as f32
        }
    };

    let mut out = RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 - tx, y as f32 - ty);
            let sx = ia * dx + ib * dy;
            let sy = -ib * dx + ia * dy;
            let (x0, y0) = (sx.floor() as i64, sy.floor() as i64);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let mut rgb = [0u8; 3];
            for (c, v) in rgb.iter_mut().enumerate() {
                let top = px(x0, y0, c) * (1.0 - fx) + px(x0 + 1, y0, c) * fx;
                let bot = px(x0, y0 + 1, c) * (1.0 - fx) + px(x0 + 1, y0 + 1, c) * fx;
                *v = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(x, y, Rgb(rgb));
        }
    }
    out
}

/// Crops and aligns one face to 112x112.
pub fn align_face(img: &RgbImage, landmarks: &[[f32; 2]; 5]) -> RgbImage {
    let m = similarity(landmarks, &TEMPLATE);
    warp_affine(img, &m, ALIGNED_SIZE, ALIGNED_SIZE)
}
