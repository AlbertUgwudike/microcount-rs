//! Morphology with flat disk structuring elements (`strel('disk', r, 0)`).
//! Out-of-image pixels never contribute (equivalent to -Inf/+Inf padding).

use super::filters::{disk_halfwidths, BAND};
use crate::core::image::{Image, Mask};
use rayon::prelude::*;

fn morph_mask(mask: &Mask, r: usize, dilate: bool) -> Mask {
    if r == 0 {
        return mask.clone();
    }
    let hw = disk_halfwidths(r);
    let mut widths: Vec<usize> = hw.clone();
    widths.sort_unstable();
    widths.dedup();
    let (w, h) = (mask.w, mask.h);
    let mut out = Mask::filled(w, h, false);
    out.data.par_chunks_mut(w * BAND).enumerate().for_each(|(bi, band)| {
        let y0 = bi * BAND;
        let bh = band.len() / w;
        let n_rows = bh + 2 * r;
        let ys = y0 as isize - r as isize;
        // rowres[wi][k*w + x]: horizontal result for width widths[wi] on band row k.
        let mut rowres = vec![vec![false; n_rows * w]; widths.len()];
        let mut pre = vec![0u32; w + 1];
        for k in 0..n_rows {
            let yy = ys + k as isize;
            if yy < 0 || yy >= h as isize {
                continue;
            }
            let row = mask.row(yy as usize);
            for x in 0..w {
                pre[x + 1] = pre[x] + row[x] as u32;
            }
            for (wi, &hwj) in widths.iter().enumerate() {
                let dst = &mut rowres[wi][k * w..(k + 1) * w];
                for x in 0..w {
                    let lo = x.saturating_sub(hwj);
                    let hi = (x + hwj + 1).min(w);
                    let c = pre[hi] - pre[lo];
                    dst[x] = if dilate { c > 0 } else { c as usize == hi - lo };
                }
            }
        }
        for by in 0..bh {
            let o = &mut band[by * w..(by + 1) * w];
            o.fill(!dilate);
            for (j, &hwj) in hw.iter().enumerate() {
                let yy = (y0 + by + j) as isize - r as isize;
                if yy < 0 || yy >= h as isize {
                    continue;
                }
                let wi = widths.binary_search(&hwj).unwrap();
                let src = &rowres[wi][(by + j) * w..(by + j + 1) * w];
                if dilate {
                    o.iter_mut().zip(src).for_each(|(a, &b)| *a |= b);
                } else {
                    o.iter_mut().zip(src).for_each(|(a, &b)| *a &= b);
                }
            }
        }
    });
    out
}

fn morph_f32(img: &Image<f32>, r: usize, dilate: bool) -> Image<f32> {
    let hw = disk_halfwidths(r);
    let (w, h) = (img.w, img.h);
    let ident = if dilate { f32::NEG_INFINITY } else { f32::INFINITY };
    let op = |a: f32, b: f32| if dilate { a.max(b) } else { a.min(b) };
    let mut widths: Vec<usize> = hw.clone();
    widths.sort_unstable();
    widths.dedup();
    let mut out = Image::filled(w, h, 0.0f32);
    out.data.par_chunks_mut(w * BAND).enumerate().for_each(|(bi, band)| {
        let y0 = bi * BAND;
        let bh = band.len() / w;
        let n_rows = bh + 2 * r;
        let ys = y0 as isize - r as isize;
        let mut rowres = vec![vec![ident; n_rows * w]; widths.len()];
        for k in 0..n_rows {
            let yy = ys + k as isize;
            if yy < 0 || yy >= h as isize {
                continue;
            }
            let row = img.row(yy as usize);
            for (wi, &hwj) in widths.iter().enumerate() {
                let dst = &mut rowres[wi][k * w..(k + 1) * w];
                for x in 0..w {
                    let lo = x.saturating_sub(hwj);
                    let hi = (x + hwj + 1).min(w);
                    dst[x] = row[lo..hi].iter().fold(ident, |a, &b| op(a, b));
                }
            }
        }
        for by in 0..bh {
            let o = &mut band[by * w..(by + 1) * w];
            o.fill(ident);
            for (j, &hwj) in hw.iter().enumerate() {
                let yy = (y0 + by + j) as isize - r as isize;
                if yy < 0 || yy >= h as isize {
                    continue;
                }
                let wi = widths.binary_search(&hwj).unwrap();
                let src = &rowres[wi][(by + j) * w..(by + j + 1) * w];
                o.iter_mut().zip(src).for_each(|(a, &b)| *a = op(*a, b));
            }
        }
    });
    out
}

pub fn dilate(mask: &Mask, r: usize) -> Mask {
    morph_mask(mask, r, true)
}

pub fn erode(mask: &Mask, r: usize) -> Mask {
    morph_mask(mask, r, false)
}

/// `imopen(bw, strel('disk', r, 0))`
pub fn open(mask: &Mask, r: usize) -> Mask {
    dilate(&erode(mask, r), r)
}

/// `imclose(img, strel('disk', r, 0))` for grayscale images.
pub fn close_f32(img: &Image<f32>, r: usize) -> Image<f32> {
    morph_f32(&morph_f32(img, r, true), r, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_removes_small_specks() {
        let mut m = Mask::filled(30, 30, false);
        for y in 5..20 {
            for x in 5..20 {
                m.set(x, y, true);
            }
        }
        m.set(27, 27, true);
        let o = open(&m, 2);
        assert!(!o.get(27, 27));
        assert!(o.get(12, 12));
        // Opening with a disk rounds the square's corners.
        assert!(!o.get(5, 5));
    }

    #[test]
    fn closing_fills_gaps() {
        let mut img = Image::filled(9, 1, 1.0f32);
        img.set(4, 0, 0.0);
        let c = close_f32(&img, 2);
        assert_eq!(c.get(4, 0), 1.0);
    }
}
