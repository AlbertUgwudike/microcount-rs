use crate::algorithm::helpers::for_each_neighbour;
use crate::utility::types::{Matrix, Pnt};

use geo::{concave_hull, ConcaveHull, ConvexHull, GeodesicArea};
use geo::{point, Area, MultiPoint, Point};
use ndarray::prelude::*;
use std::collections::VecDeque;

pub fn regions2label(regions: &Vec<Vec<Pnt>>, shape: Pnt) -> Matrix<u32> {
    let mut out = Array2::from_elem(shape, 0);

    for (i, region) in regions.iter().enumerate() {
        region.iter().for_each(|&pt| out[pt] = (i + 1) as u32)
    }

    return out;
}

pub fn label2regions(label: &Matrix<u32>) -> Vec<Vec<Pnt>> {
    let n_regions = label.iter().max().unwrap();

    let mut out = Vec::with_capacity(*n_regions as usize);

    for _ in 0..*n_regions {
        out.push(vec![]);
    }

    label.indexed_iter().for_each(|(pt, &a)| {
        if a != 0 {
            out[(a - 1) as usize].push(pt);
        }
    });

    return out;
}

pub fn centroids(region: &Vec<Pnt>) -> Pnt {
    let n_pixels = region.len();
    let acc_f = |acc: Pnt, a: &Pnt| (acc.0 + a.0, acc.1 + a.1);
    let sum_pt = region.iter().fold((0, 0), acc_f);
    (sum_pt.0 / n_pixels, sum_pt.1 / n_pixels)
}

pub fn rotunditiy(region: &Vec<Pnt>) -> f64 {
    let pts = region
        .iter()
        .map(|p| point! { x: p.0 as f64, y: p.1 as f64 })
        .collect::<Vec<Point<f64>>>();
    let concave = pts.len() as f64;
    let convex = MultiPoint::from(pts).convex_hull();
    println!("{:?}", (concave, convex.exterior()));
    concave / convex.unsigned_area()
}

pub fn floodfill(img: &Matrix<bool>, markers: &Vec<Pnt>) -> Vec<Vec<Pnt>> {
    let shape = img.dim();
    let n_cells = markers.len();

    let mut out = Vec::with_capacity(n_cells);
    let mut visited = Array2::from_elem(shape, false);
    let mut frontiers = Vec::with_capacity(n_cells);

    for i in 0..n_cells {
        let marker_pt = markers[i];
        visited[marker_pt] = true;
        out.push(vec![marker_pt]);
        frontiers.push(VecDeque::from([marker_pt]));
    }

    while !frontiers.iter().all(|a| a.is_empty()) {
        for (label_idx, frontier) in frontiers.iter_mut().enumerate() {
            for _ in 0..frontier.len() {
                let pt = frontier.pop_front().unwrap();
                for_each_neighbour(pt, shape, &mut |new_pt, _| {
                    if img[new_pt] && !visited[new_pt] {
                        frontier.push_back(new_pt);
                        visited[new_pt] = true;
                        out[label_idx].push(new_pt);
                    }
                });
            }
        }
    }

    return out;
}
