//! TIFF / BigTIFF reading and writing.
//!
//! The reader decodes only the strips or tiles that intersect a requested rectangle (the
//! equivalent of MATLAB's `imread(..., PixelRegion=...)`), which keeps memory bounded on
//! whole-slide images. Uncompressed data (what the Bio-Formats converter produces) is
//! read straight from disk row segment by row segment. LZW, Deflate and PackBits are
//! decoded; anything else is reported so the caller can fall back to Bio-Formats.
//!
//! The writer produces uncompressed, little-endian, strip-organised (Big)TIFFs with
//! 16-bit samples, streaming rows from a generator so large outputs never need to be
//! materialised.

use crate::core::image::{Image, Rect};
use anyhow::{anyhow, bail, Context, Result};
use rayon::prelude::*;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
fn read_at(f: &File, buf: &mut [u8], off: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.read_exact_at(buf, off)
}

#[cfg(windows)]
fn read_at(f: &File, mut buf: &mut [u8], mut off: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = f.seek_read(buf, off)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf = &mut buf[n..];
        off += n as u64;
    }
    Ok(())
}

/// Conversion from raw TIFF samples into an in-memory pixel type.
pub trait Sample: Copy + Default + Send + Sync + 'static {
    fn from_u32(v: u32) -> Self;
    fn from_f32(v: f32) -> Self;
}

impl Sample for u16 {
    #[inline]
    fn from_u32(v: u32) -> Self {
        v.min(u16::MAX as u32) as u16
    }
    #[inline]
    fn from_f32(v: f32) -> Self {
        v.round().clamp(0.0, 65535.0) as u16
    }
}

impl Sample for u32 {
    #[inline]
    fn from_u32(v: u32) -> Self {
        v
    }
    #[inline]
    fn from_f32(v: f32) -> Self {
        v.max(0.0) as u32
    }
}

impl Sample for f32 {
    #[inline]
    fn from_u32(v: u32) -> Self {
        v as f32
    }
    #[inline]
    fn from_f32(v: f32) -> Self {
        v
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Layout {
    Strips { rows: usize },
    Tiles { tw: usize, th: usize },
}

#[derive(Clone, Debug)]
pub struct Page {
    pub width: usize,
    pub height: usize,
    pub spp: usize,
    pub bps: u16,
    pub sample_format: u16,
    pub compression: u16,
    pub predictor: u16,
    pub planar: u16,
    pub photometric: u16,
    pub layout: Layout,
    offsets: Vec<u64>,
    counts: Vec<u64>,
    pub description: Option<String>,
    pub x_res: Option<f64>,
    pub y_res: Option<f64>,
    pub res_unit: u16,
}

impl Page {
    fn bytes_per_sample(&self) -> usize {
        (self.bps as usize) / 8
    }

    /// Samples stored per pixel within one chunk.
    fn spc(&self) -> usize {
        if self.planar == 2 {
            1
        } else {
            self.spp
        }
    }

    fn planes(&self) -> usize {
        if self.planar == 2 {
            self.spp
        } else {
            1
        }
    }

    /// (chunk width, chunk height, chunks across, chunks down)
    fn grid(&self) -> (usize, usize, usize, usize) {
        match self.layout {
            Layout::Strips { rows } => (self.width, rows, 1, self.height.div_ceil(rows)),
            Layout::Tiles { tw, th } => (tw, th, self.width.div_ceil(tw), self.height.div_ceil(th)),
        }
    }

    pub fn is_tiled(&self) -> bool {
        matches!(self.layout, Layout::Tiles { .. })
    }

    pub fn supported(&self) -> Result<()> {
        if !matches!(self.bps, 8 | 16 | 32) {
            bail!("unsupported bit depth {}", self.bps);
        }
        if !matches!(self.compression, 1 | 5 | 8 | 32946 | 32773) {
            bail!("unsupported TIFF compression scheme {}", self.compression);
        }
        if self.sample_format == 3 && self.bps != 32 {
            bail!("unsupported floating point depth {}", self.bps);
        }
        Ok(())
    }
}

pub struct TiffFile {
    file: File,
    pub path: PathBuf,
    le: bool,
    pub pages: Vec<Page>,
}

struct Entry {
    tag: u16,
    typ: u16,
    count: u64,
    data: Vec<u8>,
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => 1,
    }
}

impl TiffFile {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
        let mut hdr = [0u8; 16];
        read_at(&file, &mut hdr[..8], 0).context("reading TIFF header")?;
        let le = match &hdr[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => bail!("{} is not a TIFF file", path.display()),
        };
        let mut tf = TiffFile { file, path, le, pages: vec![] };
        let magic = tf.u16(&hdr[2..4]);
        let big = match magic {
            42 => false,
            43 => {
                read_at(&tf.file, &mut hdr, 0)?;
                true
            }
            _ => bail!("bad TIFF magic number {magic}"),
        };
        let mut next = if big { tf.u64(&hdr[8..16]) } else { tf.u32(&hdr[4..8]) as u64 };
        let mut seen = std::collections::HashSet::new();
        while next != 0 && seen.insert(next) {
            let (entries, nxt) = tf.read_ifd(next, big)?;
            tf.pages.push(tf.parse_page(&entries)?);
            next = nxt;
        }
        if tf.pages.is_empty() {
            bail!("{} contains no images", tf.path.display());
        }
        Ok(tf)
    }

    fn u16(&self, b: &[u8]) -> u16 {
        let a = [b[0], b[1]];
        if self.le {
            u16::from_le_bytes(a)
        } else {
            u16::from_be_bytes(a)
        }
    }

    fn u32(&self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        if self.le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        }
    }

    fn u64(&self, b: &[u8]) -> u64 {
        let a: [u8; 8] = b[..8].try_into().unwrap();
        if self.le {
            u64::from_le_bytes(a)
        } else {
            u64::from_be_bytes(a)
        }
    }

    fn read_ifd(&self, off: u64, big: bool) -> Result<(Vec<Entry>, u64)> {
        let (count_sz, entry_sz, ptr_sz) = if big { (8, 20, 8) } else { (2, 12, 4) };
        let mut cbuf = [0u8; 8];
        read_at(&self.file, &mut cbuf[..count_sz], off)?;
        let n = if big { self.u64(&cbuf) } else { self.u16(&cbuf) as u64 } as usize;
        if n > 10_000 {
            bail!("corrupt TIFF directory ({n} entries)");
        }
        let mut buf = vec![0u8; n * entry_sz + ptr_sz];
        read_at(&self.file, &mut buf, off + count_sz as u64)?;
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let e = &buf[i * entry_sz..(i + 1) * entry_sz];
            let tag = self.u16(&e[0..2]);
            let typ = self.u16(&e[2..4]);
            let (count, val) = if big {
                (self.u64(&e[4..12]), &e[12..20])
            } else {
                (self.u32(&e[4..8]) as u64, &e[8..12])
            };
            let total = type_size(typ) * count as usize;
            let data = if total <= val.len() {
                val[..total].to_vec()
            } else {
                let at = if big { self.u64(val) } else { self.u32(val) as u64 };
                let mut d = vec![0u8; total];
                read_at(&self.file, &mut d, at).with_context(|| format!("reading TIFF tag {tag}"))?;
                d
            };
            entries.push(Entry { tag, typ, count, data });
        }
        let p = &buf[n * entry_sz..];
        let next = if big { self.u64(p) } else { self.u32(p) as u64 };
        Ok((entries, next))
    }

    fn uints(&self, e: &Entry) -> Vec<u64> {
        let s = type_size(e.typ);
        (0..e.count as usize)
            .map(|i| {
                let b = &e.data[i * s..(i + 1) * s];
                match e.typ {
                    1 | 6 | 7 => b[0] as u64,
                    3 | 8 => self.u16(b) as u64,
                    4 | 9 | 13 => self.u32(b) as u64,
                    16 | 17 | 18 => self.u64(b),
                    _ => 0,
                }
            })
            .collect()
    }

    fn rational(&self, e: &Entry) -> Option<f64> {
        if e.typ != 5 || e.data.len() < 8 {
            return None;
        }
        let n = self.u32(&e.data[0..4]) as f64;
        let d = self.u32(&e.data[4..8]) as f64;
        (d > 0.0).then_some(n / d)
    }

    fn parse_page(&self, entries: &[Entry]) -> Result<Page> {
        let find = |t: u16| entries.iter().find(|e| e.tag == t);
        let first = |t: u16, d: u64| find(t).and_then(|e| self.uints(e).first().copied()).unwrap_or(d);
        let width = first(256, 0) as usize;
        let height = first(257, 0) as usize;
        if width == 0 || height == 0 {
            bail!("TIFF page without dimensions");
        }
        let layout = if find(322).is_some() {
            Layout::Tiles { tw: first(322, 0) as usize, th: first(323, 0) as usize }
        } else {
            let rows = (first(278, height as u64) as usize).clamp(1, height);
            Layout::Strips { rows }
        };
        let (offsets, counts) = match layout {
            Layout::Tiles { .. } => (
                find(324).map(|e| self.uints(e)).unwrap_or_default(),
                find(325).map(|e| self.uints(e)).unwrap_or_default(),
            ),
            Layout::Strips { .. } => (
                find(273).map(|e| self.uints(e)).unwrap_or_default(),
                find(279).map(|e| self.uints(e)).unwrap_or_default(),
            ),
        };
        let description = find(270).map(|e| {
            let s = String::from_utf8_lossy(&e.data);
            s.trim_end_matches('\0').to_string()
        });
        let page = Page {
            width,
            height,
            spp: first(277, 1) as usize,
            bps: first(258, 1) as u16,
            sample_format: first(339, 1) as u16,
            compression: first(259, 1) as u16,
            predictor: first(317, 1) as u16,
            planar: first(284, 1) as u16,
            photometric: first(262, 1) as u16,
            layout,
            offsets,
            counts,
            description,
            x_res: find(282).and_then(|e| self.rational(e)),
            y_res: find(283).and_then(|e| self.rational(e)),
            res_unit: first(296, 2) as u16,
        };
        let (_, _, across, down) = page.grid();
        let expected = across * down * page.planes();
        if page.offsets.len() < expected {
            bail!("TIFF page has {} chunks, expected {expected}", page.offsets.len());
        }
        Ok(page)
    }

    /// Reads and decodes one chunk completely, undoing any predictor.
    fn decode_chunk(&self, p: &Page, idx: usize, rows: usize) -> Result<Vec<u8>> {
        let (cw, _, _, _) = p.grid();
        let row_bytes = cw * p.spc() * p.bytes_per_sample();
        let expected = row_bytes * rows;
        let mut raw = vec![0u8; p.counts[idx] as usize];
        read_at(&self.file, &mut raw, p.offsets[idx])?;
        let mut out = match p.compression {
            1 => raw,
            5 => weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8)
                .decode(&raw)
                .map_err(|e| anyhow!("LZW decode failed: {e}"))?,
            8 | 32946 => {
                let mut v = Vec::with_capacity(expected);
                flate2::read::ZlibDecoder::new(&raw[..]).read_to_end(&mut v)?;
                v
            }
            32773 => packbits(&raw, expected),
            c => bail!("unsupported TIFF compression scheme {c}"),
        };
        out.resize(expected, 0);
        if p.predictor == 2 {
            undo_predictor(&mut out, row_bytes, p.spc(), p.bps, self.le);
        }
        Ok(out)
    }

    fn convert<T: Sample>(&self, src: &[u8], dst: &mut [T], p: &Page) {
        match (p.bps, p.sample_format) {
            (8, _) => dst.iter_mut().zip(src).for_each(|(d, &s)| *d = T::from_u32(s as u32)),
            (16, _) => {
                for (d, s) in dst.iter_mut().zip(src.chunks_exact(2)) {
                    *d = T::from_u32(self.u16(s) as u32)
                }
            }
            (32, 3) => {
                for (d, s) in dst.iter_mut().zip(src.chunks_exact(4)) {
                    *d = T::from_f32(f32::from_bits(self.u32(s)))
                }
            }
            _ => {
                for (d, s) in dst.iter_mut().zip(src.chunks_exact(4)) {
                    *d = T::from_u32(self.u32(s))
                }
            }
        }
    }

    /// Reads `rect` of page `page`, returning one image per sample (channel).
    pub fn read_region<T: Sample>(&self, page: usize, rect: Rect) -> Result<Vec<Image<T>>> {
        let p = self.pages.get(page).ok_or_else(|| anyhow!("page {page} does not exist"))?;
        p.supported()?;
        if rect.x1() > p.width || rect.y1() > p.height || rect.is_empty() {
            bail!("region {rect:?} outside {}x{} image", p.width, p.height);
        }
        let spp = p.spp;
        let spc = p.spc();
        let bpsb = p.bytes_per_sample();
        let (cw, ch, across, down) = p.grid();
        let per_plane = across * down;
        let mut planes_out: Vec<Vec<T>> = Vec::new();

        for plane in 0..p.planes() {
            let mut out = vec![T::default(); rect.w * rect.h * spc];
            // Split the output into row bands that each come from one chunk row.
            let mut bands: Vec<(usize, usize, &mut [T])> = Vec::new();
            let mut rest: &mut [T] = &mut out;
            let mut y = rect.y;
            while y < rect.y1() {
                let band = y / ch;
                let y_end = ((band + 1) * ch).min(rect.y1());
                let (head, tail) = rest.split_at_mut((y_end - y) * rect.w * spc);
                bands.push((band, y, head));
                rest = tail;
                y = y_end;
            }
            let tx0 = rect.x / cw;
            let tx1 = (rect.x1() - 1) / cw;

            let do_band = |band: usize, y0: usize, dst: &mut [T]| -> Result<()> {
                let rows = dst.len() / (rect.w * spc);
                let band_y0 = band * ch;
                let chunk_rows = match p.layout {
                    Layout::Strips { rows } => rows.min(p.height - band_y0),
                    Layout::Tiles { th, .. } => th,
                };
                for tx in tx0..=tx1 {
                    let idx = plane * per_plane + band * across + tx;
                    let cx0 = tx * cw;
                    let x0 = rect.x.max(cx0);
                    let x1 = rect.x1().min(cx0 + cw).min(p.width);
                    let seg = (x1 - x0) * spc;
                    let dst_off = (x0 - rect.x) * spc;
                    let row_bytes = cw * spc * bpsb;
                    if p.compression == 1 {
                        let full_rows = x0 == cx0 && x1 - cx0 == cw;
                        if full_rows {
                            let mut buf = vec![0u8; rows * row_bytes];
                            let off = p.offsets[idx] + ((y0 - band_y0) * row_bytes) as u64;
                            read_at(&self.file, &mut buf, off)?;
                            for r in 0..rows {
                                let d = &mut dst[r * rect.w * spc + dst_off..][..seg];
                                self.convert(&buf[r * row_bytes..][..seg * bpsb], d, p);
                            }
                        } else {
                            let mut buf = vec![0u8; seg * bpsb];
                            for r in 0..rows {
                                let off = p.offsets[idx]
                                    + (((y0 + r - band_y0) * cw + (x0 - cx0)) * spc * bpsb) as u64;
                                read_at(&self.file, &mut buf, off)?;
                                let d = &mut dst[r * rect.w * spc + dst_off..][..seg];
                                self.convert(&buf, d, p);
                            }
                        }
                    } else {
                        let dec = self.decode_chunk(p, idx, chunk_rows)?;
                        for r in 0..rows {
                            let s = ((y0 + r - band_y0) * cw + (x0 - cx0)) * spc * bpsb;
                            let d = &mut dst[r * rect.w * spc + dst_off..][..seg];
                            self.convert(&dec[s..s + seg * bpsb], d, p);
                        }
                    }
                }
                Ok(())
            };

            if p.compression != 1 && p.is_tiled() && tx1 > tx0 {
                // Compressed tiles: decode the tiles of each tile row in parallel.
                for (band, y0, dst) in bands {
                    let rows = dst.len() / (rect.w * spc);
                    let pieces: Vec<Result<(usize, usize, Vec<T>)>> = (tx0..=tx1)
                        .into_par_iter()
                        .map(|tx| {
                            let cx0 = tx * cw;
                            let x0 = rect.x.max(cx0);
                            let x1 = rect.x1().min(cx0 + cw).min(p.width);
                            let sub = Rect::new(x0 - rect.x, 0, x1 - x0, rows);
                            let mut tmp = vec![T::default(); sub.w * rows * spc];
                            let idx = plane * per_plane + band * across + tx;
                            let (_, th) = match p.layout {
                                Layout::Tiles { tw, th } => (tw, th),
                                _ => unreachable!(),
                            };
                            let dec = self.decode_chunk(p, idx, th)?;
                            for r in 0..rows {
                                let s = ((y0 + r - band * ch) * cw + (x0 - cx0)) * spc * bpsb;
                                self.convert(
                                    &dec[s..s + sub.w * spc * bpsb],
                                    &mut tmp[r * sub.w * spc..(r + 1) * sub.w * spc],
                                    p,
                                );
                            }
                            Ok((sub.x, sub.w, tmp))
                        })
                        .collect();
                    for piece in pieces {
                        let (x, w, tmp) = piece?;
                        for r in 0..rows {
                            dst[(r * rect.w + x) * spc..][..w * spc]
                                .copy_from_slice(&tmp[r * w * spc..(r + 1) * w * spc]);
                        }
                    }
                }
            } else {
                bands
                    .into_par_iter()
                    .map(|(band, y0, dst)| do_band(band, y0, dst))
                    .collect::<Result<Vec<_>>>()?;
            }
            planes_out.push(out);
        }

        // De-interleave into one image per sample.
        let n = rect.w * rect.h;
        let mut images = Vec::with_capacity(spp);
        if p.planar == 2 {
            for data in planes_out {
                images.push(Image::from_vec(rect.w, rect.h, data));
            }
        } else {
            let data = &planes_out[0];
            if spp == 1 {
                images.push(Image::from_vec(rect.w, rect.h, planes_out.pop().unwrap()));
            } else {
                for s in 0..spp {
                    let v: Vec<T> = (0..n).into_par_iter().map(|i| data[i * spp + s]).collect();
                    images.push(Image::from_vec(rect.w, rect.h, v));
                }
            }
        }
        Ok(images)
    }

    pub fn read_page<T: Sample>(&self, page: usize) -> Result<Vec<Image<T>>> {
        let p = &self.pages[page];
        self.read_region(page, Rect::new(0, 0, p.width, p.height))
    }

    /// Reads every `step`-th row and column, starting at the first (MATLAB
    /// `PixelRegion = {[1 step H], [1 step W]}`).
    pub fn read_subsampled(&self, page: usize, step: usize) -> Result<Vec<Image<u16>>> {
        let p = self.pages.get(page).ok_or_else(|| anyhow!("page {page} does not exist"))?;
        let (ow, oh) = (p.width.div_ceil(step), p.height.div_ceil(step));
        let pick = |row: &[Image<u16>]| -> Vec<Vec<u16>> {
            row.iter().map(|img| img.data.iter().step_by(step).copied().collect()).collect()
        };
        let rows: Vec<Vec<Vec<u16>>> = if p.compression == 1 {
            (0..oh)
                .into_par_iter()
                .map(|oy| {
                    let r = self.read_region::<u16>(page, Rect::new(0, oy * step, p.width, 1))?;
                    Ok(pick(&r))
                })
                .collect::<Result<_>>()?
        } else {
            let (_, ch, _, _) = p.grid();
            let mut out = vec![Vec::new(); oh];
            let mut y = 0;
            while y < p.height {
                let y1 = ((y / ch + 1) * ch).min(p.height);
                let wanted: Vec<usize> = (y..y1).filter(|r| r % step == 0).collect();
                if let (Some(&a), Some(&b)) = (wanted.first(), wanted.last()) {
                    let band = self.read_region::<u16>(page, Rect::new(0, a, p.width, b - a + 1))?;
                    for &r in &wanted {
                        let rows: Vec<Image<u16>> =
                            band.iter().map(|img| img.crop(Rect::new(0, r - a, p.width, 1))).collect();
                        out[r / step] = pick(&rows);
                    }
                }
                y = y1;
            }
            out
        };
        let spp = rows.first().map(|r| r.len()).unwrap_or(p.spp);
        Ok((0..spp)
            .map(|s| {
                let mut data = Vec::with_capacity(ow * oh);
                for r in &rows {
                    data.extend_from_slice(&r[s]);
                }
                Image::from_vec(ow, oh, data)
            })
            .collect())
    }

    /// Number of addressable channels (MATLAB: `max([info.SamplesPerPixel, numel(info)])`).
    pub fn channel_count(&self) -> usize {
        self.pages[0].spp.max(self.pages.len())
    }

    /// Reads channel `ch` (0-based) inside `rect`, following `Utility.read_tiff`: a
    /// single-page file holds channels as samples, a multi-page file as pages.
    pub fn read_channel(&self, ch: usize, rect: Rect) -> Result<Image<u16>> {
        if self.pages.len() == 1 {
            let mut imgs = self.read_region::<u16>(0, rect)?;
            if ch >= imgs.len() {
                bail!("channel {} requested but image has {} samples", ch + 1, imgs.len());
            }
            Ok(imgs.swap_remove(ch))
        } else {
            if ch >= self.pages.len() {
                bail!("channel {} requested but image has {} pages", ch + 1, self.pages.len());
            }
            Ok(self.read_region::<u16>(ch, rect)?.swap_remove(0))
        }
    }

    /// Physical pixel size `[height, width]` in µm from OME-XML or resolution tags.
    pub fn pixel_size_um(&self) -> Option<[f64; 2]> {
        let p = &self.pages[0];
        if let Some(desc) = &p.description {
            let attr = |name: &str| -> Option<f64> {
                let key = format!("{name}=\"");
                let start = desc.find(&key)? + key.len();
                let end = desc[start..].find('"')? + start;
                desc[start..end].parse::<f64>().ok()
            };
            let unit = |name: &str| -> f64 {
                let key = format!("{name}Unit=\"");
                desc.find(&key)
                    .and_then(|s| {
                        let s = s + key.len();
                        desc[s..].find('"').map(|e| &desc[s..s + e])
                    })
                    .map(|u| match u {
                        "nm" => 1e-3,
                        "mm" => 1e3,
                        "cm" => 1e4,
                        "m" => 1e6,
                        _ => 1.0,
                    })
                    .unwrap_or(1.0)
            };
            if let (Some(x), Some(y)) = (attr("PhysicalSizeX"), attr("PhysicalSizeY")) {
                return Some([y * unit("PhysicalSizeY"), x * unit("PhysicalSizeX")]);
            }
        }
        let per_um = match p.res_unit {
            2 => 25_400.0,
            3 => 10_000.0,
            _ => return None,
        };
        // Screen-resolution placeholders (72 dpi etc.) are not microscope calibrations.
        match (p.x_res, p.y_res) {
            (Some(x), Some(y)) if x > 0.0 && y > 0.0 && per_um / x < 50.0 && per_um / y < 50.0 => {
                Some([per_um / y, per_um / x])
            }
            _ => None,
        }
    }
}

fn packbits(src: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < src.len() && out.len() < expected {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let len = n as usize + 1;
            let end = (i + len).min(src.len());
            out.extend_from_slice(&src[i..end]);
            i = end;
        } else if n != -128 {
            let len = (-(n as isize)) as usize + 1;
            if i < src.len() {
                out.extend(std::iter::repeat(src[i]).take(len));
            }
            i += 1;
        }
    }
    out
}

fn undo_predictor(buf: &mut [u8], row_bytes: usize, spc: usize, bps: u16, le: bool) {
    for row in buf.chunks_mut(row_bytes) {
        match bps {
            8 => {
                for i in spc..row.len() {
                    row[i] = row[i].wrapping_add(row[i - spc]);
                }
            }
            16 => {
                let n = row.len() / 2;
                let rd = |r: &[u8], i: usize| {
                    let b = [r[2 * i], r[2 * i + 1]];
                    if le {
                        u16::from_le_bytes(b)
                    } else {
                        u16::from_be_bytes(b)
                    }
                };
                for i in spc..n {
                    let v = rd(row, i).wrapping_add(rd(row, i - spc));
                    let b = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                    row[2 * i] = b[0];
                    row[2 * i + 1] = b[1];
                }
            }
            32 => {
                let n = row.len() / 4;
                let rd = |r: &[u8], i: usize| {
                    let b: [u8; 4] = r[4 * i..4 * i + 4].try_into().unwrap();
                    if le {
                        u32::from_le_bytes(b)
                    } else {
                        u32::from_be_bytes(b)
                    }
                };
                for i in spc..n {
                    let v = rd(row, i).wrapping_add(rd(row, i - spc));
                    let b = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                    row[4 * i..4 * i + 4].copy_from_slice(&b);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------------------

/// Streaming writer for uncompressed 16-bit (Big)TIFFs with one or three samples.
pub struct TiffWriter {
    f: BufWriter<File>,
    big: bool,
    at: u64,
    next_ptr: u64,
}

const STRIP_BYTES: usize = 256 * 1024;

impl TiffWriter {
    pub fn create(path: impl AsRef<Path>, big: bool) -> Result<Self> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let f = BufWriter::with_capacity(
            4 << 20,
            File::create(path).with_context(|| format!("creating {}", path.display()))?,
        );
        let mut w = Self { f, big, at: 0, next_ptr: 0 };
        if big {
            w.put(b"II\x2b\x00\x08\x00\x00\x00")?;
            w.next_ptr = w.at;
            w.put(&0u64.to_le_bytes())?;
        } else {
            w.put(b"II\x2a\x00")?;
            w.next_ptr = w.at;
            w.put(&0u32.to_le_bytes())?;
        }
        Ok(w)
    }

    /// Whether files of `bytes` pixel data need BigTIFF offsets.
    pub fn needs_big(bytes: u64) -> bool {
        bytes > 3_900_000_000
    }

    fn put(&mut self, b: &[u8]) -> Result<()> {
        self.f.write_all(b)?;
        self.at += b.len() as u64;
        Ok(())
    }

    fn put_ptr(&mut self, v: u64) -> Result<()> {
        if self.big {
            self.put(&v.to_le_bytes())
        } else {
            self.put(&(v as u32).to_le_bytes())
        }
    }

    fn align(&mut self) -> Result<()> {
        if self.at % 2 == 1 {
            self.put(&[0])?;
        }
        Ok(())
    }

    /// Writes one page whose rows (interleaved samples, `w * spp` values) are produced
    /// by `row_fn(y, row)`. Rows are generated in parallel batches.
    pub fn write_page<F>(&mut self, w: usize, h: usize, spp: usize, row_fn: F) -> Result<()>
    where
        F: Fn(usize, &mut [u16]) + Sync,
    {
        let row_len = w * spp;
        let rps = (STRIP_BYTES / (row_len * 2)).clamp(1, h);
        let n_strips = h.div_ceil(rps);
        let mut offsets = Vec::with_capacity(n_strips);
        let mut counts = Vec::with_capacity(n_strips);
        let batch_rows = (rps * 32).min(h);
        let mut rows = vec![0u16; batch_rows * row_len];
        let mut bytes: Vec<u8> = Vec::new();
        let mut y = 0;
        while y < h {
            let n = batch_rows.min(h - y);
            rows[..n * row_len]
                .par_chunks_mut(row_len)
                .enumerate()
                .for_each(|(i, r)| row_fn(y + i, r));
            bytes.clear();
            bytes.extend(rows[..n * row_len].iter().flat_map(|v| v.to_le_bytes()));
            for strip in bytes.chunks(rps * row_len * 2) {
                offsets.push(self.at);
                counts.push(strip.len() as u64);
                self.f.write_all(strip)?;
                self.at += strip.len() as u64;
            }
            y += n;
        }
        self.write_ifd(w, h, spp, rps, &offsets, &counts)
    }

    fn write_ifd(
        &mut self,
        w: usize,
        h: usize,
        spp: usize,
        rps: usize,
        offsets: &[u64],
        counts: &[u64],
    ) -> Result<()> {
        let big = self.big;
        let ptr_type: u16 = if big { 16 } else { 4 };
        let ptrs = |v: &[u64]| -> Vec<u8> {
            if big {
                v.iter().flat_map(|x| x.to_le_bytes()).collect()
            } else {
                v.iter().flat_map(|&x| (x as u32).to_le_bytes()).collect()
            }
        };
        let software = b"Microcount\0".to_vec();
        let bps: Vec<u8> = (0..spp).flat_map(|_| 16u16.to_le_bytes()).collect();
        let photometric: u16 = if spp == 3 { 2 } else { 1 };
        let mut entries: Vec<(u16, u16, u64, Vec<u8>)> = vec![
            (256, 4, 1, (w as u32).to_le_bytes().to_vec()),
            (257, 4, 1, (h as u32).to_le_bytes().to_vec()),
            (258, 3, spp as u64, bps),
            (259, 3, 1, 1u16.to_le_bytes().to_vec()),
            (262, 3, 1, photometric.to_le_bytes().to_vec()),
            (273, ptr_type, offsets.len() as u64, ptrs(offsets)),
            (277, 3, 1, (spp as u16).to_le_bytes().to_vec()),
            (278, 4, 1, (rps as u32).to_le_bytes().to_vec()),
            (279, ptr_type, counts.len() as u64, ptrs(counts)),
            (284, 3, 1, 1u16.to_le_bytes().to_vec()),
            (305, 2, software.len() as u64, software),
        ];
        // Out-of-line values first, then the directory itself.
        let inline = if big { 8 } else { 4 };
        for e in entries.iter_mut() {
            if e.3.len() > inline {
                self.align()?;
                let at = self.at;
                let data = std::mem::take(&mut e.3);
                self.put(&data)?;
                e.3 = if big { at.to_le_bytes().to_vec() } else { (at as u32).to_le_bytes().to_vec() };
            }
        }
        self.align()?;
        let ifd_at = self.at;
        if big {
            self.put(&(entries.len() as u64).to_le_bytes())?;
        } else {
            self.put(&(entries.len() as u16).to_le_bytes())?;
        }
        for (tag, typ, count, mut val) in entries {
            val.resize(inline, 0);
            self.put(&tag.to_le_bytes())?;
            self.put(&typ.to_le_bytes())?;
            if big {
                self.put(&count.to_le_bytes())?;
            } else {
                self.put(&(count as u32).to_le_bytes())?;
            }
            self.put(&val)?;
        }
        let next_ptr_pos = self.at;
        self.put_ptr(0)?;
        // Link the previous directory (or the header) to this one.
        self.f.seek(SeekFrom::Start(self.next_ptr))?;
        if big {
            self.f.write_all(&ifd_at.to_le_bytes())?;
        } else {
            self.f.write_all(&(ifd_at as u32).to_le_bytes())?;
        }
        self.f.seek(SeekFrom::Start(self.at))?;
        self.next_ptr = next_ptr_pos;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.f.flush()?;
        Ok(())
    }
}

/// Writes small multi-page images in one call; each page is 1 or 3 channels.
pub fn write_pages(path: impl AsRef<Path>, pages: &[Vec<&Image<u16>>]) -> Result<()> {
    let bytes: u64 = pages.iter().map(|p| (p[0].len() * p.len() * 2) as u64).sum();
    let mut w = TiffWriter::create(path, TiffWriter::needs_big(bytes))?;
    for chans in pages {
        let (iw, ih, spp) = (chans[0].w, chans[0].h, chans.len());
        w.write_page(iw, ih, spp, |y, row| {
            for (c, img) in chans.iter().enumerate() {
                for (x, v) in img.row(y).iter().enumerate() {
                    row[x * spp + c] = *v;
                }
            }
        })?;
    }
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_rgb_and_gray_pages() {
        let dir = std::env::temp_dir().join(format!("mc_tiff_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.tiff");
        let (w, h) = (37, 1201);
        let a = Image::from_vec(w, h, (0..w * h).map(|i| (i % 65536) as u16).collect());
        let b = a.map(|v| v / 2);
        let c = a.map(|v| 65535 - v);
        write_pages(&path, &[vec![&a, &b, &c], vec![&b]]).unwrap();
        let t = TiffFile::open(&path).unwrap();
        assert_eq!(t.pages.len(), 2);
        let rgb = t.read_page::<u16>(0).unwrap();
        assert_eq!(rgb, vec![a.clone(), b.clone(), c.clone()]);
        let r = Rect::new(5, 300, 20, 700);
        assert_eq!(t.read_region::<u16>(0, r).unwrap()[2], c.crop(r));
        assert_eq!(t.read_channel(1, r).unwrap(), b.crop(r));
        let sub = t.read_subsampled(1, 20).unwrap();
        assert_eq!((sub[0].w, sub[0].h), (2, 61));
        assert_eq!(sub[0].get(1, 3), b.get(20, 60));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn packbits_decodes() {
        let enc = [0xFEu8, 0xAA, 0x02, 0x80, 0x00, 0x2A];
        assert_eq!(packbits(&enc, 6), vec![0xAA, 0xAA, 0xAA, 0x80, 0x00, 0x2A]);
    }
}
