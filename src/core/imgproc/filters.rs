//! Linear and rank filters. Everything works in row bands so peak memory stays close to
//! the size of the output, and bands run in parallel.

use crate::core::image::{Image, Mask};
use rayon::prelude::*;

pub const BAND: usize = 32;

/// MATLAB `gradient` of a vector.
fn gradient(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    (0..n)
        .map(|i| match i {
            0 => v[1] - v[0],
            i if i == n - 1 => v[n - 1] - v[n - 2],
            i => (v[i + 1] - v[i - 1]) / 2.0,
        })
        .collect()
}

/// 1-D factors of `fspecial('gaussian', l, sigma)` and its derivatives as taken by
/// `gradient`: the 2-D kernels of `pacefilt` are outer products of these.
fn gaussian_factors(l: usize, sigma: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let c = (l as f64 - 1.0) / 2.0;
    let a: Vec<f64> = (0..l).map(|i| (-(i as f64 - c).powi(2) / (2.0 * sigma * sigma)).exp()).collect();
    let s: f64 = a.iter().sum();
    let a: Vec<f64> = a.iter().map(|v| v / s).collect();
    let d = gradient(&a);
    let dd = gradient(&d);
    (a, d, dd)
}

/// `conv2(row, k, 'same')` for one row with zero padding.
fn conv_row(src: &[f32], k: &[f32], dst: &mut [f32]) {
    let r = k.len() / 2;
    let n = src.len();
    for x in 0..n {
        let mut acc = 0.0f32;
        // dst[x] = sum_t k[r + t] * src[x - t]
        let t_lo = (x + r + 1).saturating_sub(n) as isize - r as isize; // x - t <= n-1
        let t_lo = t_lo.max(-(r as isize));
        let t_hi = (x as isize).min(r as isize); // x - t >= 0
        let mut t = t_lo;
        while t <= t_hi {
            acc += k[(r as isize + t) as usize] * src[(x as isize - t) as usize];
            t += 1;
        }
        dst[x] = acc;
    }
}

/// `pacefilt.m`: magnitude of the dominant Hessian eigenvalue using separable
/// Gaussian-derivative kernels of size `l` and width `sigma`.
pub fn pacefilt(img: &Image<f32>, l: usize, sigma: f64) -> Image<f32> {
    let (a, d, dd) = gaussian_factors(l, sigma);
    let f = |v: &Vec<f64>| v.iter().map(|&x| x as f32).collect::<Vec<f32>>();
    let (a, d, dd) = (f(&a), f(&d), f(&dd));
    let r = l / 2;
    let (w, h) = (img.w, img.h);
    let mut out = Image::filled(w, h, 0.0f32);
    out.data.par_chunks_mut(w * BAND).enumerate().for_each(|(bi, band)| {
        let y0 = bi * BAND;
        let bh = band.len() / w;
        let ys = y0 as isize - r as isize;
        let n_rows = bh + 2 * r;
        // Horizontal passes: rxx needs dd(x), ryy needs a(x), rxy needs d(x).
        let mut h_a = vec![0f32; n_rows * w];
        let mut h_d = vec![0f32; n_rows * w];
        let mut h_dd = vec![0f32; n_rows * w];
        for k in 0..n_rows {
            let yy = ys + k as isize;
            if yy < 0 || yy >= h as isize {
                continue;
            }
            let src = img.row(yy as usize);
            conv_row(src, &a, &mut h_a[k * w..(k + 1) * w]);
            conv_row(src, &d, &mut h_d[k * w..(k + 1) * w]);
            conv_row(src, &dd, &mut h_dd[k * w..(k + 1) * w]);
        }
        let mut rxx = vec![0f32; w];
        let mut ryy = vec![0f32; w];
        let mut rxy = vec![0f32; w];
        for by in 0..bh {
            rxx.fill(0.0);
            ryy.fill(0.0);
            rxy.fill(0.0);
            // out(y) = sum_s k[2r - s] * tmp[by + s]
            for s in 0..=2 * r {
                let base = (by + s) * w;
                let (ka, kd, kdd) = (a[2 * r - s], d[2 * r - s], dd[2 * r - s]);
                let (ra, rd, rdd) = (&h_a[base..base + w], &h_d[base..base + w], &h_dd[base..base + w]);
                for x in 0..w {
                    rxx[x] += ka * rdd[x];
                    ryy[x] += kdd * ra[x];
                    rxy[x] += kd * rd[x];
                }
            }
            let o = &mut band[by * w..(by + 1) * w];
            for x in 0..w {
                let (xx, yy, xy) = (rxx[x], ryy[x], rxy[x]);
                let tmp = ((xx - yy) * (xx - yy) + 4.0 * xy * xy).sqrt();
                let e1 = 0.5 * (xx + yy - tmp);
                let e2 = 0.5 * (xx + yy + tmp);
                o[x] = if e2.abs() > e1.abs() { e2.abs() } else { e1.abs() };
            }
        }
    });
    out
}

/// Calls `f(y, sums, sums_sq, n_valid)` for every output row with the box sums over the
/// window `[x-before, x+after] × [y-before, y+after]` (zero padded).
fn box_rows<T, F>(img: &Image<T>, before: usize, after: usize, out_len: usize, f: F) -> Vec<f64>
where
    T: Copy + Send + Sync + Into<f64>,
    F: Fn(usize, &[f64], &[f64], &mut [f64]) + Sync,
{
    let (w, h) = (img.w, img.h);
    let win = before + after + 1;
    let mut out = vec![0f64; out_len * h];
    out.par_chunks_mut(out_len * BAND).enumerate().for_each(|(bi, band)| {
        let y0 = bi * BAND;
        let bh = band.len() / out_len;
        let n_rows = bh + win - 1;
        let ys = y0 as isize - before as isize;
        let mut hs = vec![0f64; n_rows * w];
        let mut hq = vec![0f64; n_rows * w];
        for k in 0..n_rows {
            let yy = ys + k as isize;
            if yy < 0 || yy >= h as isize {
                continue;
            }
            let row = img.row(yy as usize);
            let mut pre = vec![0f64; w + 1];
            let mut preq = vec![0f64; w + 1];
            for x in 0..w {
                let v: f64 = row[x].into();
                pre[x + 1] = pre[x] + v;
                preq[x + 1] = preq[x] + v * v;
            }
            for x in 0..w {
                let lo = x.saturating_sub(before);
                let hi = (x + after + 1).min(w);
                hs[k * w + x] = pre[hi] - pre[lo];
                hq[k * w + x] = preq[hi] - preq[lo];
            }
        }
        let mut s = vec![0f64; w];
        let mut q = vec![0f64; w];
        for by in 0..bh {
            s.fill(0.0);
            q.fill(0.0);
            for t in 0..win {
                let base = (by + t) * w;
                for x in 0..w {
                    s[x] += hs[base + x];
                    q[x] += hq[base + x];
                }
            }
            f(y0 + by, &s, &q, &mut band[by * out_len..(by + 1) * out_len]);
        }
    });
    out
}

/// `wiener2(img, [10 10])` for 16-bit input, returning 16-bit output.
pub fn wiener2_10(img: &Image<u16>) -> Image<u16> {
    let n = 100.0;
    // Pass 1: noise = mean local variance.
    let noise_rows = box_rows(img, 4, 5, 1, |_, s, q, out| {
        out[0] = s.iter().zip(q).map(|(&s, &q)| q / n - (s / n) * (s / n)).sum();
    });
    let noise = noise_rows.iter().sum::<f64>() / img.len() as f64;
    // Pass 2: filtered output.
    let w = img.w;
    let vals = box_rows(img, 4, 5, w, |y, s, q, out| {
        let row = img.row(y);
        for x in 0..w {
            let m = s[x] / n;
            let var = q[x] / n - m * m;
            let g = (var - noise).max(0.0);
            let lv = var.max(noise);
            let v = m + (row[x] as f64 - m) * g / lv;
            out[x] = if v.is_nan() { 0.0 } else { v };
        }
    });
    Image::from_vec(w, img.h, vals.into_iter().map(|v| v.round().clamp(0.0, 65535.0) as u16).collect())
}

/// `medfilt2(img, [10 10]) > t` without computing the full median: a window's median
/// exceeds `t` iff more than half its values do; ties at exactly 50 are resolved by an
/// exact selection over that window. Padding is zeros, as in `medfilt2`.
pub fn median10_gt(img: &Image<f32>, t: f32) -> Mask {
    let ind = img.map(|v| (v > t) as u8 as f32);
    let pad_gt = 0.0 > t;
    let (w, h) = (img.w, img.h);
    let counts = box_rows(&ind, 4, 5, w, |y, s, _, out| {
        let ry = (y + 5).min(h - 1) as isize - y.saturating_sub(4) as isize + 1;
        for x in 0..w {
            let rx = (x + 5).min(w - 1) as isize - x.saturating_sub(4) as isize + 1;
            let pad = 100 - (rx * ry) as usize;
            out[x] = s[x] + if pad_gt { pad as f64 } else { 0.0 };
        }
    });
    let data: Vec<bool> = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let c = counts[i].round() as usize;
            if c >= 51 {
                true
            } else if c <= 49 {
                false
            } else {
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                let mut win = Vec::with_capacity(100);
                for dy in -4..=5isize {
                    for dx in -4..=5isize {
                        let (xx, yy) = (x + dx, y + dy);
                        win.push(if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                            0.0
                        } else {
                            img.data[yy as usize * w + xx as usize]
                        });
                    }
                }
                win.sort_by(|a, b| a.total_cmp(b));
                (win[49] + win[50]) / 2.0 > t
            }
        })
        .collect();
    Image::from_vec(w, h, data)
}

/// Half widths of the rows of `strel('disk', r, 0)`, for dy = -r..=r.
pub fn disk_halfwidths(r: usize) -> Vec<usize> {
    let r2 = (r * r) as i64;
    (-(r as i64)..=r as i64)
        .map(|dy| {
            let mut w = 0i64;
            while (w + 1) * (w + 1) + dy * dy <= r2 {
                w += 1;
            }
            w as usize
        })
        .collect()
}

/// `conv2(double(mask), disk(r), 'same') > thresh`: counts set pixels within a disk.
pub fn disk_count_gt(mask: &Mask, r: usize, thresh: f64) -> Mask {
    let hw = disk_halfwidths(r);
    let (w, h) = (mask.w, mask.h);
    let mut out = Mask::filled(w, h, false);
    out.data.par_chunks_mut(w * BAND).enumerate().for_each(|(bi, band)| {
        let y0 = bi * BAND;
        let bh = band.len() / w;
        let ys = y0 as isize - r as isize;
        let n_rows = bh + 2 * r;
        let mut pre = vec![0u32; n_rows * (w + 1)];
        for k in 0..n_rows {
            let yy = ys + k as isize;
            if yy < 0 || yy >= h as isize {
                continue;
            }
            let row = mask.row(yy as usize);
            let p = &mut pre[k * (w + 1)..(k + 1) * (w + 1)];
            for x in 0..w {
                p[x + 1] = p[x] + row[x] as u32;
            }
        }
        let mut acc = vec![0u32; w];
        for by in 0..bh {
            acc.fill(0);
            for (j, &hwj) in hw.iter().enumerate() {
                let yy = y0 as isize + by as isize + j as isize - r as isize;
                if yy < 0 || yy >= h as isize {
                    continue;
                }
                let p = &pre[(by + j) * (w + 1)..(by + j + 1) * (w + 1)];
                for x in 0..w {
                    let lo = x.saturating_sub(hwj);
                    let hi = (x + hwj + 1).min(w);
                    acc[x] += p[hi] - p[lo];
                }
            }
            let o = &mut band[by * w..(by + 1) * w];
            for x in 0..w {
                o[x] = acc[x] as f64 > thresh;
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_matches_strel() {
        // strel('disk', 2, 0) has 13 elements: rows of widths 1, 3, 5, 3, 1.
        assert_eq!(disk_halfwidths(2), vec![0, 1, 2, 1, 0]);
    }

    #[test]
    fn conv_row_is_same_convolution() {
        let src = [1.0, 2.0, 3.0, 4.0];
        let k = [1.0, 0.0, -1.0];
        let mut dst = [0.0; 4];
        conv_row(&src, &k, &mut dst);
        // conv([1 2 3 4], [1 0 -1], 'same') = [2 2 2 -3]
        assert_eq!(dst, [2.0, 2.0, 2.0, -3.0]);
    }

    #[test]
    fn pacefilt_responds_to_ridges() {
        let mut img = Image::filled(41, 41, 0.0f32);
        for y in 0..41 {
            img.set(20, y, 1.0);
        }
        let r = pacefilt(&img, 21, 5.0);
        assert!(r.get(20, 20) > r.get(5, 20));
        assert!(r.get(20, 20) > 0.0);
    }

    #[test]
    fn median_threshold_matches_bruteforce() {
        let w = 23;
        let h = 17;
        let img = Image::from_vec(w, h, (0..w * h).map(|i| ((i * 7919) % 101) as f32 / 100.0).collect());
        let fast = median10_gt(&img, 0.5);
        for y in 0..h {
            for x in 0..w {
                let mut win = vec![];
                for dy in -4..=5isize {
                    for dx in -4..=5isize {
                        let (xx, yy) = (x as isize + dx, y as isize + dy);
                        win.push(if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                            0.0
                        } else {
                            img.get(xx as usize, yy as usize)
                        });
                    }
                }
                win.sort_by(|a, b| a.total_cmp(b));
                assert_eq!(fast.get(x, y), (win[49] + win[50]) / 2.0 > 0.5, "({x},{y})");
            }
        }
    }
}
