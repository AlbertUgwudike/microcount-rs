//! Image import (`lof2tiff.m`, `bg_conv_img`, `bg_down_img`, `set_metadata`): convert a
//! microscopy file to a workspace TIFF with the `bioformats` crate, write the 1/20
//! downsampled registration image, and read the image metadata.
//!
//! Every format the `bioformats` crate can read can be imported; nothing here lists
//! formats.

use crate::core::imageio::{self, ImageFile};
use crate::core::imgproc::stats::imadjust_u16;
use crate::core::workspace::DOWNSAMPLE;
use crate::core::Cancel;
use anyhow::{anyhow, Context, Result};
use bioformats::convert::output_core;
use bioformats::pixel::{interleave, swap_endianness};
use bioformats::reader::Region;
use bioformats::{OmeMetadata, WriterOptions};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Metadata {
    pub size: [usize; 2],
    pub down_size: [usize; 2],
    pub channel_count: usize,
    pub pixel_dims: Option<[f64; 2]>,
}

/// Raw pixel data read, normalised and encoded per batch.
const BATCH_BYTES: usize = 128 << 20;

fn temp_path(dst: &Path) -> PathBuf {
    let name = dst.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    dst.with_file_name(format!(".partial_{name}"))
}

/// Converts one series of `src` to the TIFF `dst`, streaming chunks of each plane
/// through the reader and writer in parallel. Reports progress in percent and stops
/// promptly when cancelled.
pub fn convert_series(src: &Path, series: usize, dst: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<()> {
    let reader = bioformats::open(src).with_context(|| format!("opening {}", src.display()))?;
    let in_core = reader.series().get(series).ok_or_else(|| anyhow!("{} has no series {}", src.display(), series + 1))?.clone();
    let out_core = output_core(&in_core);
    let meta = OmeMetadata {
        images: vec![reader.metadata().images.get(series).cloned().unwrap_or_default()],
        global_metadata: reader.metadata().global_metadata.clone(),
    };
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = temp_path(dst);
    let options = WriterOptions { original_metadata: false, ..Default::default() };
    let mut writer = bioformats::create_writer(&tmp, std::slice::from_ref(&out_core), &meta, &options).map_err(|e| anyhow!("{e}"))?;
    let result = (|| -> Result<()> {
        let chunks = writer.plane_chunks(0);
        let chunk_bytes = chunks.first().map_or(1, |r| in_core.region_bytes(r.w, r.h)).max(1);
        let batch = (BATCH_BYTES / chunk_bytes).max(rayon::current_num_threads()).max(1);
        let total = (in_core.plane_bytes() as f64 * in_core.image_count as f64).max(1.0);
        let mut done = 0f64;
        let (samples, bytes) = (in_core.rgb_channel_count.max(1) as usize, in_core.pixel_type.bytes());
        for plane in 0..in_core.image_count {
            for (bi, group) in chunks.chunks(batch).enumerate() {
                cancel.check()?;
                let encoded: Vec<Vec<u8>> = group
                    .par_iter()
                    .map(|r: &Region| {
                        let mut buf = vec![0u8; in_core.region_bytes(r.w, r.h)];
                        reader.open_bytes(series, plane, *r, &mut buf).map_err(|e| anyhow!("{e}"))?;
                        if samples > 1 && !in_core.interleaved {
                            let mut out = vec![0u8; buf.len()];
                            interleave(&buf, &mut out, samples, bytes);
                            buf = out;
                        }
                        if !in_core.little_endian && bytes > 1 {
                            swap_endianness(&mut buf, bytes);
                        }
                        writer.encode_chunk(0, *r, buf).map_err(|e| anyhow!("{e}"))
                    })
                    .collect::<Result<_>>()?;
                for (k, e) in encoded.iter().enumerate() {
                    writer.write_chunk(0, plane, bi * batch + k, e).map_err(|e| anyhow!("{e}"))?;
                }
                done += group.iter().map(|r| in_core.region_bytes(r.w, r.h) as f64).sum::<f64>();
                progress((done / total * 100.0).min(99.0) as f32);
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            writer.finish().map_err(|e| anyhow!("{e}"))?;
            std::fs::rename(&tmp, dst)?;
            Ok(())
        }
        Err(e) => {
            drop(writer);
            std::fs::remove_file(&tmp).ok();
            Err(e)
        }
    }
}

/// Produces the workspace TIFF for series `series` of `src`. Single-image TIFFs are
/// copied (a copy-on-write clone where the file system supports it); everything else
/// is converted.
pub fn convert(src: &Path, series: usize, dst: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<()> {
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if matches!(ext.as_str(), "tif" | "tiff") && series == 0 {
        if let Ok((_, s)) = imageio::probe(src) {
            if s.len() == 1 {
                if let Some(dir) = dst.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::copy(src, dst).with_context(|| format!("copying {}", src.display()))?;
                return Ok(());
            }
        }
    }
    convert_series(src, series, dst, cancel, progress)
}

/// Writes the downsampled image: every channel subsampled ×20 and contrast-adjusted,
/// one page per channel. Unlike the MATLAB version all channels are kept, so any
/// channel can serve for registration.
pub fn downsample(conv: &Path, down: &Path) -> Result<()> {
    let f = ImageFile::open(conv)?;
    let channels: Vec<_> = (0..f.channel_count())
        .map(|c| Ok(imadjust_u16(&f.read_subsampled(c, DOWNSAMPLE)?)))
        .collect::<Result<_>>()?;
    let pages: Vec<Vec<_>> = channels.iter().map(|c| vec![c]).collect();
    imageio::write_pages(down, &pages, None)
}

pub fn read_metadata(conv: &Path, down: &Path) -> Result<Metadata> {
    let f = ImageFile::open(conv)?;
    let d = ImageFile::open(down)?;
    Ok(Metadata {
        size: [f.height(), f.width()],
        down_size: [d.height(), d.width()],
        channel_count: f.channel_count(),
        pixel_dims: f.pixel_size_um(),
    })
}

/// Full import of one series of an image.
pub fn import(src: &Path, series: usize, conv_path: &Path, down_path: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<Metadata> {
    convert(src, series, conv_path, cancel, progress)?;
    cancel.check()?;
    progress(99.0);
    downsample(conv_path, down_path).context("downsampling")?;
    read_metadata(conv_path, down_path)
}
