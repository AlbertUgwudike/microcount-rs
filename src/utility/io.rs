use itertools::izip;
use std::{io, ops::Div};

use crate::{
    model::transformation::Direction,
    utility::{
        imops::{array2buff, array2rgb_buff, matrix_vec_to_volume, stack_rgb},
        types::{Matrix, Volume},
    },
};

use eframe::egui::{self};
use itertools::Itertools;
use ndarray::prelude::*;
use ome_bioformats_rs::{
    common::Loc,
    format_in::{tiff_reader::TiffReader, FormatReader},
};

pub fn read_tiff_region(
    file_name: &str,
    (x, y): (u64, u64),
    (h, w): (u64, u64),
    df: u64,
) -> io::Result<Vec<Matrix<u16>>> {
    let mut tr = TiffReader::new(file_name.into())?;
    let md = tr.metadata();

    let mut pxs_vec = vec![];

    if md.series_count() > 1 {
        for s in 0..md.series_count() {
            let origin = Loc::new(x, y, 0, 0, 0, s as u64, h, w);
            pxs_vec.push(tr.open_pixels(origin, df)?);
        }
    } else {
        for c in 0..md.dimensions(0).unwrap().c {
            let origin = Loc::new(x, y, 0, c as u64, 0, 0, h, w);
            pxs_vec.push(tr.open_pixels(origin, df)?);
        }
    }

    Ok(pxs_vec
        .iter()
        .map(|pcx| Array2::from_shape_vec((h as usize, w as usize), pcx.to_u16vec()).unwrap())
        .collect())
}

pub async fn read_tiff_region_as(
    file_name: &str,
    (x, y): (u64, u64),
    (h, w): (u64, u64),
    df: u64,
) -> io::Result<Vec<Vec<u16>>> {
    let mut tr = TiffReader::new(file_name.into())?;
    let md = tr.metadata();

    let mut pxs_vec = vec![];

    if md.series_count() > 1 {
        for s in 0..md.series_count() {
            let loc = Loc::new(x, y, 0, 0, 0, s as u64, h, w);
            let mut chunky_reader = tr.open_pixels_chunky(loc, df)?;

            let mut data = chunky_reader.step()?;
            while data.is_none() {
                data = chunky_reader.step()?;
                tokio::task::yield_now().await;
            }

            pxs_vec.push(data.unwrap().to_u16vec())
        }
    } else {
        for c in 0..md.dimensions(0).unwrap().c {
            let loc = Loc::new(x, y, 0, c as u64, 0, 0, h, w);
            let mut chunky_reader = tr.open_pixels_chunky(loc, df)?;

            let mut data = chunky_reader.step()?;
            while data.is_none() {
                data = chunky_reader.step()?;
                tokio::task::yield_now().await;
            }

            pxs_vec.push(data.unwrap().to_u16vec())
        }
    }

    if pxs_vec.len() < 3 {
        println!("Not RGB!, Channel Count: {:?}", pxs_vec.len());
        for _ in 0..(3 - pxs_vec.len()) {
            pxs_vec.push(vec![0; pxs_vec[0].len()])
        }
    }

    Ok(pxs_vec)

    // let out: Vec<u16> = izip!(
    //     pxs_vec[0].to_vec(),
    //     pxs_vec[1].to_vec(),
    //     pxs_vec[2].to_vec()
    // )
    // .map(|(a, b, c)| [a, b, c])
    // .flatten()
    // .collect();
    // // .chunks(h.div(df) as usize * w.div_ceil(df) as usize);

    // println!(
    //     "Pixels: {:?}",
    //     (h.div(df) as usize, w.div_ceil(df) as usize)
    // );
    // println!("Pixels: {:?}", h.div(df) as usize * w.div_ceil(df) as usize);
    // println!(
    //     "Pixels: {:?}",
    //     pxs_vec.iter().map(|a| a.len()).collect::<Vec<usize>>()
    // );

    // // let mut out = vec![];

    // // for arr in &flat {
    // //     out.push(arr.collect());
    // // }

    // println!("Flat: {:?}", out.len());

    // Ok(out)
}

pub async fn load_image_from_path(
    path: String,
    origin: (u64, u64),
    (h, w): (u64, u64),
    (ih, iw): (u64, u64),
    df: u64,
    direction: &Direction,
) -> io::Result<Volume<u8>> {
    let t_origin = transform_origin(origin, (h, w), (ih, iw), direction);

    let (t_h, t_w) = match direction {
        Direction::North | Direction::South => (h, w),
        Direction::East | Direction::West => (w, h),
    };

    let pixels: Vec<Vec<u8>> = read_tiff_region_as(&path, (t_origin.1, t_origin.0), (t_h, t_w), df)
        .await?
        .into_iter()
        // .flatten()
        .map(|v| {
            v.iter()
                .map(|p| std::cmp::min(255, *p) as u8)
                .collect::<Vec<u8>>()
        })
        .collect();

    // let chunks = pixels
    //     .chunks_exact(3)
    //     .into_iter()
    //     .map(|v| [v[0], v[1], v[2]])
    //     .collect();

    let (w, h) = (w.div_ceil(df) as usize, h.div(df) as usize);
    let pixels: Vec<Matrix<u8>> = pixels
        .iter()
        .map(|v| {
            let im = rotate_image(&v, (t_h, t_w), direction);
            Matrix::from_shape_vec([h, w], im).unwrap()
        })
        .collect();

    Ok(matrix_vec_to_volume(&pixels).unwrap())

    // let (w, h) = (w.div_ceil(df) as usize, h.div(df) as usize);

    // Ok(egui::ColorImage::from_rgb([w, h], &pixels))
}

fn transform_origin(
    (r, c): (u64, u64),
    (h, w): (u64, u64),
    (ih, iw): (u64, u64),
    direction: &Direction, // feature of input
) -> (u64, u64) {
    let ori = match direction {
        Direction::North => (r, c),
        Direction::East => (r, c + w - 1),
        Direction::South => (r + h - 1, c + w - 1),
        Direction::West => (r + h - 1, c),
    };
    rotate_coord(ori, (ih, iw), &direction.reciprocal())
}

fn rotate_image<'a>(img: &Vec<u8>, (h, w): (u64, u64), direction: &Direction) -> Vec<u8> {
    let mut out = vec![0; (h * w) as usize];

    let m = match direction {
        Direction::North | Direction::South => w,
        Direction::East | Direction::West => h,
    };

    for r in 0..h {
        for c in 0..w {
            let (i, j) = rotate_coord((r, c), (h, w), direction);
            let dest = (i * m + j) as usize;
            let orig = (r * w + c) as usize;
            out[dest] = img[orig];
        }
    }

    out
}

fn rotate_coord((r, c): (u64, u64), (h, w): (u64, u64), direction: &Direction) -> (u64, u64) {
    match direction {
        Direction::North => (r, c),
        Direction::East => (c, h - r - 1),
        Direction::South => (h - r - 1, w - c - 1),
        Direction::West => (w - c - 1, r),
    }
}

pub fn save_as_luma8(arr: &Matrix<u32>, file_name: &str) {
    let img = array2buff(arr.map(|a| std::cmp::min(*a, 255) as u8));
    let luma = image::DynamicImage::ImageLuma8(img);
    let _ = luma.save(file_name);
}

pub fn save_as_luma16(arr: &Matrix<u16>, file_name: &str) {
    let img = array2buff(arr.clone());
    let luma = image::DynamicImage::ImageLuma16(img);
    let _ = luma.save(file_name);
}

pub fn save_as_binary(arr: &Matrix<bool>, file_name: &str) {
    let img = array2buff(arr.map(|a| if *a { 255 } else { 0 }));
    let luma = image::DynamicImage::ImageLuma8(img);
    let _ = luma.save(file_name);
}

pub fn save_as_rgb_bool(a: &Matrix<bool>, b: &Matrix<bool>, c: &Matrix<bool>, file_name: &str) {
    let composite = stack_rgb(a, b, c);
    let composite_img = array2rgb_buff(composite.map(|a| if *a { 255u8 } else { 0u8 }));
    let composite_rgb = image::DynamicImage::ImageRgb8(composite_img);
    let _ = composite_rgb.save(file_name);
}
