use crate::utility::{
    imops::{array2buff, array2rgb_buff, stack_rgb, volume_to_matrix_vec},
    types::{Matrix, TiffInfo, TiffType, ROI},
};

use eframe::egui;
use image::open;
use itertools::Itertools;
use ndarray::prelude::*;
use tiff::{
    decoder::{Decoder, DecodingResult},
    TiffError,
};

pub fn read_as_channels(file_name: &str) -> Vec<Matrix<f64>> {
    let dyn_img = open(file_name).expect("ERRIR");

    let h = dyn_img.height() as usize;
    let w = dyn_img.width() as usize;

    let buff = dyn_img.as_rgb16().unwrap().to_owned().into_vec();
    let buff_f = buff.iter().map(|a| *a as f64).collect();
    let volume = Array3::from_shape_vec((h, w, 3), buff_f).expect("Fail");

    volume
        .permuted_axes((2, 1, 0))
        .outer_iter()
        .map(|a| a.into_owned().into_shape_clone((h, w)).expect("yum"))
        .collect()
}

pub fn read_tiff_region(
    file_name: &str,
    bbox: ROI,
    df: usize,
) -> Result<Vec<Matrix<u16>>, TiffError> {
    let h = std::fs::File::open(file_name)?;
    let mut tr = Decoder::new(h).unwrap();
    tiff_type(&mut tr).and_then(|tp| read_as_multi_panel(&mut tr, &tp, bbox, df))
}

pub fn egui_image_from_path(
    path: &str,
    bbox: ROI,
    df: usize,
) -> Result<egui::ColorImage, TiffError> {
    let image = read_tiff_region(path, bbox, df)?;
    let image: Vec<Matrix<u8>> = image.iter().map(|i| i.map(|a| *a as u8)).collect();
    let rgb = stack_rgb(&image[0], &image[1], &image[2]);
    let im = array2rgb_buff(rgb);
    let (h, w) = image[0].dim();
    let pixels = im.as_flat_samples();
    Ok(egui::ColorImage::from_rgb([w, h], pixels.as_slice()))
}

fn read_as_multi_panel(
    tr: &mut Decoder<std::fs::File>,
    tp: &TiffType,
    bbox: ROI,
    df: usize,
) -> Result<Vec<Matrix<u16>>, TiffError> {
    let (_, _, w, h) = bbox;
    match tp {
        TiffType::SinglePanel { bps, cc } => {
            let img = get_pixels(tr, tp, bbox, df)?;
            let im_vol = Array3::from_shape_vec((w.div_ceil(df), h.div_ceil(df), *cc), img)
                .expect("Array failure");
            Ok(volume_to_matrix_vec(&im_vol, (2, 0, 1)))
        }
        TiffType::MultiPanel { bps, spp, cc } => Ok((0..*cc)
            .map(|a| {
                if a > 0 {
                    let _ = tr.next_image();
                }
                let img = get_pixels(tr, tp, bbox, df).unwrap();
                Array2::from_shape_vec((w.div_ceil(df), h.div_ceil(df)), img)
                    .expect("Array failure")
            })
            .collect()),
    }
}

fn get_pixels(
    tr: &mut Decoder<std::fs::File>,
    tp: &TiffType,
    (r, c, h, w): ROI,
    df: usize,
) -> Result<Vec<u16>, TiffError> {
    let (cw, ch) = tr.chunk_dimensions();
    let spp = match tp {
        TiffType::SinglePanel { bps, cc } => cc,
        TiffType::MultiPanel { bps, spp, cc } => spp,
    };

    let start_idx = r as u32 / ch;
    let end_idx = (r + h) as u32 / ch;

    if start_idx == end_idx {
        let chunk = read_chunk(tr, start_idx)?;
        Ok(chunk
            .chunks_exact(cw as usize)
            .map(|a| {
                let idx = c * spp..(c + w) * spp;
                a[idx].iter().step_by(2).cloned().collect_vec()
            })
            .collect::<Vec<Vec<u16>>>()
            .concat())
    } else {
        Ok((start_idx..end_idx)
            .flat_map(|idx| {
                let ck = read_chunk(tr, idx).unwrap();
                let row_idx = (idx * ch) as usize;
                let lower_idx = std::cmp::max((idx * ch) as usize, r) - row_idx;
                let upper_idx = std::cmp::min(((idx + 1) * ch) as usize, r + h) - row_idx;
                ck.chunks_exact(cw as usize * spp)
                    .skip(lower_idx)
                    .take(upper_idx - lower_idx)
                    .map(|a| &a[c * spp..(c + w) * spp])
                    .map(|a| {
                        a.chunks_exact(*spp)
                            .into_iter()
                            .step_by(df)
                            .collect::<Vec<&[u16]>>()
                            .concat()
                    })
                    .collect::<Vec<Vec<u16>>>()
            })
            .step_by(df)
            .collect::<Vec<Vec<u16>>>()
            .concat())
    }
}

fn image_count(tr: &mut Decoder<std::fs::File>) -> Result<usize, TiffError> {
    tr.seek_to_image(0)?;
    let mut num_images = 1;
    while tr.more_images() {
        num_images += 1;
        tr.next_image()?;
    }
    tr.seek_to_image(0)?;
    return Ok(num_images);
}

fn tiff_type(tr: &mut Decoder<std::fs::File>) -> Result<TiffType, TiffError> {
    let bps = tr.get_tag(tiff::tags::Tag::BitsPerSample)?.into_u16_vec()?[0] as usize;
    let spp = tr.get_tag(tiff::tags::Tag::SamplesPerPixel)?.into_u16()? as usize;
    let panel_count = image_count(tr)?;

    println!("Panel Count: {:?}", panel_count);
    println!("Samples: {:?}", spp);

    if panel_count == 1 {
        Ok(TiffType::SinglePanel { bps, cc: spp })
    } else {
        Ok(TiffType::MultiPanel {
            bps,
            spp,
            cc: panel_count,
        })
    }
}

pub fn tiff_info(file_name: &str) -> Result<TiffInfo, TiffError> {
    let h = std::fs::File::open(file_name).unwrap();
    let mut tr = Decoder::new(h).unwrap();
    let panel_type = tiff_type(&mut tr)?;
    let dims = tr.dimensions()?;
    Ok(TiffInfo {
        dimensions: (dims.0 as usize, dims.1 as usize),
        n_channels: match panel_type {
            TiffType::SinglePanel { bps, cc } => cc,
            TiffType::MultiPanel { bps, spp, cc } => cc,
        },
    })
}

fn read_chunk(tr: &mut Decoder<std::fs::File>, idx: u32) -> Result<Vec<u16>, TiffError> {
    tr.read_chunk(idx).map(|chunk| match chunk {
        DecodingResult::U16(v) => v,
        DecodingResult::U8(v) => v.iter().map(|&a| a as u16).collect::<Vec<u16>>(),
        DecodingResult::U32(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::U64(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::F16(v) => v.iter().map(|&a| a.to_f32() as u16).collect(),
        DecodingResult::F32(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::F64(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I8(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I16(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I32(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I64(v) => v.iter().map(|&a| a as u16).collect(),
    })
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
