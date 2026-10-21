//! Atlas-to-histology registration.
//!
//! Coordinates are MATLAB "world" coordinates: pixel (row r, col c), 0-based, has centre
//! `(x, y) = (c + 1, r + 1)`. Transforms map atlas slice coordinates to coordinates in
//! the padded, rotated downsampled histology image.

use crate::core::image::Image;
use anyhow::{bail, Result};
use rayon::prelude::*;

/// Row-major 3×3 affine matrix acting on column vectors `[x, y, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [[f64; 3]; 3]);

impl Affine {
    pub const IDENTITY: Affine = Affine([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = &self.0;
        (m[0][0] * x + m[0][1] * y + m[0][2], m[1][0] * x + m[1][1] * y + m[1][2])
    }

    pub fn mul(&self, o: &Affine) -> Affine {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Affine(r)
    }

    pub fn inverse(&self) -> Option<Affine> {
        let m = &self.0;
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        if det.abs() < 1e-12 {
            return None;
        }
        let (a, b, c, d) = (m[1][1] / det, -m[0][1] / det, -m[1][0] / det, m[0][0] / det);
        let tx = -(a * m[0][2] + b * m[1][2]);
        let ty = -(c * m[0][2] + d * m[1][2]);
        Some(Affine([[a, b, tx], [c, d, ty], [0.0, 0.0, 1.0]]))
    }

    fn from_params(p: &[f64; 6]) -> Affine {
        Affine([[p[0], p[1], p[4]], [p[2], p[3], p[5]], [0.0, 0.0, 1.0]])
    }

    fn params(&self) -> [f64; 6] {
        let m = &self.0;
        [m[0][0], m[0][1], m[1][0], m[1][1], m[0][2], m[1][2]]
    }
}

fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if d.abs() < 1e-12 {
        return None;
    }
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let mut m = a;
        for i in 0..3 {
            m[i][k] = b[i];
        }
        *o = det(m) / d;
    }
    Some(out)
}

/// Least-squares affine mapping `src` points onto `dst` (`fitgeotform2d(..., 'affine')`).
pub fn fit_affine(src: &[[f64; 2]], dst: &[[f64; 2]]) -> Result<Affine> {
    if src.len() < 3 || src.len() != dst.len() {
        bail!("need at least three matching control points");
    }
    let mut ata = [[0.0; 3]; 3];
    let mut atx = [0.0; 3];
    let mut aty = [0.0; 3];
    for (s, d) in src.iter().zip(dst) {
        let v = [s[0], s[1], 1.0];
        for i in 0..3 {
            for j in 0..3 {
                ata[i][j] += v[i] * v[j];
            }
            atx[i] += v[i] * d[0];
            aty[i] += v[i] * d[1];
        }
    }
    let (Some(rx), Some(ry)) = (solve3(ata, atx), solve3(ata, aty)) else {
        bail!("control points are collinear");
    };
    Ok(Affine([rx, ry, [0.0, 0.0, 1.0]]))
}

/// Default control hexagon centred on an image of `(rows, cols)` (`Utility.gen_hex`).
pub fn gen_hex(rows: usize, cols: usize) -> [[f64; 2]; 6] {
    let r = (0.75 * (rows.min(cols) as f64 / 2.0)).floor();
    let (cy, cx) = ((rows as f64 / 2.0).floor(), (cols as f64 / 2.0).floor());
    let mut out = [[0.0; 2]; 6];
    for (k, p) in out.iter_mut().enumerate() {
        let t = std::f64::consts::PI / 6.0 + k as f64 * std::f64::consts::PI / 3.0;
        *p = [r * t.cos() + cx, r * t.sin() + cy];
    }
    out
}

// ----- Automatic registration ------------------------------------------------------------

const BINS: usize = 32;

struct Level {
    moving: Image<f32>,
    /// World-coordinate scale of this level relative to full resolution.
    scale: f64,
    samples: Vec<(f64, f64, usize)>, // fixed world x, y and fixed bin
}

fn to_unit(img: &Image<u16>) -> Image<f32> {
    let (lo, hi) = img.data.iter().fold((u16::MAX, 0u16), |(a, b), &v| (a.min(v), b.max(v)));
    let span = (hi.saturating_sub(lo)).max(1) as f32;
    img.map(|v| (v.saturating_sub(lo)) as f32 / span)
}

fn downsample(img: &Image<f32>, f: usize) -> Image<f32> {
    if f == 1 {
        return img.clone();
    }
    let (w, h) = (img.w.div_ceil(f), img.h.div_ceil(f));
    let mut out = Image::filled(w, h, 0.0f32);
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut n) = (0.0, 0.0);
            for yy in y * f..((y + 1) * f).min(img.h) {
                for xx in x * f..((x + 1) * f).min(img.w) {
                    s += img.get(xx, yy);
                    n += 1.0;
                }
            }
            out.set(x, y, s / n);
        }
    }
    out
}

fn bilinear(img: &Image<f32>, x: f64, y: f64) -> Option<f32> {
    // World -> 0-based continuous pixel coordinates.
    let (u, v) = (x - 1.0, y - 1.0);
    if u < 0.0 || v < 0.0 || u > (img.w - 1) as f64 || v > (img.h - 1) as f64 {
        return None;
    }
    let (x0, y0) = (u.floor() as usize, v.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(img.w - 1), (y0 + 1).min(img.h - 1));
    let (fx, fy) = ((u - x0 as f64) as f32, (v - y0 as f64) as f32);
    let top = img.get(x0, y0) * (1.0 - fx) + img.get(x1, y0) * fx;
    let bot = img.get(x0, y1) * (1.0 - fx) + img.get(x1, y1) * fx;
    Some(top * (1.0 - fy) + bot * fy)
}

fn bin(v: f32) -> usize {
    ((v.clamp(0.0, 1.0) * (BINS - 1) as f32).round()) as usize
}

impl Level {
    fn new(fixed: &Image<f32>, moving: &Image<f32>, f: usize) -> Level {
        let fixed = downsample(fixed, f);
        let moving = downsample(moving, f);
        let step = ((fixed.len() as f64 / 20_000.0).sqrt().floor() as usize).max(1);
        let mut samples = Vec::new();
        for y in (0..fixed.h).step_by(step) {
            for x in (0..fixed.w).step_by(step) {
                samples.push(((x + 1) as f64, (y + 1) as f64, bin(fixed.get(x, y))));
            }
        }
        Level { moving, scale: f as f64, samples }
    }

    /// Negative mutual information of `t` (full-resolution atlas → histology).
    fn cost(&self, t: &Affine) -> f64 {
        // Express t at this level: x_l = (x_full - 0.5) / s + 0.5.
        let s = self.scale;
        let to_full = Affine([[s, 0.0, 0.5 - 0.5 * s], [0.0, s, 0.5 - 0.5 * s], [0.0, 0.0, 1.0]]);
        let Some(inv) = t.inverse() else { return 0.0 };
        let fwd = to_full.inverse().unwrap().mul(&inv).mul(&to_full);
        let joint = self
            .samples
            .par_iter()
            .fold(
                || vec![0u32; BINS * BINS],
                |mut j, &(x, y, fb)| {
                    let (u, v) = fwd.apply(x, y);
                    if let Some(m) = bilinear(&self.moving, u, v) {
                        j[fb * BINS + bin(m)] += 1;
                    }
                    j
                },
            )
            .reduce(
                || vec![0u32; BINS * BINS],
                |mut a, b| {
                    a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
                    a
                },
            );
        let n: u32 = joint.iter().sum();
        if (n as usize) < self.samples.len() / 10 {
            return 0.0; // too little overlap
        }
        let n = n as f64;
        let mut pf = [0f64; BINS];
        let mut pm = [0f64; BINS];
        for i in 0..BINS {
            for j in 0..BINS {
                let p = joint[i * BINS + j] as f64 / n;
                pf[i] += p;
                pm[j] += p;
            }
        }
        let mut mi = 0.0;
        for i in 0..BINS {
            for j in 0..BINS {
                let p = joint[i * BINS + j] as f64 / n;
                if p > 0.0 {
                    mi += p * (p / (pf[i] * pm[j])).ln();
                }
            }
        }
        -mi
    }
}

fn otsu(img: &Image<f32>) -> usize {
    let mut hist = [0u64; 256];
    for &v in &img.data {
        hist[bin256(v)] += 1;
    }
    hist[0] = 0;
    let total: u64 = hist.iter().sum();
    let sum: f64 = hist.iter().enumerate().map(|(i, &h)| i as f64 * h as f64).sum();
    let (mut wb, mut sb, mut best, mut thr) = (0f64, 0f64, -1f64, 0usize);
    for (t, &h) in hist.iter().enumerate() {
        wb += h as f64;
        if wb == 0.0 || wb >= total as f64 {
            continue;
        }
        sb += t as f64 * h as f64;
        let wf = total as f64 - wb;
        let between = wb * wf * (sb / wb - (sum - sb) / wf).powi(2);
        if between > best {
            best = between;
            thr = t;
        }
    }
    thr
}

/// Box-blurred (radius 2) image thresholded at its Otsu level, as 0/1.
fn silhouette(img: &Image<f32>) -> Image<f32> {
    let r = 2isize;
    let (w, h) = (img.w as isize, img.h as isize);
    let blurred = Image::from_vec(
        img.w,
        img.h,
        (0..img.len())
            .into_par_iter()
            .map(|i| {
                let (x, y) = ((i % img.w) as isize, (i / img.w) as isize);
                let (mut s, mut n) = (0f32, 0f32);
                for yy in (y - r).max(0)..=(y + r).min(h - 1) {
                    for xx in (x - r).max(0)..=(x + r).min(w - 1) {
                        s += img.data[(yy * w + xx) as usize];
                        n += 1.0;
                    }
                }
                s / n
            })
            .collect(),
    );
    let t = otsu(&blurred);
    blurred.map(|v| if bin256(v) > t { 1.0 } else { 0.0 })
}

/// Bounding box `(x0, y0, x1, y1)` in world coordinates of the foreground (Otsu
/// threshold), trimming 0.5% of the foreground coordinates on each side so stray bright
/// pixels do not stretch it.
fn tissue_box(img: &Image<f32>) -> Option<(f64, f64, f64, f64)> {
    let thr = otsu(img);
    let (mut xs, mut ys) = (vec![], vec![]);
    for y in 0..img.h {
        for x in 0..img.w {
            if bin256(img.get(x, y)) > thr {
                xs.push(x);
                ys.push(y);
            }
        }
    }
    if xs.len() < 10 {
        return None;
    }
    xs.sort_unstable();
    ys.sort_unstable();
    let q = |v: &[usize], f: f64| v[((v.len() - 1) as f64 * f) as usize] as f64 + 1.0;
    Some((q(&xs, 0.005), q(&ys, 0.005), q(&xs, 0.995), q(&ys, 0.995)))
}

fn bin256(v: f32) -> usize {
    (v.clamp(0.0, 1.0) * 255.0).round() as usize
}

/// Small deterministic normal generator (Box–Muller over a xorshift stream).
struct Rng(u64);

impl Rng {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        let (u1, u2) = (self.uniform().max(1e-12), self.uniform());
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Mutual-information affine registration of an atlas reference slice onto a
/// downsampled histology image (`auto_reg.m`). `hist` must already be padded and
/// rotated as displayed. Optimised with a (1+1) evolution strategy over a three-level
/// pyramid, like MATLAB's `imregconfig('multimodal')`.
pub fn auto_register(atlas: &Image<u16>, hist: &Image<u16>) -> Result<Affine> {
    let fixed = to_unit(hist);
    let moving = to_unit(atlas);
    // Initial guess: map the atlas brain's bounding box onto the tissue's, falling back
    // to fitting the whole slice into the image.
    let mut t = match (tissue_box(&moving), tissue_box(&fixed)) {
        (Some(a), Some(h)) => {
            let (sx, sy) = ((h.2 - h.0) / (a.2 - a.0).max(1.0), (h.3 - h.1) / (a.3 - a.1).max(1.0));
            Affine([[sx, 0.0, h.0 - sx * a.0], [0.0, sy, h.1 - sy * a.1], [0.0, 0.0, 1.0]])
        }
        _ => {
            let s = (hist.h as f64 / atlas.h as f64).min(hist.w as f64 / atlas.w as f64);
            let (acx, acy) = ((atlas.w as f64 + 1.0) / 2.0, (atlas.h as f64 + 1.0) / 2.0);
            let (hcx, hcy) = ((hist.w as f64 + 1.0) / 2.0, (hist.h as f64 + 1.0) / 2.0);
            Affine([[s, 0.0, hcx - s * acx], [0.0, s, hcy - s * acy], [0.0, 0.0, 1.0]])
        }
    };
    let s = (t.0[0][0] + t.0[1][1]) / 2.0;
    // Refine on silhouettes: speckled fluorescence and the smooth reference template
    // share little texture, but their outlines and ventricles agree.
    let fixed = silhouette(&fixed);
    let moving = silhouette(&moving);

    let extent = hist.w.max(hist.h) as f64;
    // Parameter scales: linear terms are O(s), translations O(image size).
    let scales = [s, s, s, s, extent, extent];
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for (factor, iters) in [(4usize, 400usize), (2, 250), (1, 150)] {
        let level = Level::new(&fixed, &moving, factor);
        let mut best = t.params();
        let mut best_cost = level.cost(&t);
        let mut radius = 0.02;
        let (grow, shrink) = (1.05f64, 1.05f64.powf(-0.25));
        for _ in 0..iters {
            let mut cand = best;
            for k in 0..6 {
                cand[k] += radius * scales[k] * rng.normal();
            }
            let c = level.cost(&Affine::from_params(&cand));
            if c < best_cost {
                best = cand;
                best_cost = c;
                radius *= grow;
            } else {
                radius *= shrink;
            }
            if radius < 1e-5 {
                break;
            }
        }
        t = Affine::from_params(&best);
    }
    let m = &t.0;
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if !det.is_finite() || det.abs() < 1e-6 {
        bail!("registration diverged");
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_recovers_known_affine() {
        let t = Affine([[1.5, 0.2, 10.0], [-0.1, 0.9, -3.0], [0.0, 0.0, 1.0]]);
        let src = gen_hex(100, 80);
        let dst: Vec<[f64; 2]> = src.iter().map(|p| {
            let (x, y) = t.apply(p[0], p[1]);
            [x, y]
        }).collect();
        let f = fit_affine(&src, &dst).unwrap();
        for i in 0..2 {
            for j in 0..3 {
                assert!((f.0[i][j] - t.0[i][j]).abs() < 1e-9);
            }
        }
        let inv = f.inverse().unwrap();
        let (x, y) = inv.apply(dst[0][0], dst[0][1]);
        assert!((x - src[0][0]).abs() < 1e-9 && (y - src[0][1]).abs() < 1e-9);
    }

    #[test]
    fn auto_register_recovers_shift_and_scale() {
        // A blob atlas and a histology image containing it scaled ×2 and offset.
        let mut atlas = Image::filled(60, 50, 0u16);
        for y in 0..50 {
            for x in 0..60 {
                let d = ((x as f64 - 25.0).powi(2) / 300.0 + (y as f64 - 22.0).powi(2) / 150.0).sqrt();
                atlas.set(x, y, if d < 1.0 { 40000 } else if d < 1.4 { 20000 } else { 0 });
            }
        }
        let truth = Affine([[2.0, 0.0, 30.0], [0.0, 2.0, 25.0], [0.0, 0.0, 1.0]]);
        let inv = truth.inverse().unwrap();
        let mut hist = Image::filled(200, 160, 0u16);
        for y in 0..160 {
            for x in 0..200 {
                let (u, v) = inv.apply((x + 1) as f64, (y + 1) as f64);
                let (u, v) = (u.round() as isize - 1, v.round() as isize - 1);
                if u >= 0 && v >= 0 && u < 60 && v < 50 {
                    hist.set(x, y, atlas.get(u as usize, v as usize));
                }
            }
        }
        let t = auto_register(&atlas, &hist).unwrap();
        let (x, y) = t.apply(26.0, 23.0);
        let (tx, ty) = truth.apply(26.0, 23.0);
        assert!((x - tx).abs() < 6.0 && (y - ty).abs() < 6.0, "({x},{y}) vs ({tx},{ty})");
    }
}
