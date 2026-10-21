//! Intensity statistics and normalisation (`log_norm`, `mat2gray`, `stretchlim`,
//! `imadjust`, `nan_background`).

use crate::core::image::{Image, Mask};
use rayon::prelude::*;

/// `(mean, sample standard deviation)` of the non-NaN values produced by `f`.
fn mean_std(n: usize, f: impl Fn(usize) -> f64 + Sync) -> (f64, f64) {
    let (cnt, sum) = (0..n)
        .into_par_iter()
        .map(|i| f(i))
        .filter(|v| !v.is_nan())
        .fold(|| (0u64, 0.0f64), |(c, s), v| (c + 1, s + v))
        .reduce(|| (0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
    if cnt == 0 {
        return (f64::NAN, f64::NAN);
    }
    let mean = sum / cnt as f64;
    let ss = (0..n)
        .into_par_iter()
        .map(|i| f(i))
        .filter(|v| !v.is_nan())
        .map(|v| (v - mean) * (v - mean))
        .sum::<f64>();
    let std = if cnt > 1 { (ss / (cnt - 1) as f64).sqrt() } else { 0.0 };
    (mean, std)
}

/// Normalisation parameters `(C, S)` as returned by MATLAB `normalize`.
pub type NormParams = (f64, f64);

/// `log_norm.m`: z-scores `log(x + 1e-10)` (ignoring NaN), or applies the supplied
/// `(C, S)`. NaN results become 0.
pub fn log_norm(x: &Image<f32>, params: Option<NormParams>) -> (Image<f32>, NormParams) {
    let lg = |v: f32| (v as f64 + 1e-10).ln();
    let (c, s) = params.unwrap_or_else(|| mean_std(x.len(), |i| lg(x.data[i])));
    let out = x.map(|v| {
        let z = (lg(v) - c) / s;
        if z.is_nan() {
            0.0
        } else {
            z as f32
        }
    });
    (out, (c, s))
}

/// Just the `(C, S)` that [`log_norm`] would compute.
pub fn log_norm_params(x: &Image<f32>) -> NormParams {
    mean_std(x.len(), |i| (x.data[i] as f64 + 1e-10).ln())
}

/// `nan_background.m` combined with the cast to double.
pub fn nan_background<T: Copy + Send + Sync>(
    img: &Image<T>,
    mask: &Mask,
    f: impl Fn(T) -> f32 + Sync,
) -> Image<f32> {
    img.zip_map(mask, |v, m| if m { f(v) } else { f32::NAN })
}

pub fn min_max(x: &Image<f32>) -> (f64, f64) {
    x.data
        .par_iter()
        .filter(|v| !v.is_nan())
        .fold(
            || (f64::INFINITY, f64::NEG_INFINITY),
            |(lo, hi), &v| (lo.min(v as f64), hi.max(v as f64)),
        )
        .reduce(|| (f64::INFINITY, f64::NEG_INFINITY), |a, b| (a.0.min(b.0), a.1.max(b.1)))
}

/// `mat2gray(x, limits)`; with `None` the limits are the data min/max.
pub fn mat2gray(x: &Image<f32>, limits: Option<(f64, f64)>) -> Image<f32> {
    let (lo, hi) = limits.unwrap_or_else(|| min_max(x));
    if !(hi > lo) {
        return x.map(|v| v.clamp(0.0, 1.0));
    }
    let scale = 1.0 / (hi - lo);
    x.map(|v| (((v as f64 - lo) * scale).clamp(0.0, 1.0)) as f32)
}

fn lowhigh_from_hist(hist: &[u64]) -> (f64, f64) {
    let total: u64 = hist.iter().sum();
    let nbins = hist.len();
    if total == 0 {
        return (0.0, 1.0);
    }
    let mut cum = 0u64;
    let mut ilow = None;
    let mut ihigh = None;
    for (i, &h) in hist.iter().enumerate() {
        cum += h;
        let cdf = cum as f64 / total as f64;
        if ilow.is_none() && cdf > 0.01 {
            ilow = Some(i);
        }
        if ihigh.is_none() && cdf >= 0.99 {
            ihigh = Some(i);
        }
    }
    let (lo, hi) = (ilow.unwrap_or(0), ihigh.unwrap_or(nbins - 1));
    if lo == hi {
        return (0.0, 1.0);
    }
    (lo as f64 / (nbins - 1) as f64, hi as f64 / (nbins - 1) as f64)
}

/// `stretchlim` for 16-bit data (65536 bins), over any number of channels jointly.
pub fn stretchlim_u16(chans: &[&Image<u16>]) -> (f64, f64) {
    let hist = chans
        .iter()
        .map(|img| {
            img.data
                .par_chunks(1 << 16)
                .fold(
                    || vec![0u64; 65536],
                    |mut h, c| {
                        c.iter().for_each(|&v| h[v as usize] += 1);
                        h
                    },
                )
                .reduce(
                    || vec![0u64; 65536],
                    |mut a, b| {
                        a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
                        a
                    },
                )
        })
        .fold(vec![0u64; 65536], |mut a, b| {
            a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
            a
        });
    lowhigh_from_hist(&hist)
}

/// `stretchlim` for floating point data: like every non-uint8 class MATLAB uses 65536
/// bins over [0, 1].
pub fn stretchlim_f32(x: &Image<f32>) -> (f64, f64) {
    let hist = x
        .data
        .par_chunks(1 << 16)
        .fold(
            || vec![0u64; 65536],
            |mut h, c| {
                for &v in c {
                    if !v.is_nan() {
                        h[(v.clamp(0.0, 1.0) as f64 * 65535.0).round() as usize] += 1;
                    }
                }
                h
            },
        )
        .reduce(
            || vec![0u64; 65536],
            |mut a, b| {
                a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
                a
            },
        );
    lowhigh_from_hist(&hist)
}

/// Lookup table implementing `imadjust(u16, [lo hi], [])`.
pub fn imadjust_lut(lohi: (f64, f64)) -> Vec<u16> {
    let (lo, hi) = lohi;
    (0..65536u32)
        .map(|v| {
            let x = v as f64 / 65535.0;
            let y = if hi > lo { ((x - lo) / (hi - lo)).clamp(0.0, 1.0) } else { x };
            (y * 65535.0).round() as u16
        })
        .collect()
}

pub fn imadjust_u16(img: &Image<u16>) -> Image<u16> {
    let lut = imadjust_lut(stretchlim_u16(&[img]));
    img.map(|v| lut[v as usize])
}

/// `imadjust(x, [lo hi])` for floating point images.
pub fn imadjust_f32(x: &Image<f32>, lohi: (f64, f64)) -> Image<f32> {
    let (lo, hi) = lohi;
    if !(hi > lo) {
        return x.clone();
    }
    x.map(|v| (((v as f64 - lo) / (hi - lo)).clamp(0.0, 1.0)) as f32)
}

/// `perc.m`: `round(100 * a / b, 2)`.
pub fn perc(a: f64, b: f64) -> f64 {
    round2(100.0 * a / b)
}

pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub fn mean_omit_nan(v: &[f64]) -> f64 {
    let (n, s) = v.iter().filter(|x| !x.is_nan()).fold((0usize, 0.0), |(n, s), &x| (n + 1, s + x));
    if n == 0 {
        f64::NAN
    } else {
        s / n as f64
    }
}

pub fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        f64::NAN
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_norm_zscores() {
        let x = Image::from_vec(4, 1, vec![1.0, (1.0f32).exp(), f32::NAN, (2.0f32).exp()]);
        let (z, (c, s)) = log_norm(&x, None);
        assert!((c - 1.0).abs() < 1e-6);
        assert!((s - 1.0).abs() < 1e-6);
        assert_eq!(z.data[2], 0.0);
        assert!((z.data[0] + 1.0).abs() < 1e-5);
    }

    #[test]
    fn stretchlim_matches_matlab_semantics() {
        // 100 values 0..99: 1% -> first bin with cdf > 0.01 is value 1, 99% -> value 98.
        let img = Image::from_vec(100, 1, (0..100u16).collect());
        let (lo, hi) = stretchlim_u16(&[&img]);
        assert!((lo * 65535.0 - 1.0).abs() < 1e-6);
        assert!((hi * 65535.0 - 98.0).abs() < 1e-6);
        assert_eq!(perc(1.0, 3.0), 33.33);
    }
}
