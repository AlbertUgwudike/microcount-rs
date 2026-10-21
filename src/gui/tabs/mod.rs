pub mod analyse;
pub mod home;
pub mod regions;
pub mod register;
pub mod select;

use crate::core::image::Image;
use crate::core::tiff::TiffFile;
use anyhow::Result;
use std::path::Path;

/// Reads one channel (1-based) of the downsampled image.
pub fn read_down(path: &Path, ch: usize) -> Result<Image<u16>> {
    let tf = TiffFile::open(path)?;
    let p = &tf.pages[0];
    tf.read_channel(ch.saturating_sub(1), crate::core::image::Rect::new(0, 0, p.width, p.height))
}

pub fn fmt(v: f64) -> String {
    if v.is_finite() {
        let a = v.abs();
        if a != 0.0 && (a >= 1e5 || a < 1e-3) {
            format!("{v:.3e}")
        } else {
            format!("{:.4}", v).trim_end_matches('0').trim_end_matches('.').to_string()
        }
    } else {
        "--".into()
    }
}
