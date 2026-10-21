//! Connected components and related binary operations.

use crate::core::image::{Image, Mask};
use rayon::prelude::*;

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// 8-connected labelling (`bwlabel` / `bwconncomp` default for 2-D). Labels are
/// numbered 1..=n in MATLAB's column-major discovery order so that downstream
/// per-cell ordering matches the original.
pub fn label8(mask: &Mask) -> (Image<u32>, usize) {
    let (w, h) = (mask.w, mask.h);
    let mut lab = Image::filled(w, h, 0u32);
    let mut parent: Vec<u32> = vec![0];
    for y in 0..h {
        for x in 0..w {
            if !mask.data[y * w + x] {
                continue;
            }
            let mut best = 0u32;
            let mut neigh = [0u32; 4];
            let mut nn = 0;
            let mut look = |xx: isize, yy: isize| {
                if xx >= 0 && yy >= 0 && (xx as usize) < w {
                    let l = lab.data[yy as usize * w + xx as usize];
                    if l != 0 {
                        neigh[nn] = l;
                        nn += 1;
                    }
                }
            };
            let (xi, yi) = (x as isize, y as isize);
            look(xi - 1, yi);
            look(xi - 1, yi - 1);
            look(xi, yi - 1);
            look(xi + 1, yi - 1);
            for &l in &neigh[..nn] {
                let r = find(&mut parent, l);
                best = if best == 0 { r } else { best.min(r) };
            }
            if best == 0 {
                best = parent.len() as u32;
                parent.push(best);
            } else {
                for &l in &neigh[..nn] {
                    let r = find(&mut parent, l);
                    if r != best {
                        parent[r as usize] = best;
                    }
                }
            }
            lab.data[y * w + x] = best;
        }
    }
    // Resolve roots and order them by first column-major occurrence.
    let n_prov = parent.len();
    let mut root_of = vec![0u32; n_prov];
    for i in 1..n_prov {
        root_of[i] = find(&mut parent, i as u32);
    }
    let mut first = vec![u64::MAX; n_prov];
    for y in 0..h {
        for x in 0..w {
            let l = lab.data[y * w + x];
            if l != 0 {
                let r = root_of[l as usize] as usize;
                let key = (x as u64) * h as u64 + y as u64;
                if key < first[r] {
                    first[r] = key;
                }
            }
        }
    }
    let mut roots: Vec<usize> = (1..n_prov).filter(|&i| root_of[i] as usize == i).collect();
    roots.sort_by_key(|&r| first[r]);
    let mut final_of_root = vec![0u32; n_prov];
    for (k, &r) in roots.iter().enumerate() {
        final_of_root[r] = k as u32 + 1;
    }
    let map: Vec<u32> = (0..n_prov).map(|i| if i == 0 { 0 } else { final_of_root[root_of[i] as usize] }).collect();
    lab.data.par_iter_mut().for_each(|l| *l = map[*l as usize]);
    (lab, roots.len())
}

pub fn component_sizes(lab: &Image<u32>, n: usize) -> Vec<u64> {
    let mut sizes = vec![0u64; n + 1];
    for &l in &lab.data {
        sizes[l as usize] += 1;
    }
    sizes
}

/// `bwareafilt(mask, [lo hi])`
pub fn area_filter(mask: &Mask, lo: u64, hi: u64) -> Mask {
    let (lab, n) = label8(mask);
    let sizes = component_sizes(&lab, n);
    lab.map(|l| l != 0 && (lo..=hi).contains(&sizes[l as usize]))
}

/// `filter_size_cd68.m`: removes components larger than `max`.
pub fn remove_larger_than(mask: &Mask, max: u64) -> Mask {
    area_filter(mask, 0, max)
}

/// Rounded centroids (0-based) of the 8-connected components, in label order
/// (`regionprops(mask, 'centroid')` followed by `round`).
pub fn centroids(mask: &Mask) -> Vec<(usize, usize)> {
    let (lab, n) = label8(mask);
    let mut acc = vec![(0f64, 0f64, 0u64); n + 1];
    for y in 0..lab.h {
        for x in 0..lab.w {
            let l = lab.data[y * lab.w + x] as usize;
            if l != 0 {
                acc[l].0 += x as f64;
                acc[l].1 += y as f64;
                acc[l].2 += 1;
            }
        }
    }
    acc[1..]
        .iter()
        .map(|&(sx, sy, c)| {
            let (cx, cy) = (sx / c as f64, sy / c as f64);
            // MATLAB rounds the 1-based centroid; for positive values this equals
            // rounding the 0-based one.
            ((cx.round() as usize).min(mask.w - 1), (cy.round() as usize).min(mask.h - 1))
        })
        .collect()
}

/// `imfill(mask, 'holes')`: background not 4-connected to the border becomes foreground.
pub fn fill_holes(mask: &Mask) -> Mask {
    let (w, h) = (mask.w, mask.h);
    let mut reached = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let seed = |i: usize, reached: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !mask.data[i] && !reached[i] {
            reached[i] = true;
            stack.push(i);
        }
    };
    for x in 0..w {
        seed(x, &mut reached, &mut stack);
        seed((h - 1) * w + x, &mut reached, &mut stack);
    }
    for y in 0..h {
        seed(y * w, &mut reached, &mut stack);
        seed(y * w + w - 1, &mut reached, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            seed(i - 1, &mut reached, &mut stack);
        }
        if x + 1 < w {
            seed(i + 1, &mut reached, &mut stack);
        }
        if y > 0 {
            seed(i - w, &mut reached, &mut stack);
        }
        if y + 1 < h {
            seed(i + w, &mut reached, &mut stack);
        }
    }
    Image::from_vec(w, h, reached.into_par_iter().map(|r| !r).collect())
}

/// `bwperim(mask)` (4-connected): set pixels touching the background or the border.
pub fn perimeter(mask: &Mask) -> Mask {
    let (w, h) = (mask.w, mask.h);
    let data: Vec<bool> = (0..w * h)
        .into_par_iter()
        .map(|i| {
            if !mask.data[i] {
                return false;
            }
            let (x, y) = (i % w, i / w);
            x == 0
                || y == 0
                || x == w - 1
                || y == h - 1
                || !mask.data[i - 1]
                || !mask.data[i + 1]
                || !mask.data[i - w]
                || !mask.data[i + w]
        })
        .collect();
    Image::from_vec(w, h, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_column_major_order() {
        // Component A starts at (x=3, y=0); component B at (x=0, y=2) -> B is first
        // in column-major order.
        let mut m = Mask::filled(5, 4, false);
        m.set(3, 0, true);
        m.set(4, 1, true); // diagonal neighbour of (3,0): same component
        m.set(0, 2, true);
        m.set(0, 3, true);
        let (lab, n) = label8(&m);
        assert_eq!(n, 2);
        assert_eq!(lab.get(0, 2), 1);
        assert_eq!(lab.get(3, 0), 2);
        assert_eq!(lab.get(4, 1), 2);
    }

    #[test]
    fn u_shape_merges() {
        let m = Image::from_vec(3, 2, vec![true, false, true, true, true, true]);
        assert_eq!(label8(&m).1, 1);
    }

    #[test]
    fn holes_are_filled() {
        let mut m = Mask::filled(5, 5, false);
        for i in 0..5 {
            m.set(i, 0, true);
            m.set(i, 4, true);
            m.set(0, i, true);
            m.set(4, i, true);
        }
        assert!(fill_holes(&m).get(2, 2));
        let p = perimeter(&fill_holes(&m));
        assert!(p.get(0, 2) && !p.get(2, 2));
    }
}
