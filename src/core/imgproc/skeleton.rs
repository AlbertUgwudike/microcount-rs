//! Skeletonisation and 3×3 neighbourhood pattern matching.
//!
//! Neighbour code bit k holds the neighbour at `OFFSETS[k]`: E, NE, N, NW, W, SW, S, SE
//! (image rows grow downwards, so N is y-1).

use crate::core::image::Mask;
use rayon::prelude::*;

pub const OFFSETS: [(isize, isize); 8] = [(1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1)];

#[inline]
pub fn neighbour_code(m: &[u8], w: usize, h: usize, x: usize, y: usize) -> u8 {
    let mut code = 0u8;
    for (k, (dx, dy)) in OFFSETS.iter().enumerate() {
        let (xx, yy) = (x as isize + dx, y as isize + dy);
        if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h && m[yy as usize * w + xx as usize] != 0 {
            code |= 1 << k;
        }
    }
    code
}

/// Lookup tables over 8-neighbour codes for Lee's thinning, specialised to a single
/// slice: neighbour count, "simple point" (foreground neighbours form exactly one
/// 8-connected component) and Euler invariance under deletion (8-connectivity Euler
/// number from Gray's bit quads, which is what the 3-D test reduces to for a plane).
fn lee_luts() -> ([u8; 256], [bool; 256], [bool; 256]) {
    let mut count = [0u8; 256];
    let mut simple = [false; 256];
    let mut euler = [false; 256];
    for code in 0..256usize {
        let mut g = [[0u8; 3]; 3];
        for (k, (dx, dy)) in OFFSETS.iter().enumerate() {
            g[(1 + dy) as usize][(1 + dx) as usize] = ((code >> k) & 1) as u8;
        }
        count[code] = (code as u8).count_ones() as u8;
        // Components of the ring under 8-adjacency (centre excluded).
        let mut seen = [[false; 3]; 3];
        let mut comps = 0;
        for sy in 0..3 {
            for sx in 0..3 {
                if (sy, sx) == (1, 1) || g[sy][sx] == 0 || seen[sy][sx] {
                    continue;
                }
                comps += 1;
                let mut stack = vec![(sy, sx)];
                seen[sy][sx] = true;
                while let Some((y, x)) = stack.pop() {
                    for ny in y.saturating_sub(1)..=(y + 1).min(2) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(2) {
                            if (ny, nx) != (1, 1) && g[ny][nx] == 1 && !seen[ny][nx] {
                                seen[ny][nx] = true;
                                stack.push((ny, nx));
                            }
                        }
                    }
                }
            }
        }
        simple[code] = comps == 1;
        let quad = |g: &[[u8; 3]; 3], qy: usize, qx: usize| -> i32 {
            let (a, b, c, d) = (g[qy][qx], g[qy][qx + 1], g[qy + 1][qx], g[qy + 1][qx + 1]);
            match a + b + c + d {
                1 => 1,
                3 => -1,
                2 if a == d => -2,
                _ => 0,
            }
        };
        let mut with = g;
        with[1][1] = 1;
        let mut without = g;
        without[1][1] = 0;
        let delta: i32 = [(0, 0), (0, 1), (1, 0), (1, 1)]
            .iter()
            .map(|&(qy, qx)| quad(&without, qy, qx) - quad(&with, qy, qx))
            .sum();
        euler[code] = delta == 0;
    }
    (count, simple, euler)
}

/// `bwskel`: Lee, Kashyap & Chu (1994) medial-axis thinning — MATLAB pads the image to
/// 3-D and runs the ITK implementation — specialised to a single plane. Each round makes
/// six directional passes; for the two out-of-plane directions every pixel is a border
/// point. Candidates are deleted one at a time, re-checking that each is still simple.
/// MATLAB's first array dimension is the row, so ITK's "x" is our y: scanning is
/// column-major and the in-plane passes are x-1, x+1, y+1, y-1 in our coordinates.
/// Skeletons agree with `bwskel` in size and topology to within a few percent but not
/// pixel for pixel (thin structures may resolve to the other side).
pub fn skeletonize(mask: &Mask) -> Mask {
    let (w, h) = (mask.w, mask.h);
    let (count, simple, euler) = lee_luts();
    let mut img: Vec<u8> = mask.data.par_iter().map(|&b| b as u8).collect();
    let boundary = |img: &[u8], i: usize| -> bool {
        img[i] != 0 && neighbour_code(img, w, h, i % w, i / w) != 0xFF
    };
    let mut in_active = vec![false; w * h];
    let mut active: Vec<u32> = (0..w * h).into_par_iter().filter(|&i| boundary(&img, i)).map(|i| i as u32).collect();
    for &a in &active {
        in_active[a as usize] = true;
    }
    loop {
        let mut unchanged = 0;
        for border in 0..6 {
            active.par_sort_unstable_by_key(|&p| (p as usize % w) * h + p as usize / w);
            let cands: Vec<u32> = active
                .par_iter()
                .copied()
                .filter(|&p| {
                    let p = p as usize;
                    if img[p] == 0 {
                        return false;
                    }
                    let (x, y) = (p % w, p / w);
                    let is_border = match border {
                        0 => x == 0 || img[p - 1] == 0,
                        1 => x == w - 1 || img[p + 1] == 0,
                        2 => y == h - 1 || img[p + w] == 0,
                        3 => y == 0 || img[p - w] == 0,
                        _ => true,
                    };
                    if !is_border {
                        return false;
                    }
                    let code = neighbour_code(&img, w, h, x, y) as usize;
                    count[code] != 1 && euler[code] && simple[code]
                })
                .collect();
            let mut changed = false;
            for &p in &cands {
                let p = p as usize;
                // Sequential re-check: earlier deletions may have made it non-simple.
                if !simple[neighbour_code(&img, w, h, p % w, p / w) as usize] {
                    continue;
                }
                img[p] = 0;
                changed = true;
                in_active[p] = false;
                let (x, y) = ((p % w) as isize, (p / w) as isize);
                for (dx, dy) in OFFSETS {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                        continue;
                    }
                    let q = yy as usize * w + xx as usize;
                    if img[q] != 0 && !in_active[q] {
                        in_active[q] = true;
                        active.push(q as u32);
                    }
                }
            }
            if changed {
                active.retain(|&p| img[p as usize] != 0);
            } else {
                unchanged += 1;
            }
        }
        if unchanged == 6 {
            break;
        }
    }
    Mask::from_vec(w, h, img.into_par_iter().map(|v| v != 0).collect())
}

/// The 3×3 branch-point filters of `branch_filters.m`, written row by row
/// (top row first). Entries are +1 (must be set) or -1 (must be clear).
fn branch_filters() -> Vec<[[i8; 3]; 3]> {
    let flip = |a: [[i8; 3]; 3]| [a[2], a[1], a[0]];
    let tr = |a: [[i8; 3]; 3]| {
        let mut t = [[0i8; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                t[i][j] = a[j][i];
            }
        }
        t
    };
    let arrow1 = [[-1, -1, 1], [1, 1, -1], [-1, 1, -1]];
    let hammer1 = [[-1, -1, -1], [1, 1, 1], [-1, 1, -1]];
    let lambda1 = [[1, -1, 1], [-1, 1, -1], [-1, -1, 1]];
    let yesod1 = [[1, -1, 1], [-1, 1, -1], [-1, 1, -1]];
    let plus = [[-1, 1, -1], [1, 1, 1], [-1, 1, -1]];
    let times = [[1, -1, 1], [-1, 1, -1], [1, -1, 1]];
    let hammer2 = flip(hammer1);
    let yesod2 = flip(yesod1);
    vec![
        arrow1,
        flip(arrow1),
        tr(arrow1),
        flip(tr(arrow1)),
        hammer1,
        hammer2,
        tr(hammer1),
        tr(hammer2),
        lambda1,
        flip(lambda1),
        tr(lambda1),
        flip(tr(lambda1)),
        yesod1,
        yesod2,
        tr(yesod1),
        tr(yesod2),
        plus,
        times,
    ]
}

/// Neighbour codes that `count_branches.m` flags as branch points. `conv2` rotates the
/// kernel by 180°, so the image offset (dx, dy) is compared with entry F(1-dy, 1-dx).
fn branch_lut() -> [bool; 256] {
    let filters = branch_filters();
    let mut lut = [false; 256];
    for code in 0..256usize {
        lut[code] = filters.iter().any(|f| {
            OFFSETS.iter().enumerate().all(|(k, &(dx, dy))| {
                let want = f[(1 - dy) as usize][(1 - dx) as usize];
                let set = (code >> k) & 1 == 1;
                if want > 0 {
                    set
                } else {
                    !set
                }
            })
        });
    }
    lut
}

/// Branch points of a skeleton (`count_branches.m`, the `detected` image).
pub fn branch_points(skel: &Mask) -> Mask {
    let lut = branch_lut();
    let (w, h) = (skel.w, skel.h);
    let img: Vec<u8> = skel.data.par_iter().map(|&b| b as u8).collect();
    let data: Vec<bool> = (0..w * h)
        .into_par_iter()
        .map(|i| img[i] != 0 && lut[neighbour_code(&img, w, h, i % w, i / w) as usize])
        .collect();
    Mask::from_vec(w, h, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thick_bar_thins_to_line() {
        let mut m = Mask::filled(30, 11, false);
        for y in 3..8 {
            for x in 3..27 {
                m.set(x, y, true);
            }
        }
        let s = skeletonize(&m);
        // Each interior column holds exactly one skeleton pixel.
        for x in 8..22 {
            let n = (0..11).filter(|&y| s.get(x, y)).count();
            assert_eq!(n, 1, "column {x}");
        }
    }

    #[test]
    fn plus_junction_is_detected() {
        let mut m = Mask::filled(9, 9, false);
        for i in 0..9 {
            m.set(4, i, true);
            m.set(i, 4, true);
        }
        let b = branch_points(&m);
        assert!(b.get(4, 4));
        assert_eq!(b.count(), 1);
    }
}
