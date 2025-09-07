use crate::algorithm::helpers::conv;
use crate::utility::types::Matrix;

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
