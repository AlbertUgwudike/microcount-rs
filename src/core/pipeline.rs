//! Region-level operations: masks, full processing runs, previews and exports
//! (`Model.bg_run_microcount`, `AnalyseController.process_preview`,
//! `Model.io_export_results`, `Model.io_export_regions`).

use crate::core::analysis::algorithms::{self, compute_norm, Norm, RegionInput};
use crate::core::analysis::data::{summarise, write_tables, MicrocountResult};
use crate::core::analysis::render::{Renderer, PAGE_CELL, PAGE_CELL_POLY, PAGE_CO, PAGE_CO_POLY, PAGE_SOMA};
use crate::core::analysis::{Algorithm, Settings};
use crate::core::atlas::{upscale_mask, Atlas};
use crate::core::image::{apply_mask, Image, Mask, Rect};
use crate::core::imageio::{self, ImageFile};
use crate::core::workspace::{ImageEntry, Region, WsPaths};
use crate::core::Cancel;
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::time::Instant;

/// A region mask at full resolution, cropped to its bounding box.
pub struct FullMask {
    pub rect: Rect,
    pub mask: Mask,
    /// Bounding box of the downsampled mask (the thumbnail's extent).
    pub dn_rect: Rect,
    /// Full-resolution pixels per downsampled pixel, `(x, y)`.
    pub scale: (f64, f64),
}

impl FullMask {
    /// Maps a rectangle in thumbnail (downsampled crop) pixels to full-resolution pixels
    /// relative to `rect`, clipped to it.
    pub fn thumb_to_full(&self, x: f64, y: f64, w: f64, h: f64) -> Rect {
        let fx = |v: f64| (self.dn_rect.x as f64 + v) * self.scale.0 - self.rect.x as f64;
        let fy = |v: f64| (self.dn_rect.y as f64 + v) * self.scale.1 - self.rect.y as f64;
        let clamp = |v: f64, n: usize| v.round().clamp(0.0, n as f64) as usize;
        let (x0, y0) = (clamp(fx(x), self.rect.w), clamp(fy(y), self.rect.h));
        let (x1, y1) = (clamp(fx(x + w), self.rect.w), clamp(fy(y + h), self.rect.h));
        Rect::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
    }
}

pub fn dn_mask(atlas: &Atlas, img: &ImageEntry, region: &Region) -> Result<Mask> {
    atlas.region_mask(img.transform.as_ref(), (img.down_size[0], img.down_size[1]), &region.key, region.laterality)
}

pub fn full_mask(atlas: &Atlas, img: &ImageEntry, region: &Region) -> Result<FullMask> {
    let dn = dn_mask(atlas, img, region)?;
    let dn_rect = dn.bbox().ok_or_else(|| anyhow!("region {} does not intersect this section", region.key))?;
    let (rect, mask) =
        upscale_mask(&dn, img.size[1], img.size[0]).ok_or_else(|| anyhow!("region {} is empty", region.key))?;
    let scale = (img.size[1] as f64 / dn.w as f64, img.size[0] as f64 / dn.h as f64);
    Ok(FullMask { rect, mask, dn_rect, scale })
}

/// Writes the region thumbnail (`add_region_save_mask`): the cell and co-marker
/// channels of the downsampled image inside the region, cropped to it. Returns the
/// crop rectangle.
pub fn write_mask_thumbnail(paths: &WsPaths, atlas: &Atlas, img: &ImageEntry, region: &Region) -> Result<Rect> {
    let dn = dn_mask(atlas, img, region)?;
    let bbox = dn.bbox().ok_or_else(|| anyhow!("region {} does not intersect this section", region.key))?;
    let down = ImageFile::open(paths.down_path(&img.id))?;
    let mask = dn.crop(bbox);
    let chan = |c: usize| -> Result<Image<u16>> {
        let mut m = down.read_channel(c.saturating_sub(1), bbox)?;
        apply_mask(&mut m, &mask, 0);
        Ok(m)
    };
    let cell = chan(img.cell_channel)?;
    let co = chan(img.co_channel)?;
    let zero = Image::filled(bbox.w, bbox.h, 0u16);
    imageio::write_pages(&paths.mask_path(&region.id(&img.id)), &[vec![&cell, &co, &zero]], None)?;
    Ok(bbox)
}

/// Maps a rectangle in thumbnail pixels to full-resolution pixels relative to the
/// region crop, given the rectangles stored on the region.
pub fn thumb_to_crop(img: &ImageEntry, dn_rect: Rect, crop: Rect, t: [f64; 4]) -> Rect {
    let fm = FullMask {
        rect: crop,
        mask: Mask::filled(0, 0, false),
        dn_rect,
        scale: (img.size[1] as f64 / img.down_size[1] as f64, img.size[0] as f64 / img.down_size[0] as f64),
    };
    fm.thumb_to_full(t[0], t[1], t[2], t[3])
}

pub fn load_input(algo: Algorithm, tf: &ImageFile, img: &ImageEntry, rect: Rect, mask: Mask) -> Result<RegionInput> {
    let needs = algo.needs();
    let n_pixels = mask.count();
    let read = |c: usize| tf.read_channel(c.saturating_sub(1), rect);
    Ok(RegionInput {
        cell: if needs.cell { Some(read(img.cell_channel)?) } else { None },
        co: if needs.co { Some(read(img.co_channel)?) } else { None },
        rgb: if needs.rgb { Some(tf.read_plane::<u16>(0, rect)?) } else { None },
        mask,
        n_pixels,
        full_range: tf.full_range(),
    })
}

pub struct RegionOutcome {
    pub crop: Rect,
    pub result: MicrocountResult,
    pub result_out_plaque: Option<MicrocountResult>,
    pub seconds: f64,
}

pub fn suffixed(path: &Path, suffix: &str) -> std::path::PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    path.with_file_name(format!("{stem}{suffix}{ext}"))
}

/// Processes one region end to end and writes its output image and tables.
pub fn process_region(
    paths: &WsPaths,
    atlas: &Atlas,
    img: &ImageEntry,
    region: &Region,
    algo: Algorithm,
    cancel: &Cancel,
) -> Result<RegionOutcome> {
    let t0 = Instant::now();
    let rid = region.id(&img.id);
    let fm = full_mask(atlas, img, region)?;
    let tf = ImageFile::open(paths.conv_path(&img.id))?;
    log::info!("{rid}: {}×{} px crop, {} px in region", fm.rect.w, fm.rect.h, fm.mask.count());
    let fm_rect = fm.rect;
    let input = load_input(algo, &tf, img, fm.rect, fm.mask)?;
    cancel.check()?;
    let settings = Settings::new(&region.settings, img.pixel_dims);
    let data = algorithms::run(algo, &input, &settings, None, cancel)?;
    drop(input);
    cancel.check()?;
    let (proc_path, scholl_path) = (paths.proc_path(&rid), paths.scholl_path(&rid));
    let (main, result_out_plaque) = if algo == Algorithm::MicroPlaque {
        let (inside, outside) = algorithms::split_plaque(&data, &settings);
        drop(data);
        let (res_out, tables_out) = summarise(&outside, img.pixel_dims);
        write_tables(&tables_out, &suffixed(&scholl_path, "_out_plaque"))?;
        Renderer::new(&outside).write_tiff(&suffixed(&proc_path, "_out_plaque"), img.pixel_dims)?;
        (inside, Some(res_out))
    } else {
        (data, None)
    };
    let (result, tables) = summarise(&main, img.pixel_dims);
    write_tables(&tables, &scholl_path).context("writing tables")?;
    cancel.check()?;
    Renderer::new(&main).write_tiff(&proc_path, img.pixel_dims).context("writing output image")?;
    Ok(RegionOutcome { crop: fm_rect, result, result_out_plaque, seconds: t0.elapsed().as_secs_f64() })
}

/// Pages shown in the analysis view, in display order: cell marker, co-marker, cell
/// outlines, co-marker outlines, soma outlines.
pub const VIEW_PAGES: [usize; 5] = [PAGE_CELL, PAGE_CO, PAGE_CELL_POLY, PAGE_CO_POLY, PAGE_SOMA];

pub struct Preview {
    pub region_id: String,
    /// Requested rectangle in thumbnail pixels.
    pub thumb: [f64; 4],
    /// Full-resolution bounding box of the region.
    pub crop: Rect,
    /// Rectangle relative to the region crop.
    pub rect: Rect,
    pub pages: Vec<[Image<u16>; 3]>,
    pub result: MicrocountResult,
}

/// Normalisation of a whole region, reused while previewing it with other thresholds.
#[derive(Clone)]
pub struct NormCache {
    pub key: String,
    pub norm: Norm,
}

pub fn norm_key(img: &ImageEntry, region: &Region, algo: Algorithm) -> String {
    format!("{}|{algo:?}|{}|{}", region.id(&img.id), img.cell_channel, img.co_channel)
}

/// Runs the algorithm on the part of the region under `thumb` (thumbnail pixels
/// `[x, y, w, h]`) using normalisation from the whole region (`process_preview`).
pub fn preview(
    paths: &WsPaths,
    atlas: &Atlas,
    img: &ImageEntry,
    region: &Region,
    algo: Algorithm,
    thumb: [f64; 4],
    cache: Option<NormCache>,
    cancel: &Cancel,
) -> Result<(Preview, NormCache)> {
    let fm = full_mask(atlas, img, region)?;
    let rel = fm.thumb_to_full(thumb[0], thumb[1], thumb[2], thumb[3]);
    let tf = ImageFile::open(paths.conv_path(&img.id))?;
    let key = norm_key(img, region, algo);
    let cache = match cache {
        Some(c) if c.key == key => c,
        _ => {
            log::info!("computing normalisation over the whole region for the preview");
            let full = load_input(algo, &tf, img, fm.rect, fm.mask.clone())?;
            NormCache { key, norm: compute_norm(algo, &full)? }
        }
    };
    cancel.check()?;
    let rel = rel.intersect(&fm.mask.rect());
    if rel.is_empty() {
        bail!("preview rectangle lies outside the region");
    }
    let sub_mask = fm.mask.crop(rel);
    let input = load_input(algo, &tf, img, rel.translate(fm.rect.x, fm.rect.y), sub_mask)?;
    if input.n_pixels == 0 {
        bail!("preview rectangle lies outside the region");
    }
    let settings = Settings::new(&region.settings, img.pixel_dims);
    let mut data = algorithms::run(algo, &input, &settings, Some(&cache.norm), cancel)?;
    if algo == Algorithm::MicroPlaque {
        data = algorithms::split_plaque(&data, &settings).0;
    }
    let (result, _) = summarise(&data, img.pixel_dims);
    let r = Renderer::new(&data);
    let full = Rect::new(0, 0, rel.w, rel.h);
    let pages = VIEW_PAGES.iter().map(|&p| r.page(p, full)).collect();
    Ok((Preview { region_id: region.id(&img.id), thumb, crop: fm.rect, rect: rel, pages, result }, cache))
}

/// Reads the view pages of a processed region's output image inside `rel`.
pub fn read_processed(paths: &WsPaths, region_id: &str, rel: Rect) -> Result<Vec<[Image<u16>; 3]>> {
    let tf = ImageFile::open(paths.proc_path(region_id))?;
    let rel = rel.intersect(&tf.rect());
    if rel.is_empty() {
        bail!("view outside processed image");
    }
    VIEW_PAGES
        .iter()
        .map(|&page| {
            let mut c = tf.read_plane::<u16>(page, rel)?;
            if c.len() < 3 {
                let z = c[0].clone();
                c.resize(3, z);
            }
            let (c2, c1, c0) = (c.pop().unwrap(), c.pop().unwrap(), c.pop().unwrap());
            Ok([c0, c1, c2])
        })
        .collect()
}

/// `Utility.region2export` + `writetable`.
pub fn export_csv(paths: &WsPaths, rows: &[(&ImageEntry, &Region)], path: &Path) -> Result<()> {
    let mut w = csv::Writer::from_path(path).with_context(|| format!("writing {}", path.display()))?;
    let mut header = vec![
        "file_name",
        "mask_file_name",
        "region_ID",
        "branch_threshold",
        "comarker_threshold",
        "soma_threshold",
        "max_comarker_size",
        "min_comarker_overlap_percentage",
    ];
    header.extend(MicrocountResult::COLUMNS);
    w.write_record(&header)?;
    let num = |v: f64| if v.is_finite() { format!("{v}") } else { "NaN".to_string() };
    for (img, r) in rows {
        let rid = r.id(&img.id);
        let mut put = |id: String, proc: String, res: &MicrocountResult| -> Result<()> {
            let s = &r.settings;
            let mut rec = vec![
                proc,
                paths.mask_path(&rid).display().to_string(),
                id,
                num(s.cell_threshold),
                num(s.comarker_threshold),
                num(s.soma_threshold),
                s.max_comarker_size.to_string(),
                num(s.overlap_percentage),
            ];
            rec.extend(res.values().iter().map(|&v| num(v)));
            w.write_record(&rec)?;
            Ok(())
        };
        let proc = paths.proc_path(&rid);
        if let Some(res) = &r.result {
            put(rid.clone(), proc.display().to_string(), res)?;
        }
        if let Some(res) = &r.result_out_plaque {
            put(format!("{rid}_out_plaque"), suffixed(&proc, "_out_plaque").display().to_string(), res)?;
        }
    }
    w.flush()?;
    Ok(())
}

/// Writes every region of `images` as a masked 3-channel TIFF (cell, co-marker,
/// registration) into `dir` (`io_export_regions`).
pub fn export_regions(paths: &WsPaths, atlas: &Atlas, images: &[ImageEntry], dir: &Path, cancel: &Cancel) -> Result<usize> {
    let mut n = 0;
    for img in images {
        let tf = ImageFile::open(paths.conv_path(&img.id))?;
        for region in &img.regions {
            cancel.check()?;
            let fm = full_mask(atlas, img, region)?;
            let read = |c: usize| -> Result<Image<u16>> {
                let mut m = tf.read_channel(c.saturating_sub(1), fm.rect)?;
                apply_mask(&mut m, &fm.mask, 0);
                Ok(m)
            };
            let (cell, co, reg) = (read(img.cell_channel)?, read(img.co_channel)?, read(img.registration_channel)?);
            let out = dir.join(format!("{}.tiff", region.id(&img.id)));
            imageio::write_pages(&out, &[vec![&cell, &co, &reg]], Some(img.pixel_dims))?;
            log::info!("exported {}", out.display());
            n += 1;
        }
    }
    Ok(n)
}
