//! The segmentation algorithms (`algo_*.m`) and their normalisation parameters
//! (`Utility.get_norm_*`), which previews compute over the whole region and then apply
//! to a small crop so the preview matches the full run.

use super::cells;
use super::data::{partition, Data};
use super::{Algorithm, Settings};
use crate::core::image::{apply_mask, Image, Mask};
use crate::core::imgproc::stats::{
    imadjust_f32, log_norm, log_norm_params, mat2gray, min_max, nan_background, perc, stretchlim_f32, NormParams,
};
use crate::core::imgproc::{clahe, filters, label, morph};
use crate::core::Cancel;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use std::sync::Arc;

/// Channels of one region crop. Channel images are unmasked; `mask` marks the region.
pub struct RegionInput {
    pub mask: Mask,
    pub n_pixels: u64,
    pub cell: Option<Image<u16>>,
    pub co: Option<Image<u16>>,
    /// Every sample of the first page (brightfield RGB for DAB stains).
    pub rgb: Option<Vec<Image<u16>>>,
    /// Largest value of the source data type (255 for 8-bit files).
    pub full_range: u16,
}

/// Which channels an algorithm reads.
pub struct Needs {
    pub cell: bool,
    pub co: bool,
    pub rgb: bool,
}

impl Algorithm {
    pub fn needs(self) -> Needs {
        use Algorithm::*;
        match self {
            MicroFluor | MicroPlaque | AstroFluor => Needs { cell: true, co: true, rgb: false },
            MicroDab | AstroDab => Needs { cell: false, co: false, rgb: true },
            AstroSingle => Needs { cell: true, co: false, rgb: false },
        }
    }
}

#[derive(Clone, Debug)]
pub enum Norm {
    Fluor { co: NormParams, cell: NormParams, range: (f64, f64), stretch: (f64, f64) },
    Dab { first: NormParams, second: NormParams },
    AstroFluor { first: NormParams, second: NormParams, third: NormParams, fourth: (f64, f64) },
    None,
}

fn need<'a, T>(v: &'a Option<T>, what: &str) -> Result<&'a T> {
    v.as_ref().ok_or_else(|| anyhow!("{what} channel was not loaded"))
}

fn masked(img: &Image<u16>, mask: &Mask) -> Image<u16> {
    let mut m = img.clone();
    apply_mask(&mut m, mask, 0);
    m
}

fn min_channel(chans: &[Image<u16>]) -> Image<u16> {
    let mut out = chans[0].clone();
    for c in &chans[1..] {
        out.data.par_iter_mut().zip(c.data.par_iter()).for_each(|(a, &b)| *a = (*a).min(b));
    }
    out
}

fn as_f32(img: &Image<u16>) -> Image<f32> {
    img.map(|v| v as f32)
}

fn pacefilt4(img: &Image<f32>) -> Image<f32> {
    let mut p = filters::pacefilt(img, 21, 5.0);
    p.data.par_iter_mut().for_each(|v| *v /= 4.0);
    p
}

/// `log_norm(X) > t`.
fn log_threshold(x: &Image<f32>, params: Option<NormParams>, t: f64) -> Mask {
    let (z, _) = log_norm(x, params);
    z.map(|v| v as f64 > t)
}

/// Co-marker mask when no co-marker is imaged (DAB): `segment_activation` on an all-zero
/// image normalises to zeros, so the mask is `0 > threshold` everywhere.
fn constant_comarker(w: usize, h: usize, s: &Settings) -> Mask {
    let m = Mask::filled(w, h, 0.0 > s.comarker_threshold);
    label::remove_larger_than(&m, s.max_comarker_size)
}

// ----- Fluorescent microglia (+ plaques) -------------------------------------------------

/// Shared pre-processing of `segment_microglia.m`: closed ridge response.
fn fluor_ridges(cell: &Image<u16>, range: Option<(f64, f64)>) -> Image<f32> {
    let g = mat2gray(&as_f32(cell), range);
    morph::close_f32(&pacefilt4(&g), 2)
}

fn norm_fluor(inp: &RegionInput) -> Result<Norm> {
    let cell = masked(need(&inp.cell, "cell marker")?, &inp.mask);
    let co = masked(need(&inp.co, "co-marker")?, &inp.mask);
    let co_p = log_norm_params(&nan_background(&co, &inp.mask, |v| v as f32));
    let cell_p = log_norm_params(&nan_background(&cell, &inp.mask, |v| v as f32));
    let range = min_max(&as_f32(&cell));
    let stretch = stretchlim_f32(&fluor_ridges(&cell, Some(range)));
    Ok(Norm::Fluor { co: co_p, cell: cell_p, range, stretch })
}

fn run_fluor(inp: &RegionInput, s: &Settings, norm: Option<&Norm>, plaque: bool, cancel: &Cancel) -> Result<Data> {
    let (co_p, cell_p, range, stretch) = match norm {
        Some(Norm::Fluor { co, cell, range, stretch }) => (Some(*co), Some(*cell), Some(*range), Some(*stretch)),
        _ => (None, None, None, None),
    };
    let mask = &inp.mask;
    let cell = masked(need(&inp.cell, "cell marker")?, mask);
    let co = masked(need(&inp.co, "co-marker")?, mask);

    let co_nan = nan_background(&co, mask, |v| v as f32);
    let co_mask = if plaque {
        // segment_plaques.m
        let raw = log_threshold(&co_nan, co_p, 1.0);
        let clean = morph::open(&raw, 5);
        filters::disk_count_gt(&clean, 100, s.comarker_threshold * 1000.0)
    } else {
        let m = log_threshold(&co_nan, co_p, s.comarker_threshold);
        label::remove_larger_than(&m, s.max_comarker_size)
    };
    drop(co_nan);
    cancel.check()?;

    // segment_somas.m
    let cell_nan = nan_background(&cell, mask, |v| v as f32);
    let soma = label::fill_holes(&morph::open(&log_threshold(&cell_nan, cell_p, s.soma_threshold), 7));
    drop(cell_nan);
    cancel.check()?;

    // segment_microglia.m
    let ridges = fluor_ridges(&cell, range);
    let sl = stretch.unwrap_or_else(|| stretchlim_f32(&ridges));
    let branches = imadjust_f32(&ridges, sl).map(|v| v as f64 > s.cell_threshold);
    drop(ridges);
    cancel.check()?;

    finish(inp, vec![cell], co, co_mask, soma, branches, s, cancel)
}

// ----- DAB microglia ---------------------------------------------------------------------

/// Brightfield DAB: darker = more stain. Returns the inverted, normalised stain image.
fn dab_stain(min_img: &Image<u16>, mask: &Mask, first: Option<NormParams>) -> (Image<f32>, NormParams) {
    let scl = nan_background(min_img, mask, |v| v as f32 / 65535.0);
    let (norm, p) = log_norm(&scl, first);
    let mut stain = mat2gray(&norm, Some((-2.5, 3.0))).map(|v| 1.0 - v);
    apply_mask(&mut stain, mask, 0.0);
    (stain, p)
}

fn norm_dab(inp: &RegionInput, wiener: bool) -> Result<Norm> {
    let mut min_img = min_channel(need(&inp.rgb, "RGB")?);
    if wiener {
        min_img = filters::wiener2_10(&min_img);
    }
    let (stain, first) = dab_stain(&min_img, &inp.mask, None);
    let p = pacefilt4(&stain);
    let second = if wiener {
        log_norm_params(&p.zip_map(&inp.mask, |v, m| if m { v } else { f32::NAN }))
    } else {
        log_norm_params(&p)
    };
    Ok(Norm::Dab { first, second })
}

fn run_dab_micro(inp: &RegionInput, s: &Settings, norm: Option<&Norm>, cancel: &Cancel) -> Result<Data> {
    let (first, second) = match norm {
        Some(Norm::Dab { first, second }) => (Some(*first), Some(*second)),
        _ => (None, None),
    };
    let rgb = need(&inp.rgb, "RGB")?;
    let (stain, _) = dab_stain(&min_channel(rgb), &inp.mask, first);
    cancel.check()?;
    let branches = log_threshold(&pacefilt4(&stain), second, s.cell_threshold);
    cancel.check()?;
    let soma = stain.map(|v| v as f64 >= s.soma_threshold);
    let soma = morph::open(&label::area_filter(&soma, 1000, 100_000), 5);
    let (w, h) = (stain.w, stain.h);
    let co_mask = constant_comarker(w, h, s);
    finish(inp, rgb.clone(), Image::filled(w, h, 0), co_mask, soma, branches, s, cancel)
}

// ----- DAB astrocytes --------------------------------------------------------------------

fn run_dab_astro(inp: &RegionInput, s: &Settings, norm: Option<&Norm>, cancel: &Cancel) -> Result<Data> {
    let (first, second) = match norm {
        Some(Norm::Dab { first, second }) => (Some(*first), Some(*second)),
        _ => (None, None),
    };
    let rgb = need(&inp.rgb, "RGB")?;
    let filt = filters::wiener2_10(&min_channel(rgb));
    cancel.check()?;
    let (stain, _) = dab_stain(&filt, &inp.mask, first);
    drop(filt);
    let pp = pacefilt4(&stain).zip_map(&inp.mask, |v, m| if m { v } else { f32::NAN });
    let (mut p_out, _) = log_norm(&pp, second);
    apply_mask(&mut p_out, &inp.mask, 0.0);
    drop(pp);
    cancel.check()?;
    let branches = p_out.map(|v| v as f64 > s.cell_threshold);
    let soma = filters::median10_gt(&p_out, s.soma_threshold as f32);
    let (w, h) = (stain.w, stain.h);
    let co_mask = constant_comarker(w, h, s);
    finish(inp, rgb.clone(), Image::filled(w, h, 0), co_mask, soma, branches, s, cancel)
}

// ----- Fluorescent astrocytes ------------------------------------------------------------

fn astro_stain(img: &Image<u16>, mask: &Mask, first: Option<NormParams>) -> (Image<f32>, NormParams) {
    let scl = nan_background(img, mask, |v| v as f32 / 65535.0);
    let (norm, p) = log_norm(&scl, first);
    let mut stain = mat2gray(&norm, Some((-2.0, 2.0)));
    apply_mask(&mut stain, mask, 0.0);
    (stain, p)
}

fn norm_astro_fluor(inp: &RegionInput) -> Result<Norm> {
    let img = need(&inp.cell, "cell marker")?;
    let (stain, first) = astro_stain(img, &inp.mask, None);
    let second = log_norm_params(&pacefilt4(&stain));
    let co = masked(need(&inp.co, "co-marker")?, &inp.mask);
    let third = log_norm_params(&nan_background(&co, &inp.mask, |v| v as f32));
    let fourth = min_max(&as_f32(&clahe::adapthisteq(img, 0.02, inp.full_range)));
    Ok(Norm::AstroFluor { first, second, third, fourth })
}

fn run_fluor_astro(inp: &RegionInput, s: &Settings, norm: Option<&Norm>, cancel: &Cancel) -> Result<Data> {
    let (first, second, third, fourth) = match norm {
        Some(Norm::AstroFluor { first, second, third, fourth }) => (Some(*first), Some(*second), Some(*third), Some(*fourth)),
        _ => (None, None, None, None),
    };
    let img = need(&inp.cell, "cell marker")?;
    let (stain, _) = astro_stain(img, &inp.mask, first);
    let branches = log_threshold(&pacefilt4(&stain), second, s.cell_threshold);
    drop(stain);
    cancel.check()?;

    let co = masked(need(&inp.co, "co-marker")?, &inp.mask);
    let co_mask = log_threshold(&nan_background(&co, &inp.mask, |v| v as f32), third, s.comarker_threshold);
    let co_mask = label::remove_larger_than(&co_mask, s.max_comarker_size);
    cancel.check()?;

    let ad = as_f32(&clahe::adapthisteq(img, 0.02, inp.full_range));
    let soma = mat2gray(&ad, fourth).map(|v| v as f64 > s.soma_threshold);
    drop(ad);
    let soma = morph::open(&label::fill_holes(&soma), 5);
    finish(inp, vec![img.clone()], co, co_mask, soma, branches, s, cancel)
}

// ----- Single astrocyte ------------------------------------------------------------------

/// `trace_branches.m`: number of skeleton pixels in the sub-tree hanging off each pixel
/// when the skeleton is walked breadth-first from the soma outline.
fn trace_branches(origins: &[usize], skel: &Mask) -> Vec<u32> {
    let (w, h) = (skel.w, skel.h);
    let mut dist = vec![0u32; w * h];
    let mut to_visit = skel.data.clone();
    const DIRS: [(isize, isize); 8] = [(1, 0), (-1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let mut queue: std::collections::VecDeque<usize> = origins.iter().rev().copied().collect();
    let mut order: Vec<(usize, usize, usize)> = Vec::new(); // (pixel, child start, child end)
    let mut children: Vec<usize> = Vec::new();
    while let Some(p) = queue.pop_front() {
        let start = children.len();
        let (x, y) = ((p % w) as isize, (p / w) as isize);
        for (dx, dy) in DIRS {
            let (xx, yy) = (x + dx, y + dy);
            if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                continue;
            }
            let q = yy as usize * w + xx as usize;
            if to_visit[q] {
                to_visit[q] = false;
                children.push(q);
                queue.push_back(q);
            }
        }
        order.push((p, start, children.len()));
    }
    for &(p, a, b) in order.iter().rev() {
        dist[p] = 1 + children[a..b].iter().map(|&c| dist[c]).sum::<u32>();
    }
    dist
}

/// `branchlets.m`: branch order labels — the walk continues along the heaviest sub-tree
/// and every side branch increments the order.
fn branchlets(weights: &[u32], w: usize, h: usize, origins: &[usize]) -> Vec<u32> {
    let mut lab = vec![0u32; w * h];
    let mut visited: Vec<bool> = weights.iter().map(|&v| v == 0).collect();
    let mut frontier: Vec<Vec<usize>> = origins.iter().map(|&p| vec![p]).collect();
    for &p in origins {
        lab[p] = 1;
        visited[p] = true;
    }
    const DIRS: [(isize, isize); 8] = [(1, 0), (-1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let mut neigh = Vec::with_capacity(8);
    loop {
        let mut any = false;
        for f in frontier.iter_mut() {
            if f.is_empty() {
                continue;
            }
            any = true;
            let mut next = Vec::new();
            for &p in f.iter() {
                let cur = lab[p];
                let (x, y) = ((p % w) as isize, (p / w) as isize);
                neigh.clear();
                for (dx, dy) in DIRS {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                        continue;
                    }
                    let q = yy as usize * w + xx as usize;
                    if !visited[q] {
                        neigh.push(q);
                    }
                }
                let mut max_k = 0;
                let mut max_wt = 0;
                for (k, &q) in neigh.iter().enumerate() {
                    if weights[q] > max_wt {
                        max_k = k;
                        max_wt = weights[q];
                    }
                }
                for (k, &q) in neigh.iter().enumerate() {
                    next.push(q);
                    lab[q] = if k == max_k { cur } else { cur + 1 };
                    visited[q] = true;
                }
            }
            *f = next;
        }
        if !any {
            break;
        }
    }
    lab
}

fn run_single_astro(inp: &RegionInput, s: &Settings, cancel: &Cancel) -> Result<Data> {
    let cell = masked(need(&inp.cell, "cell marker")?, &inp.mask);
    let (w, h) = (cell.w, cell.h);
    let soma = morph::open(&cell.map(|v| v as f64 > s.soma_threshold * 1000.0), 4);
    let soma = label::area_filter(&soma, 500, 10_000);
    let co_mask = cell.map(|v| v as f64 > s.comarker_threshold * 100.0);
    let branches = pacefilt4(&as_f32(&cell)).map(|v| v as f64 > s.cell_threshold);
    cancel.check()?;
    let c = cells::analyse(&soma, &branches, s.pixel_dims, cancel)?;
    cancel.check()?;

    let soma_perim = label::perimeter(&soma);
    // Origins in column-major order, as returned by MATLAB `find`.
    let mut origins = Vec::new();
    for x in 0..w {
        for y in 0..h {
            let p = y * w + x;
            if soma_perim.data[p] && c.skeleton.data[p] > 0 {
                origins.push(p);
            }
        }
    }
    let tree = Mask::from_vec(
        w,
        h,
        (0..w * h).into_par_iter().map(|p| c.skeleton.data[p] > 0 && !soma.data[p]).collect(),
    );
    let weights = trace_branches(&origins, &tree);
    let order = branchlets(&weights, w, h, &origins);
    let cd68 = Image::from_vec(w, h, order.into_iter().map(|v| v.min(65535) as u16).collect());
    let n = c.n_cells;
    Ok(Data {
        iba1_area_px: branches.count(),
        iba1: Arc::new(vec![cell]),
        cd68: Arc::new(cd68),
        cd68_mask: Arc::new(co_mask),
        soma_mask: soma,
        cells: c,
        overlap_pcs: vec![],
        n_activated: 0,
        n_microglia: n,
        n_pixels: inp.n_pixels,
    })
}

// ----- Shared tail -----------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn finish(
    inp: &RegionInput,
    iba1: Vec<Image<u16>>,
    cd68: Image<u16>,
    co_mask: Mask,
    soma: Mask,
    branches: Mask,
    s: &Settings,
    cancel: &Cancel,
) -> Result<Data> {
    let binary = soma.or(&branches);
    drop(branches);
    let iba1_area_px = binary.count();
    let c = cells::analyse(&soma, &binary, s.pixel_dims, cancel)?;
    drop(binary);
    let n = c.n_cells;
    let mut all = vec![0u64; n + 1];
    let mut ov = vec![0u64; n + 1];
    for (p, &l) in c.segmented.data.iter().enumerate() {
        if l > 0 {
            all[l as usize] += 1;
            if co_mask.data[p] {
                ov[l as usize] += 1;
            }
        }
    }
    let overlap_pcs: Vec<f64> = (1..=n).map(|l| perc(ov[l] as f64, all[l] as f64)).collect();
    let n_activated = overlap_pcs.iter().filter(|&&p| p >= s.min_overlap).count();
    Ok(Data {
        iba1: Arc::new(iba1),
        cd68: Arc::new(cd68),
        cd68_mask: Arc::new(co_mask),
        soma_mask: soma,
        cells: c,
        iba1_area_px,
        overlap_pcs,
        n_activated,
        n_microglia: n,
        n_pixels: inp.n_pixels,
    })
}

/// Normalisation parameters of a whole region, for previews.
pub fn compute_norm(algo: Algorithm, inp: &RegionInput) -> Result<Norm> {
    use Algorithm::*;
    match algo {
        MicroFluor | MicroPlaque => norm_fluor(inp),
        MicroDab => norm_dab(inp, false),
        AstroDab => norm_dab(inp, true),
        AstroFluor => norm_astro_fluor(inp),
        AstroSingle => Ok(Norm::None),
    }
}

/// Runs `algo` on a region. `norm` overrides the normalisation computed from `inp`.
pub fn run(algo: Algorithm, inp: &RegionInput, s: &Settings, norm: Option<&Norm>, cancel: &Cancel) -> Result<Data> {
    if inp.n_pixels == 0 {
        return Err(anyhow!("region is empty"));
    }
    use Algorithm::*;
    match algo {
        MicroFluor => run_fluor(inp, s, norm, false, cancel),
        MicroPlaque => run_fluor(inp, s, norm, true, cancel),
        MicroDab => run_dab_micro(inp, s, norm, cancel),
        AstroFluor => run_fluor_astro(inp, s, norm, cancel),
        AstroDab => run_dab_astro(inp, s, norm, cancel),
        AstroSingle => run_single_astro(inp, s, cancel),
    }
}

/// For the plaque algorithm: `(in_plaque, out_of_plaque)`.
pub fn split_plaque(d: &Data, s: &Settings) -> (Data, Data) {
    (partition(d, true, s.min_overlap), partition(d, false, s.min_overlap))
}
