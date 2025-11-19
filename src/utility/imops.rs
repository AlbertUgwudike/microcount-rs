use crate::utility::types::{Matrix, Volume};
use eframe::egui::{self, ColorImage};
use image::{ImageBuffer, Luma, Primitive, Rgb};
use imageproc::definitions::Image;
use ndarray::{concatenate, prelude::*, Slice};

pub fn volume_to_matrix_vec<T: Clone>(
    vol: &Volume<T>,
    order: (usize, usize, usize),
) -> Vec<Matrix<T>> {
    // let (h, w, _) = vol.dim();
    vol.clone()
        .permuted_axes(order)
        .outer_iter()
        .map(|a| a.into_owned()) //.into_shape_clone((h, w)).expect("yum")
        .collect()
}

pub fn matrix_vec_to_volume<T: Clone>(matrixs: &Vec<Matrix<T>>) -> Option<Volume<T>> {
    matrixs
        .iter()
        .map(|m| m.clone().insert_axis(Axis(0)))
        .reduce(|acc, a| concatenate![Axis(0), acc, a])
}

pub fn array2buff<T: Copy + Primitive>(arr: Matrix<T>) -> ImageBuffer<Luma<T>, Vec<T>> {
    let (h, w) = arr.dim();
    let fast = arr.iter().map(|a| a.to_owned()).collect();
    vec2buff(fast, h, w)
}

pub fn vec2buff<T: Copy + Primitive>(
    fast: Vec<T>,
    h: usize,
    w: usize,
) -> ImageBuffer<Luma<T>, Vec<T>> {
    let mut buff = image::ImageBuffer::new(w as u32, h as u32);
    buff.copy_from_slice(&fast);
    buff
}

pub fn array2rgb_buff(arr1: Volume<u8>) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let arr = arr1.permuted_axes((2, 1, 0));
    let (w, h, _) = arr.dim();

    let buff = ImageBuffer::new(w as u32, h as u32);

    arr.rows()
        .into_iter()
        .enumerate()
        .fold(buff, |mut acc, (i, p)| {
            let px = Rgb([p[0], p[1], p[2]]);
            let (r, c) = ((i / h) as u32, (i % h) as u32);
            acc.put_pixel(r, c, px);
            acc
        })
}

pub fn stack_rgb<T: Clone>(red: &Matrix<T>, green: &Matrix<T>, blue: &Matrix<T>) -> Volume<T> {
    let mut red_ch = red.clone().insert_axis(Axis(0));
    let green_ch = green.clone().insert_axis(Axis(0));
    let blue_ch = blue.clone().insert_axis(Axis(0));

    red_ch
        .append(Axis(0), (&green_ch).into())
        .expect("Failure to expand");

    red_ch
        .append(Axis(0), (&blue_ch).into())
        .expect("Failure to expand");

    red_ch
}

pub fn get_slice<T: Clone>(vol: &Volume<T>, idx: isize, axis: usize) -> Matrix<T> {
    vol.slice_axis(Axis(axis), Slice::new(idx, Some(idx + 1), 1))
        .remove_axis(Axis(axis))
        .to_owned()
}

pub fn egui_image_from_mat(mat: Matrix<u16>) -> ColorImage {
    let im = array2buff(mat.map(|&p| std::cmp::min(p, 255) as u8));
    let (h, w) = mat.dim();
    let pixels = im.as_flat_samples();
    egui::ColorImage::from_gray([w, h], pixels.as_slice())
}
