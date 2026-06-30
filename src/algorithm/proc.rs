use core::f32;
use std::f32::consts::PI;
use std::ops::Div;

use crate::algorithm::helpers::conv;
use crate::utility::imops::{array2buff, buff2array, vec2buff};
use crate::utility::io::{save_as_luma16, save_as_luma8};
use crate::utility::types::{Matrix, Volume};

use image::imageops::FilterType;
use image::{ImageBuffer, Luma};
use imageproc::geometric_transformations::{
    rotate, warp, warp_into, warp_with, Interpolation, Projection,
};
use imageproc::image::imageops::resize;
use nalgebra::{dmatrix, matrix, stack};
use ndarray::prelude::*;

pub fn pacefilt(img: &Matrix<f64>, l: usize, sigma: f64) -> Matrix<f64> {
    let gaussian = Array2::from_shape_fn((l, l), |(i, j)| {
        let r = i as f64 - l as f64 / 2.0;
        let c = j as f64 - l as f64 / 2.0;
        let exp = -(r.powf(2.0) + c.powf(2.0)) / (2.0 * sigma.powf(2.0));
        exp.exp()
    });

    let sobel_x = array![[-1.0, -2.0, -1.0], [0.0, 0.0, 0.0], [1.0, 2.0, 1.0]];
    let sobel_y = array![[-1.0, 0.0, 1.0], [-2.0, 0.0, 2.0], [-1.0, 0.0, 1.0]];

    let gx = conv(&gaussian, &sobel_x);
    let gy = conv(&gaussian, &sobel_y);

    let gxx = conv(&gx, &sobel_x);
    let gxy = conv(&gx, &sobel_y);
    let gyy = conv(&gy, &sobel_y);

    let rxx = conv(&img, &gxx);
    let rxy = conv(&img, &gxy);
    let ryy = conv(&img, &gyy);

    let tmp = ((&rxx - &ryy).powf(2.0) + 4.0 * &rxy.powf(2.0)).sqrt();

    let mut eig1 = 0.125 * (&rxx + &ryy - &tmp).abs();
    let eig2 = 0.125 * (&rxx + &ryy + &tmp).abs();

    eig1.zip_mut_with(&eig2, |a, b| *a = if *a > *b { *a } else { *b });

    eig1
}

fn imadjust_buff(buff: &ImageBuffer<Luma<f32>, Vec<f32>>) -> ImageBuffer<Luma<f32>, Vec<f32>> {
    let mut arr = buff.clone().into_vec();
    arr.sort_by(|a, b| f32::total_cmp(a, b));

    let (w, h) = buff.dimensions();
    let n_pixels = arr.len();
    let lb = arr[n_pixels.div_ceil(20)];
    let ub = arr[19 * n_pixels.div_ceil(20)];
    let interval = ub - lb;

    let barr = buff
        .iter()
        .map(|&a| ((a - lb) / interval).clamp(0.0, 1.0))
        .collect();

    return vec2buff(barr, h as usize, w as usize);
}

pub fn iter_align(
    moving: &ImageBuffer<Luma<f32>, Vec<f32>>,
    fixed: &ImageBuffer<Luma<f32>, Vec<f32>>,
) -> [f32; 9] {
    println!("Aligning ... ");

    // let moving = array2buff(moving.t().to_owned());
    // let fixed = array2buff(fixed.to_owned());

    let (moving, fixed) = (imadjust_buff(&moving), imadjust_buff(&fixed));

    let (w, h) = fixed.dimensions();
    let r_moving = resize(&moving, w, h, FilterType::Gaussian);

    let mut t = std::array::from_fn(|i| if [0, 4, 8].contains(&i) { 1.0 } else { 0.0 });
    let mut curr_fitness = mutual_information(&r_moving, &fixed, false);

    for i in 0..2000 {
        let new_t = mutate(&t); // <--- mutate should be function of interation
        let proj = Projection::from_matrix(new_t).unwrap();
        let new_moving = warp(&r_moving, &proj, Interpolation::Nearest, Luma([0.0]));
        let new_fitness = mutual_information(&new_moving, &fixed, true);
        if new_fitness < 0.000001 {
            continue;
        } else if new_fitness > curr_fitness {
            t = new_t;
            curr_fitness = new_fitness;
            println!("{:?}", curr_fitness);
        }
    }

    println!("tform == {:?}", t);

    let proj = Projection::from_matrix(t).unwrap();
    let new_moving = warp(&r_moving, &proj, Interpolation::Nearest, Luma([0.0]));

    let (w, h) = new_moving.dimensions();

    let out_img = Array2::from_shape_vec((h as usize, w as usize), new_moving.into_vec()).unwrap();
    let out_his = Array2::from_shape_vec((h as usize, w as usize), fixed.into_vec()).unwrap();

    save_as_luma8(&out_img.map(|a| (255.0 * a) as u32), "./out.tiff");
    save_as_luma8(&out_his.map(|a| (255.0 * a) as u32), "./out1.tiff");

    return t;
}

fn mutate(chr: &[f32; 9]) -> [f32; 9] {
    std::array::from_fn(|j| {
        if j < 6 {
            chr[j] + (rand::random::<f32>() * 2.0 - 1.0) * 0.001
        } else {
            chr[j]
        }
    })
}

fn mutual_information(
    fixed: &ImageBuffer<Luma<f32>, Vec<f32>>,
    moving: &ImageBuffer<Luma<f32>, Vec<f32>>,
    show: bool,
) -> f32 {
    let n_bins = 100.0;

    let mut joint = Array2::from_elem((n_bins as usize, n_bins as usize), 0f32);

    moving
        .clone()
        .into_vec()
        .iter()
        .map(|a| a.to_owned())
        .zip(fixed.clone().into_vec())
        .for_each(|(a, b)| {
            let i = (n_bins * a).clamp(0.0, n_bins - 1.0) as usize;
            let j = (n_bins * b).clamp(0.0, n_bins - 1.0) as usize;
            joint[(i, j)] += 1.0;
        });

    let pxy = &joint / joint.sum();
    let px = pxy.sum_axis(Axis(0));
    let py = pxy.sum_axis(Axis(1));
    let px_py = Array2::from_shape_fn(joint.dim(), |(i, j)| px[j] * py[i]);

    if show {
        save_as_luma16(&joint.map(|a| (a * 1.0) as u16), "./out2.tiff");
    }

    pxy.indexed_iter().fold(0.0, |acc, (idx, &a)| {
        if a == 0.0 || px_py[idx] == 0.0 {
            acc
        } else {
            acc + a * (a / px_py[idx]).ln()
        }
    })
}

pub fn register_control_points(
    moving: [(f32, f32); 6],
    fixed: [(f32, f32); 6],
) -> Option<[f32; 6]> {
    let comp = |(a, b): (f32, f32)| matrix![ a, b, 1.0, 0.0, 0.0, 0.0; 0.0, 0.0, 0.0, a, b, 1.0];

    let m = stack![
        comp(moving[0]);
        comp(moving[1]);
        comp(moving[2]);
        comp(moving[3]);
        comp(moving[4]);
        comp(moving[5])
    ];

    let z = matrix![
        fixed[0].0; fixed[0].1;
        fixed[1].0; fixed[1].1;
        fixed[2].0; fixed[2].1;
        fixed[3].0; fixed[3].1;
        fixed[4].0; fixed[4].1;
        fixed[5].0; fixed[5].1;
    ];

    let svd = m.svd(true, true);
    let arr = svd.solve(&z, 1e-6).ok()?;

    // m.qr().solve(&z).map(|arr| {
    let mut out = [0.0; 6];
    for i in 0..6 {
        out[i] = arr[i];
    }
    Some(out)
    // })
}

pub fn warp_image(
    atlas_slice: Matrix<u16>,
    (h, w): (usize, usize),
    theta: [f32; 6],
) -> Matrix<u16> {
    let mut t = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    for i in 0..6 {
        t[i] = theta[i]
    }

    let proj = Projection::from_matrix(t).unwrap();
    let r_moving = array2buff(atlas_slice);
    // let new_moving = warp(&r_moving, &proj, Interpolation::Nearest, Luma([0]));

    let mut new_moving = ImageBuffer::from_pixel(w as u32, h as u32, Luma([0]));
    warp_into(
        &r_moving,
        &proj,
        Interpolation::Nearest,
        Luma([0]),
        &mut new_moving,
    );

    buff2array(new_moving)
}

pub fn add_overlay(img: &mut Volume<u8>, overlay: Matrix<u8>) {
    for (idx, v) in overlay.indexed_iter() {
        if *v != 0 {
            img[(0, idx.0, idx.1)] = 255;
            img[(1, idx.0, idx.1)] = 255;
            img[(2, idx.0, idx.1)] = 255;
        }
    }
}
