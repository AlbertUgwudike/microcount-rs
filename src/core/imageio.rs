//! Image I/O through the `bioformats` crate (the Rust port of OME Bio-Formats).
//!
//! Microcount thinks in channels: channel `c` of an image is either a sample of an RGB
//! plane or a plane of its own, as MATLAB's `imread` saw TIFF pages. [`ImageFile`]
//! maps that onto Bio-Formats' series / plane / sample model and converts pixels to
//! the integer types the analysis uses. [`write_tiff`] streams 16-bit images out
//! through the Bio-Formats TIFF writer.

use crate::core::image::{Image, Rect};
use anyhow::{anyhow, bail, Context, Result};
use bioformats::meta::{ImageMetadata, Quantity};
use bioformats::reader::Region;
use bioformats::{CoreMetadata, FormatReader, OmeMetadata, PixelType, WriterOptions, Zct};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// Conversion from a decoded sample to an in-memory pixel type.
pub trait Sample: Copy + Default + Send + Sync + 'static {
    fn from_f64(v: f64) -> Self;
    fn from_u32(v: u32) -> Self;
}

impl Sample for u16 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v.round().clamp(0.0, 65535.0) as u16
    }
    #[inline]
    fn from_u32(v: u32) -> Self {
        v.min(65535) as u16
    }
}

impl Sample for u32 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v.round().clamp(0.0, u32::MAX as f64) as u32
    }
    #[inline]
    fn from_u32(v: u32) -> Self {
        v
    }
}

/// One series of an image file opened with Bio-Formats.
pub struct ImageFile {
    reader: Box<dyn FormatReader>,
    series: usize,
    pub path: PathBuf,
}

/// What [`probe`] reports about each series of a file.
#[derive(Clone, Debug)]
pub struct SeriesInfo {
    pub index: usize,
    pub name: Option<String>,
    pub width: usize,
    pub height: usize,
    pub channels: usize,
}

/// Lists the series in a file (a Leica project can hold many images).
pub fn probe(path: &Path) -> Result<(String, Vec<SeriesInfo>)> {
    let reader = bioformats::open(path).with_context(|| format!("opening {}", path.display()))?;
    let series = reader
        .series()
        .iter()
        .enumerate()
        .map(|(i, c)| SeriesInfo {
            index: i,
            name: reader.metadata().images.get(i).and_then(|m| m.name.clone()),
            width: c.size_x as usize,
            height: c.size_y as usize,
            channels: c.size_c as usize,
        })
        .collect();
    Ok((reader.format_name().to_string(), series))
}

/// Names of the formats that can be imported.
pub fn readable_formats() -> Vec<&'static str> {
    bioformats::formats::READERS.iter().map(|f| f.name).collect()
}

/// Lower-case suffixes of the formats that can be imported.
pub fn readable_suffixes() -> Vec<&'static str> {
    bioformats::formats::READERS.iter().flat_map(|f| f.suffixes.iter().copied()).collect()
}

fn micrometres(q: &Quantity) -> f64 {
    let f = match q.unit.as_str() {
        "nm" => 1e-3,
        "mm" => 1e3,
        "cm" => 1e4,
        "m" => 1e6,
        "Å" => 1e-4,
        _ => 1.0,
    };
    q.value * f
}

impl ImageFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_series(path, 0)
    }

    pub fn open_series(path: impl AsRef<Path>, series: usize) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let reader = bioformats::open(&path).with_context(|| format!("opening {}", path.display()))?;
        if series >= reader.series().len() {
            bail!("{} has no series {}", path.display(), series + 1);
        }
        Ok(ImageFile { reader, series, path })
    }

    pub fn core(&self) -> &CoreMetadata {
        &self.reader.series()[self.series]
    }

    pub fn width(&self) -> usize {
        self.core().size_x as usize
    }

    pub fn height(&self) -> usize {
        self.core().size_y as usize
    }

    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.width(), self.height())
    }

    /// Channels, counting RGB samples (MATLAB `max(SamplesPerPixel, numel(info))`).
    pub fn channel_count(&self) -> usize {
        self.core().size_c as usize
    }

    /// Samples stored in each plane (3 for RGB).
    pub fn samples(&self) -> usize {
        self.core().rgb_channel_count.max(1) as usize
    }

    pub fn planes(&self) -> usize {
        self.core().image_count as usize
    }

    /// Largest value of the stored type, as MATLAB uses for 8-bit data.
    pub fn full_range(&self) -> u16 {
        match self.core().pixel_type {
            PixelType::Uint8 | PixelType::Int8 | PixelType::Bit => 255,
            _ => 65535,
        }
    }

    /// Physical pixel size `[height, width]` in µm, when the file records it.
    pub fn pixel_size_um(&self) -> Option<[f64; 2]> {
        let m = self.reader.metadata().images.get(self.series)?;
        let x = micrometres(m.physical_size_x.as_ref()?);
        let y = micrometres(m.physical_size_y.as_ref()?);
        // Screen-resolution placeholders (72 dpi etc.) are not microscope calibrations.
        (x > 0.0 && y > 0.0 && x < 50.0 && y < 50.0).then_some([y, x])
    }

    fn region(&self, rect: Rect) -> Result<Region> {
        if rect.is_empty() || rect.x1() > self.width() || rect.y1() > self.height() {
            bail!("region {rect:?} outside {}x{} image", self.width(), self.height());
        }
        Ok(Region::new(rect.x as u32, rect.y as u32, rect.w as u32, rect.h as u32))
    }

    /// Every sample of plane `plane` inside `rect`.
    pub fn read_plane<T: Sample>(&self, plane: usize, rect: Rect) -> Result<Vec<Image<T>>> {
        let core = self.core();
        if plane >= self.planes() {
            bail!("plane {} requested but the image has {}", plane + 1, self.planes());
        }
        let region = self.region(rect)?;
        let mut buf = vec![0u8; core.region_bytes(region.w, region.h)];
        self.reader
            .open_bytes(self.series, plane as u32, region, &mut buf)
            .with_context(|| format!("reading {}", self.path.display()))?;
        Ok((0..self.samples()).map(|s| decode(&buf, core, rect.w, rect.h, s)).collect())
    }

    /// Plane and sample that hold channel `ch` (0-based) at the first Z and T.
    fn locate(&self, ch: usize) -> Result<(usize, usize)> {
        let core = self.core();
        if ch >= self.channel_count() {
            bail!("channel {} requested but the image has {}", ch + 1, self.channel_count());
        }
        let s = self.samples();
        let plane = core
            .plane_index(Zct { z: 0, c: (ch / s) as u32, t: 0 })
            .map_err(|e| anyhow!("{e}"))?;
        Ok((plane as usize, ch % s))
    }

    /// Channel `ch` (0-based) inside `rect`.
    pub fn read_channel(&self, ch: usize, rect: Rect) -> Result<Image<u16>> {
        let (plane, sample) = self.locate(ch)?;
        Ok(self.read_plane::<u16>(plane, rect)?.swap_remove(sample))
    }

    /// Every `step`-th row and column of channel `ch`, starting at the first. Reads
    /// whole bands of rows at a time, in parallel.
    pub fn read_subsampled(&self, ch: usize, step: usize) -> Result<Image<u16>> {
        let (plane, sample) = self.locate(ch)?;
        let (w, h) = (self.width(), self.height());
        let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
        let (_, band) = self.reader.optimal_tile_size(self.series);
        let band = (band as usize).clamp(1, h);
        let bands: Vec<usize> = (0..h).step_by(band).collect();
        let rows: Vec<Vec<(usize, Vec<u16>)>> = bands
            .par_iter()
            .map(|&y0| {
                let y1 = (y0 + band).min(h);
                let wanted: Vec<usize> = (y0..y1).filter(|y| y % step == 0).collect();
                let (Some(&a), Some(&b)) = (wanted.first(), wanted.last()) else {
                    return Ok(vec![]);
                };
                let img = self.read_plane::<u16>(plane, Rect::new(0, a, w, b - a + 1))?.swap_remove(sample);
                Ok(wanted
                    .iter()
                    .map(|&y| (y / step, img.row(y - a).iter().step_by(step).copied().collect()))
                    .collect())
            })
            .collect::<Result<_>>()?;
        let mut out = Image::filled(ow, oh, 0u16);
        for (oy, row) in rows.into_iter().flatten() {
            out.data[oy * ow..(oy + 1) * ow].copy_from_slice(&row);
        }
        Ok(out)
    }
}

/// Decodes sample `s` of a `w` x `h` region buffer laid out as `core` describes.
fn decode<T: Sample>(buf: &[u8], core: &CoreMetadata, w: usize, h: usize, s: usize) -> Image<T> {
    let b = core.pixel_type.bytes();
    let spp = core.rgb_channel_count.max(1) as usize;
    let n = w * h;
    let le = core.little_endian;
    let at = |i: usize| -> usize { if core.interleaved { (i * spp + s) * b } else { (s * n + i) * b } };
    let pt = core.pixel_type;
    let data: Vec<T> = (0..n)
        .into_par_iter()
        .map(|i| {
            let o = at(i);
            let raw = &buf[o..o + b];
            macro_rules! num {
                ($t:ty) => {{
                    let a = raw.try_into().unwrap();
                    if le {
                        <$t>::from_le_bytes(a)
                    } else {
                        <$t>::from_be_bytes(a)
                    }
                }};
            }
            match pt {
                PixelType::Uint8 | PixelType::Bit => T::from_u32(raw[0] as u32),
                PixelType::Int8 => T::from_f64(raw[0] as i8 as f64),
                PixelType::Uint16 => T::from_u32(num!(u16) as u32),
                PixelType::Int16 => T::from_f64(num!(i16) as f64),
                PixelType::Uint32 => T::from_u32(num!(u32)),
                PixelType::Int32 => T::from_f64(num!(i32) as f64),
                PixelType::Float => T::from_f64(num!(f32) as f64),
                PixelType::Double => T::from_f64(num!(f64)),
            }
        })
        .collect();
    Image::from_vec(w, h, data)
}

/// Writes `planes` 16-bit planes of `samples` (1 or 3) channels as a TIFF. Rows
/// (interleaved samples, `w * samples` values) come from `fill(plane, y, row)`, are
/// generated and encoded in parallel batches and streamed to disk.
pub fn write_tiff<F>(path: &Path, w: usize, h: usize, samples: usize, planes: usize, pixel_um: Option<[f64; 2]>, fill: F) -> Result<()>
where
    F: Fn(usize, usize, &mut [u16]) + Sync,
{
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let mut core = CoreMetadata::new(w as u32, h as u32, PixelType::Uint16);
    core.rgb_channel_count = samples as u32;
    core.size_c = (samples * planes) as u32;
    core.interleaved = samples > 1;
    core.update_image_count();
    let mut meta = OmeMetadata {
        images: vec![ImageMetadata {
            physical_size_x: pixel_um.map(|p| Quantity::micrometers(p[1])),
            physical_size_y: pixel_um.map(|p| Quantity::micrometers(p[0])),
            ..Default::default()
        }],
        ..Default::default()
    };
    meta.conform_to(std::slice::from_ref(&core));
    let options = WriterOptions { original_metadata: false, ..Default::default() };
    let mut writer = bioformats::create_writer(path, std::slice::from_ref(&core), &meta, &options)
        .with_context(|| format!("creating {}", path.display()))?;
    let chunks = writer.plane_chunks(0);
    let row_len = w * samples;
    let batch = ((64usize << 20) / (row_len * 2 * chunks.first().map_or(1, |c| c.h as usize)).max(1)).max(rayon::current_num_threads());
    for plane in 0..planes {
        for (bi, group) in chunks.chunks(batch).enumerate() {
            let encoded: Vec<Vec<u8>> = group
                .par_iter()
                .map(|r| {
                    let mut row = vec![0u16; row_len];
                    let mut bytes = Vec::with_capacity(r.w as usize * r.h as usize * samples * 2);
                    for y in r.y as usize..(r.y + r.h) as usize {
                        fill(plane, y, &mut row);
                        let seg = &row[r.x as usize * samples..(r.x + r.w) as usize * samples];
                        bytes.extend(seg.iter().flat_map(|v| v.to_le_bytes()));
                    }
                    writer.encode_chunk(0, *r, bytes).map_err(|e| anyhow!("{e}"))
                })
                .collect::<Result<_>>()?;
            for (k, e) in encoded.iter().enumerate() {
                writer.write_chunk(0, plane as u32, bi * batch + k, e).map_err(|e| anyhow!("{e}"))?;
            }
        }
    }
    writer.finish().map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

/// Writes small images: each page is 1 or 3 channels of equal size.
pub fn write_pages(path: &Path, pages: &[Vec<&Image<u16>>], pixel_um: Option<[f64; 2]>) -> Result<()> {
    let (w, h, spp) = (pages[0][0].w, pages[0][0].h, pages[0].len());
    write_tiff(path, w, h, spp, pages.len(), pixel_um, |p, y, row| {
        for (c, img) in pages[p].iter().enumerate() {
            for (x, v) in img.row(y).iter().enumerate() {
                row[x * spp + c] = *v;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_rgb_and_gray_planes() {
        let dir = std::env::temp_dir().join(format!("mc_io_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (37, 1201);
        let a = Image::from_vec(w, h, (0..w * h).map(|i| (i % 65536) as u16).collect());
        let b = a.map(|v| v / 2);
        let c = a.map(|v| 65535 - v);

        let rgb = dir.join("rgb.tiff");
        write_pages(&rgb, &[vec![&a, &b, &c], vec![&c, &b, &a]], Some([0.5, 0.25])).unwrap();
        let f = ImageFile::open(&rgb).unwrap();
        assert_eq!((f.width(), f.height(), f.samples(), f.planes()), (w, h, 3, 2));
        assert_eq!(f.read_plane::<u16>(1, f.rect()).unwrap(), vec![c.clone(), b.clone(), a.clone()]);
        let r = Rect::new(5, 300, 20, 700);
        assert_eq!(f.read_channel(1, r).unwrap(), b.crop(r));
        let px = f.pixel_size_um().unwrap();
        assert!((px[0] - 0.5).abs() < 1e-6 && (px[1] - 0.25).abs() < 1e-6, "{px:?}");

        let gray = dir.join("gray.tiff");
        write_pages(&gray, &[vec![&a], vec![&b], vec![&c]], None).unwrap();
        let g = ImageFile::open(&gray).unwrap();
        assert_eq!(g.channel_count(), 3);
        assert_eq!(g.read_channel(2, r).unwrap(), c.crop(r));
        let sub = g.read_subsampled(1, 20).unwrap();
        assert_eq!((sub.w, sub.h), (2, 61));
        assert_eq!(sub.get(1, 3), b.get(20, 60));
        std::fs::remove_dir_all(dir).ok();
    }
}
