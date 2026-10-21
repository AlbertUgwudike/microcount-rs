//! Per-cell segmentation and morphometry shared by every algorithm:
//! `floodfill.m`, `rotundity.m`, `count_branches.m`, `branch_length.m`, `scholl.m`.

use crate::core::image::{Image, Mask};
use crate::core::imgproc::{geometry, label, skeleton};
use crate::core::Cancel;
use anyhow::Result;
use rayon::prelude::*;

pub struct Cells {
    /// Cell label per pixel (1-based, 0 = background).
    pub segmented: Image<u32>,
    /// Number of soma seeds (= somas found).
    pub n_seeds: usize,
    /// `max(segmented)` — the cell count used by the summary statistics.
    pub n_cells: usize,
    pub cell_areas: Vec<f64>,
    pub soma_areas: Vec<f64>,
    pub rotundities: Vec<f64>,
    /// Cell outlines (label per outline pixel).
    pub poly: Image<u32>,
    /// Skeleton branch points.
    pub detected: Mask,
    pub branch_counts: Vec<f64>,
    /// Skeleton pixels carrying their cell label.
    pub skeleton: Image<u32>,
    /// Path length (pixels) from the soma to every branch tip, per cell.
    pub branch_lengths: Vec<Vec<u32>>,
    pub scholl: Vec<f64>,
    pub crossings: Vec<Vec<u32>>,
}

/// Multi-source breadth-first fill from the soma centroids through `binary`, cells
/// taking turns one BFS layer at a time (so a pixel goes to the nearest soma). Returns
/// the labels and, per seed, the number of points appended (MATLAB's `height(pts)`).
pub fn floodfill(seeds: &[(usize, usize)], binary: &Mask) -> (Image<u32>, Vec<u64>) {
    let (w, h) = (binary.w, binary.h);
    let mut visited: Vec<bool> = binary.data.par_iter().map(|&b| !b).collect();
    let mut lab = Image::filled(w, h, 0u32);
    let n = seeds.len();
    let mut frontier: Vec<Vec<u32>> = seeds.iter().map(|&(x, y)| vec![(y * w + x) as u32]).collect();
    let mut appended = vec![1u64; n];
    let mut next: Vec<u32> = Vec::new();
    loop {
        let mut any = false;
        for i in 0..n {
            if frontier[i].is_empty() {
                continue;
            }
            any = true;
            next.clear();
            for &p in &frontier[i] {
                let p = p as usize;
                lab.data[p] = (i + 1) as u32;
                let (x, y) = (p % w, p / w);
                // MATLAB direction order [0 1; 0 -1; -1 0; 1 0] in (row, col).
                let mut visit = |q: usize| {
                    if !visited[q] {
                        visited[q] = true;
                        next.push(q as u32);
                    }
                };
                if x + 1 < w {
                    visit(p + 1);
                }
                if x > 0 {
                    visit(p - 1);
                }
                if y > 0 {
                    visit(p - w);
                }
                if y + 1 < h {
                    visit(p + w);
                }
            }
            appended[i] += next.len() as u64;
            std::mem::swap(&mut frontier[i], &mut next);
        }
        if !any {
            break;
        }
    }
    (lab, appended)
}

struct CellShape {
    rotundity: f64,
    soma_area: f64,
    outline: Vec<u32>,
}

/// `rotundity.m` for one cell: convexity (pixels / convex hull pixels), soma area and
/// the outline of the cell plus its soma.
fn cell_shape(pts: &[u32], w: usize, appended: u64, soma: &Mask) -> CellShape {
    let xs = || pts.iter().map(|&p| p as usize % w);
    let ys = || pts.iter().map(|&p| p as usize / w);
    let (x0, x1) = (xs().min().unwrap(), xs().max().unwrap());
    let (y0, y1) = (ys().min().unwrap(), ys().max().unwrap());
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut local = vec![false; bw * bh];
    let mut hull_pts: Vec<(i64, i64)> = Vec::with_capacity(pts.len());
    for &p in pts {
        let (x, y) = (p as usize % w - x0, p as usize / w - y0);
        local[y * bw + x] = true;
        hull_pts.push((x as i64, y as i64));
    }
    let hull = geometry::convex_hull(&mut hull_pts);
    let rotundity = if hull.len() < 3 {
        f64::NAN
    } else {
        appended as f64 / geometry::polygon_pixel_count(&hull) as f64
    };
    let soma_local: Vec<bool> = (0..bw * bh)
        .map(|i| local[i] && soma.data[(y0 + i / bw) * w + x0 + i % bw])
        .collect();
    let perim = |m: &[bool], x: usize, y: usize| {
        let i = y * bw + x;
        m[i] && (x == 0 || y == 0 || x == bw - 1 || y == bh - 1 || !m[i - 1] || !m[i + 1] || !m[i - bw] || !m[i + bw])
    };
    let mut outline = Vec::new();
    let mut soma_area = 0.0;
    for y in 0..bh {
        for x in 0..bw {
            let frame = x == 0 || y == 0 || x == bw - 1 || y == bh - 1;
            if soma_local[y * bw + x] {
                soma_area += 1.0;
            }
            if perim(&local, x, y) || (!frame && perim(&soma_local, x, y)) {
                outline.push(((y0 + y) * w + x0 + x) as u32);
            }
        }
    }
    CellShape { rotundity, soma_area, outline }
}

/// Per-cell lengths from the skeleton point nearest the soma centroid to each tip,
/// walking the skeleton breadth-first with 8-connectivity (`branch_length.m`).
fn branch_lengths(skel: &Image<u32>, seeds: &[(usize, usize)]) -> Vec<Vec<u32>> {
    let (w, h) = (skel.w, skel.h);
    let n = seeds.len();
    // Start point: nearest skeleton pixel of the cell, first in column-major order on ties.
    let mut best: Vec<(u64, u64, usize)> = vec![(u64::MAX, u64::MAX, usize::MAX); n];
    for (p, &l) in skel.data.iter().enumerate() {
        if l == 0 || l as usize > n {
            continue;
        }
        let i = l as usize - 1;
        let (x, y) = (p % w, p / w);
        let (sx, sy) = seeds[i];
        let d = (x.abs_diff(sx).pow(2) + y.abs_diff(sy).pow(2)) as u64;
        let key = (x * h + y) as u64;
        if (d, key) < (best[i].0, best[i].1) {
            best[i] = (d, key, p);
        }
    }
    let mut dist = vec![0u32; w * h];
    let mut visited: Vec<bool> = skel.data.par_iter().map(|&l| l == 0).collect();
    let mut frontier: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n {
        let p = best[i].2;
        if p != usize::MAX {
            frontier[i].push(p as u32);
            dist[p] = 1;
            visited[p] = true;
        }
    }
    // MATLAB order: [0 1; 0 -1; -1 0; 1 0; 1 1; -1 -1; -1 1; 1 -1] as (row, col).
    const DIRS: [(isize, isize); 8] = [(1, 0), (-1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let mut lengths = vec![Vec::new(); n];
    let mut next = Vec::new();
    let mut neigh = Vec::with_capacity(8);
    loop {
        let mut any = false;
        for i in 0..n {
            if frontier[i].is_empty() {
                continue;
            }
            any = true;
            next.clear();
            for &p in &frontier[i] {
                let p = p as usize;
                let cur = dist[p];
                let (x, y) = ((p % w) as isize, (p / w) as isize);
                neigh.clear();
                for (dx, dy) in DIRS {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                        continue;
                    }
                    let q = yy as usize * w + xx as usize;
                    if visited[q] || skel.data[q] != skel.data[p] {
                        continue;
                    }
                    neigh.push(q);
                }
                if neigh.is_empty() {
                    lengths[i].push(cur);
                }
                for &q in &neigh {
                    next.push(q as u32);
                    dist[q] = cur + 1;
                    visited[q] = true;
                }
            }
            std::mem::swap(&mut frontier[i], &mut next);
        }
        if !any {
            break;
        }
    }
    lengths
}

/// `scholl.m`: per cell, the negative slope of log10(crossings / πr²) against r, with
/// crossings counted on rings of unit width around the soma centre of mass.
fn scholl(skel: &Image<u32>, soma_lab: &Image<u32>, px: [f64; 2]) -> (Vec<f64>, Vec<Vec<u32>>) {
    let n = skel.data.par_iter().copied().max().unwrap_or(0) as usize;
    if n == 0 {
        return (vec![], vec![]);
    }
    let w = skel.w;
    let mut pts: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (p, &l) in skel.data.iter().enumerate() {
        if l != 0 {
            pts[l as usize - 1].push(p as u32);
        }
    }
    let mut com = vec![(0f64, 0f64, 0u64); n];
    for (p, &l) in soma_lab.data.iter().enumerate() {
        if l != 0 && (l as usize) <= n {
            let c = &mut com[l as usize - 1];
            c.0 += (p % w + 1) as f64;
            c.1 += (p / w + 1) as f64;
            c.2 += 1;
        }
    }
    // Distances are in µm for sub-micron pixels and in pixels otherwise, as in MATLAB.
    let (sy, sx) = (px[0].min(1.0), px[1].min(1.0));
    (0..n)
        .into_par_iter()
        .map(|i| {
            let (cx, cy, cn) = com[i];
            if cn == 0 || pts[i].is_empty() {
                return (f64::NAN, vec![]);
            }
            let (comx, comy) = ((cx / cn as f64).round() * sx, (cy / cn as f64).round() * sy);
            let dists: Vec<u32> = pts[i]
                .iter()
                .map(|&p| {
                    let x = (p as usize % w + 1) as f64 * sx;
                    let y = (p as usize / w + 1) as f64 * sy;
                    ((x - comx).powi(2) + (y - comy).powi(2)).sqrt().ceil() as u32
                })
                .collect();
            let max_dist = *dists.iter().max().unwrap() as usize;
            if max_dist < 1 {
                return (f64::NAN, vec![]);
            }
            let mut counts = vec![0u32; max_dist];
            for d in dists {
                if d >= 1 {
                    counts[d as usize - 1] += 1;
                }
            }
            let (xs, ys): (Vec<f64>, Vec<f64>) = counts
                .iter()
                .enumerate()
                .filter(|(_, &c)| c > 0)
                .map(|(k, &c)| {
                    let r = (k + 1) as f64;
                    (r, (c as f64 / (std::f64::consts::PI * r * r)).log10())
                })
                .unzip();
            let coeff = if xs.len() < 2 {
                f64::NAN
            } else {
                let m = xs.len() as f64;
                let (mx, my) = (xs.iter().sum::<f64>() / m, ys.iter().sum::<f64>() / m);
                let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
                let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
                -(sxy / sxx)
            };
            (coeff, counts)
        })
        .unzip()
}

/// Segments cells from somas and the cell-marker foreground and measures them.
pub fn analyse(soma: &Mask, binary: &Mask, px: [f64; 2], cancel: &Cancel) -> Result<Cells> {
    let (w, h) = (soma.w, soma.h);
    let seeds = label::centroids(soma);
    let n_seeds = seeds.len();
    let (segmented, appended) = floodfill(&seeds, binary);
    cancel.check()?;

    // Group pixels per label (CSR) for the per-cell shape measurements.
    let mut counts = vec![0usize; n_seeds + 2];
    for &l in &segmented.data {
        counts[l as usize + 1] += 1;
    }
    for i in 1..counts.len() {
        counts[i] += counts[i - 1];
    }
    let mut fill = counts.clone();
    let mut members = vec![0u32; segmented.len()];
    for (p, &l) in segmented.data.iter().enumerate() {
        members[fill[l as usize]] = p as u32;
        fill[l as usize] += 1;
    }
    let shapes: Vec<CellShape> = (0..n_seeds)
        .into_par_iter()
        .map(|i| {
            let l = i + 1;
            let mut pts = members[counts[l]..counts[l + 1]].to_vec();
            let (sx, sy) = seeds[i];
            let seed = (sy * w + sx) as u32;
            if segmented.data[seed as usize] as usize != l {
                pts.push(seed);
            }
            cell_shape(&pts, w, appended[i], soma)
        })
        .collect();
    drop(members);
    let mut poly = Image::filled(w, h, 0u32);
    for (i, s) in shapes.iter().enumerate() {
        for &p in &s.outline {
            poly.data[p as usize] = (i + 1) as u32;
        }
    }
    cancel.check()?;

    let n_cells = segmented.data.par_iter().copied().max().unwrap_or(0) as usize;
    let skel_mask = skeleton::skeletonize(&segmented.map(|l| l > 0));
    cancel.check()?;
    let detected = skeleton::branch_points(&skel_mask);
    let skel = segmented.zip_map(&skel_mask, |l, s| if s { l } else { 0 });
    let mut branch_counts = vec![0f64; n_cells];
    for (p, &d) in detected.data.iter().enumerate() {
        let l = segmented.data[p];
        if d && l > 0 {
            branch_counts[l as usize - 1] += 1.0;
        }
    }
    let lengths = branch_lengths(&skel, &seeds);
    cancel.check()?;
    let soma_lab = segmented.zip_map(soma, |l, s| if s { l } else { 0 });
    let (scholl, crossings) = scholl(&skel, &soma_lab, px);

    Ok(Cells {
        n_seeds,
        n_cells,
        cell_areas: appended.iter().map(|&a| a as f64).collect(),
        soma_areas: shapes.iter().map(|s| s.soma_area).collect(),
        rotundities: shapes.iter().map(|s| s.rotundity).collect(),
        segmented,
        poly,
        detected,
        branch_counts,
        skeleton: skel,
        branch_lengths: lengths,
        scholl,
        crossings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floodfill_splits_between_nearest_somas() {
        let binary = Mask::filled(11, 1, true);
        let (lab, app) = floodfill(&[(0, 0), (10, 0)], &binary);
        assert_eq!(lab.data[..5], [1, 1, 1, 1, 1]);
        assert_eq!(lab.data[6..], [2, 2, 2, 2, 2]);
        // Seeds are not marked visited, so each seed is re-appended once by its neighbour.
        assert_eq!(app.iter().sum::<u64>(), 11 + 2);
    }

    #[test]
    fn analyse_star_cell() {
        let mut soma = Mask::filled(41, 41, false);
        let mut bin = Mask::filled(41, 41, false);
        for y in 17..24 {
            for x in 17..24 {
                soma.set(x, y, true);
                bin.set(x, y, true);
            }
        }
        for i in 2..39 {
            bin.set(20, i, true);
            bin.set(i, 20, true);
        }
        let c = analyse(&soma, &bin, [1.0, 1.0], &Cancel::default()).unwrap();
        assert_eq!(c.n_cells, 1);
        assert_eq!(c.branch_lengths[0].len(), 4);
        assert!(c.rotundities[0] > 0.0 && c.rotundities[0] < 1.0);
        assert_eq!(c.soma_areas[0], 49.0);
        assert!(c.scholl[0].is_finite());
    }
}
