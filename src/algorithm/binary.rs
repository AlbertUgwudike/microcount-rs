use crate::algorithm::helpers::{conv, for_each_neighbour};
use crate::utility::types::{Matrix, Pnt};

use ndarray::prelude::*;
use std::collections::VecDeque;

pub fn conncomps(img: &Matrix<bool>) -> Vec<Vec<Pnt>> {
    let sh = img.dim();

    let mut visited = Array2::from_elem(sh, false);
    let mut out: Vec<Vec<Pnt>> = vec![];

    img.indexed_iter().for_each(|(pt, &v)| {
        if v && !visited[pt] {
            let cps = connected_pixels(img, pt, &mut visited);
            out.push(cps);
        }
    });

    return out;
}

fn connected_pixels(img: &Matrix<bool>, origin: Pnt, visited: &mut Matrix<bool>) -> Vec<Pnt> {
    let sh = img.dim();

    let mut pts = vec![origin];
    let mut out = vec![];

    while !pts.is_empty() {
        let pt = pts.pop().unwrap();
        out.push(pt);
        visited[pt] = true;
        for_each_neighbour(pt, sh, &mut |new_pt, _| {
            if img[new_pt] && !visited[new_pt] {
                pts.push(new_pt);
            }
        });
    }

    return out;
}

pub fn perimeter(img: &Matrix<u32>) -> Matrix<u32> {
    let sobel_x = array![[-1.0, -2.0, -1.0], [0.0, 0.0, 0.0], [1.0, 2.0, 1.0]];
    let sobel_y = array![[-1.0, 0.0, 1.0], [-2.0, 0.0, 2.0], [-1.0, 0.0, 1.0]];

    let mut fimg = img.map(|&a| a as f64);

    let grad_x = conv(&fimg, &sobel_x);
    let grad_y = conv(&fimg, &sobel_y);
    let grad = grad_x.pow2() + grad_y.pow2();

    fimg.zip_mut_with(&grad, |a, &b| {
        if b == 0.0 {
            *a = 0.0
        }
    });

    fimg.map(|&a| a as u32)
}

pub fn skel(img: &Matrix<bool>) -> Matrix<bool> {
    let max_iterations = 100;
    let sh = img.dim();
    let ordering = [(0, 2, 4, 2, 4, 6), (0, 2, 6, 0, 4, 6)];

    let mut out = img.clone();
    let mut labels = Array2::from_elem(sh, false);

    for i in 0..max_iterations {
        let mut change = false;

        for order in ordering {
            change = false;
            out.indexed_iter().for_each(|(pt, val)| {
                if *val {
                    labels[pt] = mark(&out, order, pt, sh);
                    change |= labels[pt];
                }
            });

            labels.indexed_iter_mut().for_each(|(pt, val)| {
                if *val {
                    out[pt] = false;
                    *val = false;
                }
            });
        }

        if !change {
            println!("Skel iterations = {:?}", i);
            break;
        }
    }

    return out;
}

fn mark(
    out: &Matrix<bool>,
    c_idx: (usize, usize, usize, usize, usize, usize),
    pt: Pnt,
    sh: Pnt,
) -> bool {
    let mut pxs = [0u8; 8];

    for_each_neighbour(pt, sh, &mut |new_pt, k| {
        pxs[k] = out[new_pt] as u8;
    });

    let f = |xs, &(x, y)| xs + ((pxs[x], pxs[y]) == (0, 1)) as u8;
    let a = PS.iter().fold(0, f);
    let b = pxs.iter().fold(0, |xs, x| xs + *x);
    let c = pxs[c_idx.0] * pxs[c_idx.1] * pxs[c_idx.2];
    let d = pxs[c_idx.3] * pxs[c_idx.4] * pxs[c_idx.5];

    b >= 2 && b <= 6 && a == 1 && c == 0 && d == 0
}

pub fn count_branches(img: &Matrix<bool>) -> Matrix<bool> {
    let shape = img.dim();
    let mut out = Array2::from_elem(shape, false);
    img.indexed_iter().for_each(|(pt, &val)| {
        if val {
            out[pt] = mark_branch(img, pt, shape)
        }
    });
    out
}

fn mark_branch(out: &Matrix<bool>, pt: Pnt, sh: Pnt) -> bool {
    let mut pxs = [0u8; 8];

    for_each_neighbour(pt, sh, &mut |new_pt, k| {
        pxs[k] = out[new_pt] as u8;
    });

    let f = |xs, &(x, y)| xs + ((pxs[x], pxs[y]) == (0, 1)) as u8;

    PS.iter().fold(0, f) >= 3
}

const PS: [Pnt; 8] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 0),
];

pub fn branch_length(img: &Matrix<u32>, markers: &Vec<Pnt>) -> (Matrix<usize>, Vec<Vec<usize>>) {
    let shape = img.dim();
    let n_cells = markers.len();

    let mut out = Array2::from_elem(shape, 0);
    let mut visited = Array2::from_elem(shape, false);
    let mut frontiers = Vec::with_capacity(n_cells);
    let mut histograms = Vec::with_capacity(n_cells);

    for i in 0..n_cells {
        let marker_pt = markers[i];
        visited[marker_pt] = true;
        out[marker_pt] = 1;
        frontiers.push(VecDeque::from([marker_pt]));
        histograms.push(vec![1]);
    }

    while !frontiers.iter().all(|a| a.is_empty()) {
        for (label_idx, frontier) in frontiers.iter_mut().enumerate() {
            for _ in 0..frontier.len() {
                let pt = frontier.pop_front().unwrap();

                for_each_neighbour(pt, shape, &mut |new_pt, _| {
                    if !(img[new_pt] as usize == label_idx + 1 && !visited[new_pt]) {
                        return;
                    }

                    let dist = out[pt] + 1;
                    let curr_max_man = histograms[label_idx].len();

                    if dist >= curr_max_man {
                        histograms[label_idx].push(1);
                    } else {
                        histograms[label_idx][dist] += 1;
                    }

                    frontier.push_back(new_pt);
                    visited[new_pt] = true;
                    out[new_pt] = dist;
                });
            }
        }
    }

    return (out, histograms);
}
