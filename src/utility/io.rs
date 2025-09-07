use crate::utility::types::{Matrix, Pnt, Volume, ROI};

use image::{open, ImageBuffer, Luma, Primitive, Rgb};
use ndarray::prelude::*;
use tiff::decoder::{ifd, Decoder, DecodingResult};

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

pub fn volume_to_matrix_vec<T: Clone>(
    vol: &Volume<T>,
    order: (usize, usize, usize),
) -> Vec<Matrix<T>> {
    let (h, w, _) = vol.dim();
    vol.clone()
        .permuted_axes(order)
        .outer_iter()
        .map(|a| a.into_owned().into_shape_clone((h, w)).expect("yum"))
        .collect()
}

pub fn array2buff<T: Copy + Primitive>(arr: Matrix<T>) -> ImageBuffer<Luma<T>, Vec<T>> {
    let (h, w) = arr.dim();
    let (fast, _) = arr.into_raw_vec_and_offset();
    let mut buff = image::ImageBuffer::new(w as u32, h as u32);
    buff.copy_from_slice(&fast);
    buff
}

pub fn array2rgb_buff(arr1: Volume<u8>) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let arr = arr1.permuted_axes((2, 1, 0));
    let (h, w, _) = arr.dim();

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

pub fn read_tiff_region(file_name: &str, bbox: ROI) -> Vec<Matrix<u16>> {
    let h = std::fs::File::open(file_name).unwrap();
    let mut tr = Decoder::new(h).unwrap();

    let tinfo = tiff_info(&mut tr);

    match tinfo {
        TiffType::SinglePanel(bps) => read_as_single_panel(&mut tr, bbox),
        TiffType::MultiPanel => read_as_multi_panel(&mut tr, bbox),
    }
}

fn read_as_single_panel(tr: &mut Decoder<std::fs::File>, (r, c, h, w): ROI) -> Vec<Matrix<u16>> {
    let (cw, ch) = tr.chunk_dimensions();

    let start_idx = r as u32 / ch;
    let end_idx = (r + h) as u32 / ch;

    println!("{:?}", (ch, cw));

    let img = if start_idx == end_idx {
        read_chunk(tr, start_idx)
            .chunks_exact(cw as usize)
            .map(|a| &a[c..c + w])
            .collect::<Vec<&[u16]>>()
            .concat()
    } else {
        (start_idx..end_idx + 1)
            .flat_map(|idx| {
                let ck = read_chunk(tr, idx);
                let lower_idx = std::cmp::max((idx * ch) as usize, r) - (idx * ch) as usize;
                let upper_idx =
                    std::cmp::min(((idx + 1) * ch) as usize, r + h) - (idx * ch) as usize;
                ck.chunks_exact(cw as usize * 3)
                    .skip(lower_idx)
                    .take(upper_idx - lower_idx)
                    .map(|a| &a[c * 3..(c + w) * 3])
                    .collect::<Vec<&[u16]>>()
                    .concat()
            })
            .collect()
    };

    let im_vol = Array3::from_shape_vec((h, w, 3), img).expect("Array failure");
    volume_to_matrix_vec(&im_vol, (2, 0, 1))
}

fn read_as_multi_panel(tr: &mut Decoder<std::fs::File>, bbox: ROI) -> Vec<Matrix<u16>> {
    (0..3)
        .map(|a| {
            if a > 0 {
                tr.next_image();
            }
            read_panel(tr, bbox)
        })
        .collect()
}

fn read_panel(tr: &mut Decoder<std::fs::File>, (r, c, h, w): ROI) -> Matrix<u16> {
    let (cw, ch) = tr.chunk_dimensions();

    let start_idx = r as u32 / ch;
    let end_idx = (r + h) as u32 / ch;

    let img = if start_idx == end_idx {
        read_chunk(tr, start_idx)
            .chunks_exact(cw as usize)
            .map(|a| &a[c..c + w])
            .collect::<Vec<&[u16]>>()
            .concat()
    } else {
        (start_idx..end_idx + 1)
            .flat_map(|idx| {
                let ck = read_chunk(tr, idx);
                let row_idx = (idx * ch) as usize;
                let lower_idx = std::cmp::max((idx * ch) as usize, r) - row_idx;
                let upper_idx = std::cmp::min(((idx + 1) * ch) as usize, r + h) - row_idx;
                ck.chunks_exact(cw as usize)
                    .skip(lower_idx)
                    .take(upper_idx - lower_idx)
                    .map(|a| &a[c..(c + w)])
                    .collect::<Vec<&[u16]>>()
                    .concat()
            })
            .collect()
    };

    Array2::from_shape_vec((h, w), img).expect("Array failure")
}

#[derive(Debug)]
enum TiffType {
    MultiPanel,
    SinglePanel(u32),
}

fn tiff_info(tr: &mut Decoder<std::fs::File>) -> TiffType {
    let bps = tr
        .get_tag(tiff::tags::Tag::BitsPerSample)
        .expect("Get Tag failed");

    println!("{:?}", bps);

    match bps {
        ifd::Value::List(v) => TiffType::SinglePanel(v[0].clone().into_u32().unwrap()),
        _ => TiffType::MultiPanel,
    }
}

fn read_chunk(tr: &mut Decoder<std::fs::File>, idx: u32) -> Vec<u16> {
    let chunk = tr.read_chunk(idx).unwrap();
    match chunk {
        DecodingResult::U16(v) => v,
        DecodingResult::U8(v) => v.iter().map(|&a| a as u16).collect::<Vec<u16>>(),
        DecodingResult::U32(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::U64(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::F16(v) => vec![2, 3, 4],
        DecodingResult::F32(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::F64(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I8(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I16(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I32(v) => v.iter().map(|&a| a as u16).collect(),
        DecodingResult::I64(v) => v.iter().map(|&a| a as u16).collect(),
    }
}

pub fn save_as_luma8(arr: &Matrix<u32>, file_name: &str) {
    let img = array2buff(arr.map(|a| std::cmp::min(*a, 255) as u8));
    let luma = image::DynamicImage::ImageLuma8(img);
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
