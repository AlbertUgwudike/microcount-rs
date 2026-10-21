//! Contrast-limited adaptive histogram equalisation, following MATLAB `adapthisteq`
//! with `NumTiles = [8 8]`, `NBins = 256`, `Distribution = 'rayleigh'`, `Alpha = 0.4`.

use crate::core::image::Image;
use rayon::prelude::*;

const TILES: usize = 8;
const BINS: usize = 256;

fn clip_histogram(hist: &mut [u64], clip: u64) {
    let nbins = hist.len() as u64;
    let mut excess: u64 = hist.iter().map(|&h| h.saturating_sub(clip)).sum();
    let avg_incr = excess / nbins;
    let upper = clip.saturating_sub(avg_incr);
    for h in hist.iter_mut() {
        if *h > clip {
            *h = clip;
        } else if *h > upper {
            excess = excess.saturating_sub(clip - *h);
            *h = clip;
        } else {
            excess = excess.saturating_sub(avg_incr);
            *h += avg_incr;
        }
    }
    // Spread the remainder evenly over bins still below the limit.
    let mut start = 0usize;
    while excess > 0 {
        let step = ((nbins / excess).max(1)) as usize;
        let before = excess;
        let mut m = start;
        while m < hist.len() && excess > 0 {
            if hist[m] < clip {
                hist[m] += 1;
                excess -= 1;
            }
            m += step;
        }
        start += 1;
        if start >= hist.len() {
            start = 0;
            if before == excess {
                break; // every bin is at the limit
            }
        }
    }
}

fn make_mapping(hist: &[u64], n_pix: u64) -> Vec<f64> {
    let alpha: f64 = 0.4;
    let hconst = 2.0 * alpha * alpha;
    let vmax = 1.0 - (-1.0 / hconst).exp();
    let mut cum = 0u64;
    hist.iter()
        .map(|&h| {
            cum += h;
            let mut val = vmax * (cum as f64 / n_pix as f64);
            if val >= 1.0 {
                val = 1.0 - f64::EPSILON;
            }
            (-hconst * (1.0 - val).ln()).sqrt().min(1.0)
        })
        .collect()
}

/// `adapthisteq(img, 'ClipLimit', clip_limit, 'Distribution', 'rayleigh')`. `full` is the
/// full range of the source data type (255 for 8-bit images, 65535 for 16-bit), which
/// MATLAB uses to bin intensities and scale the output.
pub fn adapthisteq(img: &Image<u16>, clip_limit: f64, full: u16) -> Image<u16> {
    let (w, h) = (img.w, img.h);
    if w < TILES || h < TILES {
        return img.clone();
    }
    let tw = w.div_ceil(TILES);
    let th = h.div_ceil(TILES);
    let n_pix = (tw * th) as u64;
    let min_clip = n_pix.div_ceil(BINS as u64);
    let clip = min_clip + (clip_limit * (n_pix - min_clip) as f64).round() as u64;
    let fullf = full.max(1) as f64;
    let bin_of = |v: u16| ((v.min(full) as f64) * (BINS - 1) as f64 / fullf).round() as usize;

    // Tiles cover a symmetric padding of the image up to TILES × tile size.
    let pad_x = (tw * TILES - w) / 2;
    let pad_y = (th * TILES - h) / 2;
    let sym = |i: isize, n: usize| -> usize {
        let n = n as isize;
        let mut i = i;
        loop {
            if i < 0 {
                i = -i - 1;
            } else if i >= n {
                i = 2 * n - i - 1;
            } else {
                return i as usize;
            }
        }
    };
    let maps: Vec<Vec<f64>> = (0..TILES * TILES)
        .into_par_iter()
        .map(|t| {
            let (tx, ty) = (t % TILES, t / TILES);
            let mut hist = vec![0u64; BINS];
            for yy in 0..th {
                let y = sym((ty * th + yy) as isize - pad_y as isize, h);
                let row = img.row(y);
                for xx in 0..tw {
                    let x = sym((tx * tw + xx) as isize - pad_x as isize, w);
                    hist[bin_of(row[x])] += 1;
                }
            }
            clip_histogram(&mut hist, clip);
            make_mapping(&hist, n_pix)
        })
        .collect();

    let mut out = Image::filled(w, h, 0u16);
    out.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        // Position relative to tile centres.
        let fy = ((y + pad_y) as f64 + 0.5) / th as f64 - 0.5;
        let ty0 = fy.floor().clamp(0.0, (TILES - 1) as f64) as usize;
        let ty1 = (ty0 + 1).min(TILES - 1);
        let wy = (fy - ty0 as f64).clamp(0.0, 1.0);
        for x in 0..w {
            let fx = ((x + pad_x) as f64 + 0.5) / tw as f64 - 0.5;
            let tx0 = fx.floor().clamp(0.0, (TILES - 1) as f64) as usize;
            let tx1 = (tx0 + 1).min(TILES - 1);
            let wx = (fx - tx0 as f64).clamp(0.0, 1.0);
            let b = bin_of(img.get(x, y));
            let m = |tx: usize, ty: usize| maps[ty * TILES + tx][b];
            let v = (1.0 - wy) * ((1.0 - wx) * m(tx0, ty0) + wx * m(tx1, ty0))
                + wy * ((1.0 - wx) * m(tx0, ty1) + wx * m(tx1, ty1));
            row[x] = (v * fullf).round().clamp(0.0, fullf) as u16;
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clahe_preserves_order_locally() {
        let img = Image::from_vec(64, 64, (0..64 * 64).map(|i| ((i % 64) * 1000) as u16).collect());
        let out = adapthisteq(&img, 0.02, 65535);
        assert!(out.get(40, 32) >= out.get(10, 32));
    }
}
