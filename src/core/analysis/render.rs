//! Output images of a processed region (the image half of `data2result.m`).
//!
//! Seven RGB pages, as in the MATLAB version:
//! 1 cell marker (contrast adjusted), 2 co-marker, 3 cell outlines (random colour per
//! cell), 4 co-marker outlines, 5 = 1 with 3 overlaid, 6 = 2 with 4 overlaid,
//! 7 soma outlines.

use super::data::Data;
use crate::core::image::{Image, Mask, Rect};
use crate::core::imgproc::{label, stats};
use crate::core::tiff::TiffWriter;
use anyhow::Result;
use std::path::Path;

pub const N_PAGES: usize = 7;
pub const PAGE_CELL: usize = 0;
pub const PAGE_CO: usize = 1;
pub const PAGE_CELL_POLY: usize = 2;
pub const PAGE_CO_POLY: usize = 3;
pub const PAGE_SOMA: usize = 6;

/// Deterministic colour in [40000, 60000]³ for a cell label.
pub fn label_colour(l: u32) -> [u16; 3] {
    let mut z = (l as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut out = [0u16; 3];
    for c in out.iter_mut() {
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        *c = 40000 + (z % 20000) as u16;
    }
    out
}

pub struct Renderer<'a> {
    d: &'a Data,
    iba1_lut: Vec<u16>,
    cd68_lut: Vec<u16>,
    co_perim: Mask,
    soma_perim: Mask,
}

impl<'a> Renderer<'a> {
    pub fn new(d: &'a Data) -> Self {
        let chans: Vec<&Image<u16>> = d.iba1.iter().collect();
        Self {
            iba1_lut: stats::imadjust_lut(stats::stretchlim_u16(&chans)),
            cd68_lut: stats::imadjust_lut(stats::stretchlim_u16(&[&d.cd68])),
            co_perim: label::perimeter(&d.cd68_mask),
            soma_perim: label::perimeter(&d.soma_mask),
            d,
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.d.cd68.w, self.d.cd68.h)
    }

    fn cell_rgb(&self, i: usize) -> [u16; 3] {
        let c = &self.d.iba1;
        if c.len() >= 3 {
            [self.iba1_lut[c[0].data[i] as usize], self.iba1_lut[c[1].data[i] as usize], self.iba1_lut[c[2].data[i] as usize]]
        } else {
            [self.iba1_lut[c[0].data[i] as usize], 0, 0]
        }
    }

    /// Fills one interleaved RGB row of `page`.
    pub fn row(&self, page: usize, y: usize, out: &mut [u16]) {
        let w = self.d.cd68.w;
        for x in 0..w {
            let i = y * w + x;
            let poly = self.d.cells.poly.data[i];
            let px: [u16; 3] = match page {
                0 => self.cell_rgb(i),
                1 => [self.cd68_lut[self.d.cd68.data[i] as usize]; 3],
                2 => {
                    if poly > 0 {
                        label_colour(poly)
                    } else {
                        [0; 3]
                    }
                }
                3 => [if self.co_perim.data[i] { 65535 } else { 0 }; 3],
                4 => {
                    if poly > 0 {
                        label_colour(poly)
                    } else {
                        self.cell_rgb(i)
                    }
                }
                5 => {
                    if self.co_perim.data[i] {
                        [65535; 3]
                    } else {
                        [self.cd68_lut[self.d.cd68.data[i] as usize]; 3]
                    }
                }
                _ => [if self.soma_perim.data[i] { 65535 } else { 0 }; 3],
            };
            out[3 * x..3 * x + 3].copy_from_slice(&px);
        }
    }

    pub fn write_tiff(&self, path: &Path) -> Result<()> {
        let (w, h) = self.size();
        let bytes = (w * h * 3 * 2 * N_PAGES) as u64;
        let mut tw = TiffWriter::create(path, TiffWriter::needs_big(bytes))?;
        for page in 0..N_PAGES {
            tw.write_page(w, h, 3, |y, row| self.row(page, y, row))?;
        }
        tw.finish()
    }

    /// Materialises `rect` of a page as three channel images (used for previews).
    pub fn page(&self, page: usize, rect: Rect) -> [Image<u16>; 3] {
        let (w, _) = self.size();
        let mut row = vec![0u16; w * 3];
        let mut chans = [
            Image::filled(rect.w, rect.h, 0u16),
            Image::filled(rect.w, rect.h, 0u16),
            Image::filled(rect.w, rect.h, 0u16),
        ];
        for y in 0..rect.h {
            self.row(page, rect.y + y, &mut row);
            for x in 0..rect.w {
                for c in 0..3 {
                    chans[c].data[y * rect.w + x] = row[3 * (rect.x + x) + c];
                }
            }
        }
        chans
    }
}
