//! Allen CCF atlas (`Atlas.m`): reference/annotation volumes, the structure tree,
//! warping annotation slices into histology space and building region masks.

use crate::core::image::{Image, Mask};
use crate::core::registration::Affine;
use crate::core::tiff::TiffFile;
use crate::core::workspace::{Direction, Laterality, Orientation, TransformData, PAD};
use anyhow::{anyhow, bail, Context, Result};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Structure {
    pub id: u32,
    pub acronym: String,
    pub name: String,
    pub parent: Option<u32>,
}

pub struct Atlas {
    /// Volume dimensions: rows (h), columns (w), slices (d) — MATLAB `size(volume)`.
    pub h: usize,
    pub w: usize,
    pub d: usize,
    reference: Vec<u16>,
    annotation: Vec<u32>,
    pub structures: Vec<Structure>,
    by_acronym: HashMap<String, usize>,
    children: HashMap<u32, Vec<u32>>,
    descendants: Mutex<HashMap<String, Arc<HashSet<u32>>>>,
}

/// Region key of the whole image (no atlas mask).
pub const WHOLE_IMAGE: &str = "WI";

fn read_volume<T: crate::core::tiff::Sample>(path: &Path) -> Result<(usize, usize, usize, Vec<T>)> {
    let tf = TiffFile::open(path)?;
    let (w, h, d) = (tf.pages[0].width, tf.pages[0].height, tf.pages.len());
    let pages: Vec<Vec<T>> = (0..d)
        .into_par_iter()
        .map(|p| Ok(tf.read_page::<T>(p)?.swap_remove(0).data))
        .collect::<Result<_>>()?;
    Ok((w, h, d, pages.concat()))
}

impl Atlas {
    pub fn load(assets: &Path) -> Result<Atlas> {
        let (w, h, d, reference) = read_volume::<u16>(&assets.join("reference.tiff")).context("loading reference atlas")?;
        let (aw, ah, ad, annotation) =
            read_volume::<u32>(&assets.join("annotation.tiff")).context("loading annotation atlas")?;
        if (aw, ah, ad) != (w, h, d) {
            bail!("reference and annotation volumes differ in size");
        }
        let structures = read_structures(&assets.join("structures.csv"))?;
        let by_acronym = structures.iter().enumerate().map(|(i, s)| (s.acronym.clone(), i)).collect();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for s in &structures {
            if let Some(p) = s.parent {
                children.entry(p).or_default().push(s.id);
            }
        }
        Ok(Atlas { h, w, d, reference, annotation, structures, by_acronym, children, descendants: Mutex::default() })
    }

    pub fn structure(&self, acronym: &str) -> Option<&Structure> {
        self.by_acronym.get(acronym).map(|&i| &self.structures[i])
    }

    pub fn children_of(&self, id: u32) -> &[u32] {
        self.children.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn by_id(&self, id: u32) -> Option<&Structure> {
        self.structures.iter().find(|s| s.id == id)
    }

    /// Ids of a structure and all of its descendants (`abr_to_idx_set`).
    pub fn id_set(&self, acronym: &str) -> Result<Arc<HashSet<u32>>> {
        if let Some(s) = self.descendants.lock().unwrap().get(acronym) {
            return Ok(s.clone());
        }
        let root = self.structure(acronym).ok_or_else(|| anyhow!("unknown atlas region {acronym}"))?.id;
        let mut set = HashSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if set.insert(id) {
                stack.extend_from_slice(self.children_of(id));
            }
        }
        let set = Arc::new(set);
        self.descendants.lock().unwrap().insert(acronym.to_string(), set.clone());
        Ok(set)
    }

    pub fn n_slices(&self, ori: Orientation) -> usize {
        match ori {
            Orientation::Axial => self.d,
            Orientation::Sagittal => self.w,
            Orientation::Coronal => self.h,
        }
    }

    /// `(rows, cols)` of a slice.
    pub fn slice_size(&self, ori: Orientation) -> (usize, usize) {
        match ori {
            Orientation::Axial => (self.h, self.w),
            Orientation::Sagittal => (self.h, self.d),
            Orientation::Coronal => (self.w, self.d),
        }
    }

    fn slice<T: Copy + Send + Sync + Default>(&self, vol: &[T], ori: Orientation, idx1: usize) -> Image<T> {
        let i = idx1.clamp(1, self.n_slices(ori)) - 1;
        let (rows, cols) = self.slice_size(ori);
        let (h, w) = (self.h, self.w);
        let at = |r: usize, c: usize, p: usize| vol[(p * h + r) * w + c];
        let data: Vec<T> = (0..rows * cols)
            .into_par_iter()
            .map(|k| {
                let (row, col) = (k / cols, k % cols);
                match ori {
                    Orientation::Axial => at(row, col, i),
                    Orientation::Sagittal => at(row, i, col),
                    Orientation::Coronal => at(i, row, col),
                }
            })
            .collect();
        Image::from_vec(cols, rows, data)
    }

    /// Reference (Nissl-like) slice, 1-based index.
    pub fn reference_slice(&self, ori: Orientation, idx1: usize) -> Image<u16> {
        self.slice(&self.reference, ori, idx1)
    }

    pub fn annotation_slice(&self, ori: Orientation, idx1: usize) -> Image<u32> {
        self.slice(&self.annotation, ori, idx1)
    }

    /// Hemisphere mask of a slice (true = left), `get_lr_mask`. For sagittal slices the
    /// midline is half the number of sagittal slices (the MATLAB code used the row
    /// count there, disagreeing with its own laterality label).
    pub fn left_mask(&self, ori: Orientation, idx1: usize) -> Mask {
        let (rows, cols) = self.slice_size(ori);
        match ori {
            Orientation::Axial => {
                let mid = cols / 2 - 1;
                Image::from_vec(cols, rows, (0..rows * cols).map(|k| k % cols < mid).collect())
            }
            Orientation::Sagittal => Mask::filled(cols, rows, idx1 * 2 >= self.n_slices(ori)),
            Orientation::Coronal => {
                let mid = rows / 2 - 1;
                Image::from_vec(cols, rows, (0..rows * cols).map(|k| k / cols < mid).collect())
            }
        }
    }

    pub fn laterality_label(&self, ori: Orientation, idx1: usize) -> &'static str {
        match ori {
            Orientation::Sagittal if idx1 * 2 < self.n_slices(ori) => "RIGHT",
            Orientation::Sagittal => "LEFT",
            _ => "LEFT | RIGHT",
        }
    }

    /// Annotation and hemisphere slices warped into the (padded, rotated) histology frame
    /// with nearest-neighbour sampling (`transform_atlas_slice`).
    pub fn warp(&self, td: &TransformData) -> (Image<u32>, Mask) {
        let ann = self.annotation_slice(td.orientation, td.slice_idx);
        let left = self.left_mask(td.orientation, td.slice_idx);
        let (rows, cols) = td.rotated_size();
        let inv = Affine(td.matrix).inverse().unwrap_or(Affine::IDENTITY);
        let mut out_a = Image::filled(cols, rows, 0u32);
        let mut out_l = Mask::filled(cols, rows, false);
        out_a
            .data
            .par_chunks_mut(cols)
            .zip(out_l.data.par_chunks_mut(cols))
            .enumerate()
            .for_each(|(r, (ra, rl))| {
                for c in 0..cols {
                    let (u, v) = inv.apply((c + 1) as f64, (r + 1) as f64);
                    let (x, y) = (u.round() - 1.0, v.round() - 1.0);
                    if x >= 0.0 && y >= 0.0 && (x as usize) < ann.w && (y as usize) < ann.h {
                        let i = y as usize * ann.w + x as usize;
                        ra[c] = ann.data[i];
                        rl[c] = left.data[i];
                    }
                }
            });
        (out_a, out_l)
    }

    /// Pixels belonging to any of `acronyms` (`fill_region`).
    pub fn fill(&self, ann: &Image<u32>, acronyms: &[&str]) -> Result<Mask> {
        let mut ids = HashSet::new();
        for a in acronyms {
            ids.extend(self.id_set(a)?.iter().copied());
        }
        Ok(ann.map(|v| v != 0 && ids.contains(&v)))
    }

    /// Region mask at downsampled resolution, oriented like the downsampled image
    /// (`create_dn_size_mask`). `down_size` is `(rows, cols)`.
    pub fn region_mask(&self, td: Option<&TransformData>, down_size: (usize, usize), key: &str, lat: Laterality) -> Result<Mask> {
        let (rows, cols) = down_size;
        if key == WHOLE_IMAGE {
            return Ok(Mask::filled(cols, rows, true));
        }
        let td = td.ok_or_else(|| anyhow!("image is not registered to the atlas"))?;
        let (ann, left) = self.warp(td);
        let ann = ann.unpad(PAD);
        let left = left.unpad(PAD);
        let region = self.fill(&ann, &[key])?;
        let m = region.zip_map(&left, |r, l| {
            r && match lat {
                Laterality::Left => l,
                Laterality::Right => !l,
                Laterality::Bilat => true,
            }
        });
        let m = m.rotate_ccw(td.direction.reverse_quarters());
        if (m.h, m.w) != (rows, cols) {
            bail!("atlas mask {}x{} does not match image {}x{}", m.h, m.w, rows, cols);
        }
        Ok(m)
    }
}

/// Border lines of the warped annotation plus fills for selected regions per hemisphere
/// (`calc_borders`).
pub struct Overlay {
    pub borders: Mask,
    pub left: Mask,
    pub right: Mask,
}

impl Atlas {
    pub fn overlay(&self, td: &TransformData, left_keys: &[&str], right_keys: &[&str]) -> Result<Overlay> {
        let (ann, lat) = self.warp(td);
        let (w, h) = (ann.w, ann.h);
        // round(conv2(ann, ones(3)/9, 'same')) ~= ann, with zero padding.
        let borders = Image::from_vec(
            w,
            h,
            (0..w * h)
                .into_par_iter()
                .map(|i| {
                    let (x, y) = ((i % w) as isize, (i / w) as isize);
                    let mut s = 0f64;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let (xx, yy) = (x + dx, y + dy);
                            if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                                s += ann.data[yy as usize * w + xx as usize] as f64;
                            }
                        }
                    }
                    (s / 9.0).round() != ann.data[i] as f64
                })
                .collect(),
        );
        let fill = |keys: &[&str], want_left: bool| -> Result<Mask> {
            if keys.is_empty() {
                return Ok(Mask::filled(w, h, false));
            }
            let m = self.fill(&ann, keys)?;
            Ok(m.zip_map(&lat, |r, l| r && l == want_left))
        };
        Ok(Overlay { borders, left: fill(left_keys, true)?, right: fill(right_keys, false)? })
    }
}

fn read_structures(path: &Path) -> Result<Vec<Structure>> {
    let mut rdr = csv::Reader::from_path(path).with_context(|| format!("reading {}", path.display()))?;
    let headers = rdr.headers()?.clone();
    let col = |name: &str| headers.iter().position(|h| h == name).ok_or_else(|| anyhow!("structures.csv lacks {name}"));
    let (ia, ii, inm, ip) = (col("acronym")?, col("id")?, col("name")?, col("parent_structure_id")?);
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        let id: u32 = rec[ii].trim().parse().context("bad structure id")?;
        let parent = rec[ip].trim().parse::<f64>().ok().map(|p| p as u32);
        out.push(Structure { id, acronym: rec[ia].to_string(), name: rec[inm].to_string(), parent });
    }
    Ok(out)
}

impl Direction {
    /// `to_angle` as counter-clockwise quarter turns (display orientation).
    pub fn quarters(self) -> i32 {
        match self {
            Direction::North => 0,
            Direction::East => 1,
            Direction::South => 2,
            Direction::West => -1,
        }
    }

    pub fn reverse_quarters(self) -> i32 {
        -self.quarters()
    }
}

/// Upscales a downsampled region mask to full resolution (bilinear interpolation then
/// threshold, like `imresize` on a logical image) and returns its bounding box and the
/// mask cropped to it.
pub fn upscale_mask(dn: &Mask, full_w: usize, full_h: usize) -> Option<(crate::core::image::Rect, Mask)> {
    use crate::core::image::Rect;
    let b = dn.bbox()?;
    if b == dn.rect() && dn.count() as usize == dn.len() {
        let r = Rect::new(0, 0, full_w, full_h);
        return Some((r, Mask::filled(full_w, full_h, true)));
    }
    let sx = full_w as f64 / dn.w as f64;
    let sy = full_h as f64 / dn.h as f64;
    let x0 = (((b.x as f64) - 1.0) * sx).floor().max(0.0) as usize;
    let y0 = (((b.y as f64) - 1.0) * sy).floor().max(0.0) as usize;
    let x1 = (((b.x1() + 1) as f64) * sx).ceil().min(full_w as f64) as usize;
    let y1 = (((b.y1() + 1) as f64) * sy).ceil().min(full_h as f64) as usize;
    let cand = Rect::new(x0, y0, x1 - x0, y1 - y0);
    let val = |x: isize, y: isize| -> f64 {
        let x = x.clamp(0, dn.w as isize - 1) as usize;
        let y = y.clamp(0, dn.h as isize - 1) as usize;
        dn.data[y * dn.w + x] as u8 as f64
    };
    let mut m = Mask::filled(cand.w, cand.h, false);
    m.data.par_chunks_mut(cand.w).enumerate().for_each(|(r, row)| {
        let v = (cand.y + r) as f64 / sy + 0.5 / sy - 0.5;
        let vy = v.floor();
        let fy = v - vy;
        for (c, out) in row.iter_mut().enumerate() {
            let u = (cand.x + c) as f64 / sx + 0.5 / sx - 0.5;
            let ux = u.floor();
            let fx = u - ux;
            let (ix, iy) = (ux as isize, vy as isize);
            let top = val(ix, iy) * (1.0 - fx) + val(ix + 1, iy) * fx;
            let bot = val(ix, iy + 1) * (1.0 - fx) + val(ix + 1, iy + 1) * fx;
            *out = top * (1.0 - fy) + bot * fy > 0.5;
        }
    });
    let tight = m.bbox()?;
    Some((tight.translate(cand.x, cand.y), m.crop(tight)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscaled_mask_tracks_source() {
        let mut dn = Mask::filled(10, 8, false);
        for y in 2..5 {
            for x in 3..7 {
                dn.set(x, y, true);
            }
        }
        let (r, m) = upscale_mask(&dn, 200, 160).unwrap();
        assert!(r.x >= 55 && r.x <= 65, "{r:?}");
        assert!(r.x1() >= 135 && r.x1() <= 145, "{r:?}");
        assert_eq!(m.w, r.w);
        assert!(m.get(m.w / 2, m.h / 2));
    }
}
