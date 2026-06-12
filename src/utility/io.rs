use itertools::izip;
use std::{io, ops::Div};

use crate::utility::{
    imops::{array2buff, array2rgb_buff, stack_rgb},
    types::Matrix,
};

use eframe::egui;
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

    let flat = izip!(
        pxs_vec[0].to_vec(),
        pxs_vec[1].to_vec(),
        pxs_vec[2].to_vec()
    )
    .map(|(a, b, c)| [a, b, c])
    .flatten()
    .chunks(h.div(df) as usize * w.div_ceil(df) as usize);

    println!(
        "Pixels: {:?}",
        (h.div(df) as usize, w.div_ceil(df) as usize)
    );
    println!("Pixels: {:?}", h.div(df) as usize * w.div_ceil(df) as usize);
    println!(
        "Pixels: {:?}",
        pxs_vec.iter().map(|a| a.len()).collect::<Vec<usize>>()
    );

    let mut out = vec![];

    for arr in &flat {
        out.push(arr.collect());
    }

    Ok(out)
}

pub async fn egui_image_from_path(
    path: String,
    origin: (u64, u64),
    (h, w): (u64, u64),
    df: u64,
) -> io::Result<egui::ColorImage> {
    let pixels: Vec<u8> = read_tiff_region_as(&path, origin, (h, w), df)
        .await?
        .into_iter()
        .flatten()
        .map(|p| std::cmp::min(255, p) as u8)
        .collect();

    Ok(egui::ColorImage::from_rgb(
        [w.div_ceil(df) as usize, h.div(df) as usize],
        &pixels,
    ))
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
