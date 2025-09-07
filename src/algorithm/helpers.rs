use std::f64::consts::PI;

use crate::utility::types::{Matrix, Pnt};

use itertools::Itertools;
use ndarray_conv::{ConvExt, ConvMode, PaddingMode};

pub fn for_each_neighbour(pt: Pnt, sh: Pnt, f: &mut dyn FnMut(Pnt, usize)) {
    for k in 0..8 {
        let (ii, jj) = (pt.0 as i32 + DIRS[k].0, pt.1 as i32 + DIRS[k].1);
        if ii >= 0 && jj >= 0 && ii < sh.0 as i32 && jj < sh.1 as i32 {
            (*f)((ii as usize, jj as usize), k);
        }
    }
}

const DIRS: [(i32, i32); 8] = [
    (-1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
];

pub fn fit_linear_model(x: &Vec<f64>, y: &Vec<f64>) -> (f64, f64) {
    assert!(x.len() == y.len());
    let n = x.len() as f64;
    let sx: f64 = x.iter().sum();
    let sxx: f64 = x.iter().map(|&a| a * a).sum();
    let sy: f64 = y.iter().sum();
    let sxy: f64 = x.iter().zip(y).map(|(&a, &b)| a * b).sum();
    let theta = (n * sxy - sx * sy) / (n * sxx - sx * sx);
    let alpha = (sy * sxx - sx * sxy) / (n * sxx - sx * sx);
    return (theta, alpha);
}

pub fn conv(arr: &Matrix<f64>, ker: &Matrix<f64>) -> Matrix<f64> {
    arr.conv(ker, ConvMode::Same, PaddingMode::Reflect)
        .expect("FAILURE")
}

pub fn scholl(origin: Pnt, pts: Vec<Pnt>) -> f64 {
    let (x, y) = pts
        .iter()
        .map(|&a| euc_sq(a, origin))
        .filter(|&a| a != 0)
        .sorted()
        .chunk_by(|&a| a)
        .into_iter()
        .map(|(key, ds)| {
            let r = (key as f64).sqrt();
            (r, -(ds.count() as f64 / (PI * r * r)).log10())
        })
        .unzip();

    return fit_linear_model(&x, &y).0;
}

pub fn euc_sq((p, q): Pnt, (r, s): Pnt) -> usize {
    let (a, b, c, d) = (p as i32, q as i32, r as i32, s as i32);
    ((c - a) * (c - a) + (d - b) * (d - b)) as usize
}

pub fn nearest(pt: Pnt, pts: &Vec<Pnt>) -> Option<&Pnt> {
    pts.iter().reduce(|acc, a| {
        if euc_sq(*a, pt) < euc_sq(*acc, pt) {
            a
        } else {
            acc
        }
    })
}
