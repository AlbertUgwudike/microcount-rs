//! Workspace model (`Workspace.m`, `ImageMetadata.m`, `Region.m`, `TransformationData.m`)
//! persisted as `ws.json` in the workspace folder, next to the same sub-folders the
//! MATLAB version used.

use crate::core::analysis::data::MicrocountResult;
use crate::core::analysis::{Algorithm, RegionSettings};
use crate::core::registration::gen_hex;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DIR_CONVERT: &str = "ws_converted";
pub const DIR_DOWN: &str = "ws_downsampled";
pub const DIR_PROC: &str = "ws_processed";
pub const DIR_MASK: &str = "ws_masks";
pub const DIR_SCHOLL: &str = "ws_scholl";
pub const WS_FILE: &str = "ws.json";
/// Padding (downsampled pixels) around the histology during registration.
pub const PAD: usize = 100;
/// Downsampling factor of the registration images.
pub const DOWNSAMPLE: usize = 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvertStatus {
    #[default]
    Unconverted,
    Converting,
    Converted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStatus {
    #[default]
    Unprocessed,
    Queued,
    Processing,
    Processed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn rotate(self) -> Self {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
        }
    }

    pub fn is_sideways(self) -> bool {
        matches!(self, Direction::East | Direction::West)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Axial,
    Sagittal,
    Coronal,
}

impl Orientation {
    pub fn cycle(self) -> Self {
        match self {
            Orientation::Axial => Orientation::Sagittal,
            Orientation::Sagittal => Orientation::Coronal,
            Orientation::Coronal => Orientation::Axial,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Laterality {
    Left,
    Right,
    Bilat,
}

impl Laterality {
    pub fn name(self) -> &'static str {
        match self {
            Laterality::Left => "LEFT",
            Laterality::Right => "RIGHT",
            Laterality::Bilat => "BILAT",
        }
    }
}

/// Atlas alignment of one image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformData {
    pub atlas_hex: [[f64; 2]; 6],
    pub hist_hex: [[f64; 2]; 6],
    /// Atlas slice coordinates → padded, rotated downsampled image coordinates.
    pub matrix: [[f64; 3]; 3],
    /// `(rows, cols)` of the padded, unrotated downsampled image.
    pub image_size: [usize; 2],
    /// 1-based atlas slice index.
    pub slice_idx: usize,
    pub direction: Direction,
    pub orientation: Orientation,
}

impl TransformData {
    /// `TransformationData.default`.
    pub fn default_for(down_size: [usize; 2], atlas_size: (usize, usize), n_slices: usize, dir: Direction, ori: Orientation) -> Self {
        let image_size = [down_size[0] + 2 * PAD, down_size[1] + 2 * PAD];
        let mut td = TransformData {
            atlas_hex: gen_hex(atlas_size.0, atlas_size.1),
            hist_hex: [[0.0; 2]; 6],
            matrix: crate::core::registration::Affine::IDENTITY.0,
            image_size,
            slice_idx: (n_slices / 2).max(1),
            direction: dir,
            orientation: ori,
        };
        let (r, c) = td.rotated_size();
        td.hist_hex = gen_hex(r, c);
        td
    }

    /// `(rows, cols)` of the padded image as displayed (after rotation).
    pub fn rotated_size(&self) -> (usize, usize) {
        let [r, c] = self.image_size;
        if self.direction.is_sideways() {
            (c, r)
        } else {
            (r, c)
        }
    }
}

/// Characters MATLAB's `RegionKey.from_string` replaces with `_`.
pub fn sanitize_key(key: &str) -> String {
    key.chars().map(|c| if matches!(c, ' ' | ',' | '-' | '/') { '_' } else { c }).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Atlas acronym, or `WI` for the whole image.
    pub key: String,
    pub laterality: Laterality,
    pub settings: RegionSettings,
    pub status: ProcessStatus,
    pub result: Option<MicrocountResult>,
    /// Cells outside plaques (plaque algorithm only).
    #[serde(default)]
    pub result_out_plaque: Option<MicrocountResult>,
    /// Bounding box of the region in the downsampled image (the thumbnail extent).
    #[serde(default)]
    pub dn_rect: Option<crate::core::image::Rect>,
    /// Bounding box at full resolution (the extent of the processed image).
    #[serde(default)]
    pub crop: Option<crate::core::image::Rect>,
}

impl Region {
    pub fn new(key: &str, laterality: Laterality) -> Self {
        let laterality = if key == crate::core::atlas::WHOLE_IMAGE { Laterality::Bilat } else { laterality };
        Region {
            key: key.to_string(),
            laterality,
            settings: RegionSettings::default(),
            status: ProcessStatus::Unprocessed,
            result: None,
            result_out_plaque: None,
            dn_rect: None,
            crop: None,
        }
    }

    /// `Region.generate_id`.
    pub fn id(&self, image_id: &str) -> String {
        format!("{image_id}__{}__{}", sanitize_key(&self.key), self.laterality.name())
    }

    pub fn location(&self) -> String {
        format!("{}--{}", self.laterality.name(), self.key)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageEntry {
    pub id: String,
    pub source: PathBuf,
    pub status: ConvertStatus,
    #[serde(skip)]
    pub progress: f32,
    /// `(rows, cols)` at full resolution.
    pub size: [usize; 2],
    pub down_size: [usize; 2],
    pub channel_count: usize,
    /// `[height, width]` of a pixel in µm.
    pub pixel_dims: [f64; 2],
    /// 1-based channel indices.
    pub registration_channel: usize,
    pub cell_channel: usize,
    pub co_channel: usize,
    pub aligned: bool,
    pub whole_aligned: bool,
    pub transform: Option<TransformData>,
    pub regions: Vec<Region>,
}

impl ImageEntry {
    pub fn new(source: PathBuf) -> Self {
        let id = source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        ImageEntry {
            id,
            source,
            status: ConvertStatus::Unconverted,
            progress: 0.0,
            size: [0, 0],
            down_size: [0, 0],
            channel_count: 0,
            pixel_dims: [1.0, 1.0],
            registration_channel: 1,
            cell_channel: 1,
            co_channel: 1,
            aligned: false,
            whole_aligned: false,
            transform: None,
            regions: vec![],
        }
    }

    /// Default channel roles (`set_metadata`): registration 1, cell 2, co-marker 3
    /// (wrapping when there are fewer channels).
    pub fn set_default_channels(&mut self) {
        let n = self.channel_count.max(1);
        self.registration_channel = 1;
        self.cell_channel = 1 + 1 % n;
        self.co_channel = 1 + 2 % n;
    }

    pub fn valid_channels(&self, chans: [usize; 3]) -> bool {
        chans.iter().all(|&c| c >= 1 && c <= self.channel_count)
    }
}

/// File locations inside a workspace folder.
#[derive(Clone, Debug)]
pub struct WsPaths {
    pub dir: PathBuf,
}

impl WsPaths {
    pub fn conv_path(&self, id: &str) -> PathBuf {
        self.dir.join(DIR_CONVERT).join(format!("{id}_conv.tiff"))
    }

    pub fn down_path(&self, id: &str) -> PathBuf {
        self.dir.join(DIR_DOWN).join(format!("{id}_down.tiff"))
    }

    pub fn proc_path(&self, region_id: &str) -> PathBuf {
        self.dir.join(DIR_PROC).join(format!("{region_id}.tiff"))
    }

    pub fn mask_path(&self, region_id: &str) -> PathBuf {
        self.dir.join(DIR_MASK).join(format!("{region_id}.tiff"))
    }

    pub fn scholl_path(&self, region_id: &str) -> PathBuf {
        self.dir.join(DIR_SCHOLL).join(format!("{region_id}.xlsx"))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(skip)]
    pub dir: PathBuf,
    pub version: u32,
    pub algorithm: Algorithm,
    pub images: Vec<ImageEntry>,
}

impl Workspace {
    pub fn create(dir: &Path) -> Result<Workspace> {
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!("{} is not empty; choose an empty or new folder", dir.display());
        }
        for sub in ["", DIR_CONVERT, DIR_DOWN, DIR_PROC, DIR_MASK, DIR_SCHOLL] {
            std::fs::create_dir_all(dir.join(sub)).with_context(|| format!("creating {}", dir.display()))?;
        }
        let ws = Workspace { dir: dir.to_path_buf(), version: 1, ..Default::default() };
        ws.save()?;
        Ok(ws)
    }

    pub fn load(dir: &Path) -> Result<Workspace> {
        let file = dir.join(WS_FILE);
        if !file.is_file() {
            if dir.join("ws.mat").is_file() {
                bail!(
                    "{} is a MATLAB Microcount workspace (ws.mat). Create a new workspace and re-add the images.",
                    dir.display()
                );
            }
            bail!("{} is not a Microcount workspace (no {WS_FILE})", dir.display());
        }
        let text = std::fs::read_to_string(&file)?;
        let mut ws: Workspace = serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))?;
        ws.dir = dir.to_path_buf();
        for sub in [DIR_CONVERT, DIR_DOWN, DIR_PROC, DIR_MASK, DIR_SCHOLL] {
            std::fs::create_dir_all(dir.join(sub)).ok();
        }
        // Work interrupted by closing the app is not running any more.
        for img in &mut ws.images {
            if img.status == ConvertStatus::Converting {
                img.status = ConvertStatus::Unconverted;
            }
            for r in &mut img.regions {
                if matches!(r.status, ProcessStatus::Processing | ProcessStatus::Queued) {
                    r.status = if r.result.is_some() { ProcessStatus::Processed } else { ProcessStatus::Unprocessed };
                }
            }
        }
        Ok(ws)
    }

    /// Atomically writes `ws.json`.
    pub fn save(&self) -> Result<()> {
        let tmp = self.dir.join(format!("{WS_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, self.dir.join(WS_FILE))?;
        Ok(())
    }

    pub fn paths(&self) -> WsPaths {
        WsPaths { dir: self.dir.clone() }
    }

    pub fn image(&self, id: &str) -> Option<&ImageEntry> {
        self.images.iter().find(|i| i.id == id)
    }

    pub fn image_mut(&mut self, id: &str) -> Option<&mut ImageEntry> {
        self.images.iter_mut().find(|i| i.id == id)
    }

    /// Adds images, skipping file names already present (`io_add_image`).
    pub fn add_images(&mut self, paths: &[PathBuf]) -> Vec<String> {
        let mut skipped = vec![];
        for p in paths {
            let entry = ImageEntry::new(p.clone());
            if self.images.iter().any(|i| i.id == entry.id) {
                skipped.push(entry.id);
            } else {
                self.images.push(entry);
            }
        }
        skipped
    }

    /// `(image index, region index)` of a region id.
    pub fn find_region(&self, region_id: &str) -> Option<(usize, usize)> {
        self.images.iter().enumerate().find_map(|(ii, img)| {
            img.regions.iter().position(|r| r.id(&img.id) == region_id).map(|ri| (ii, ri))
        })
    }

    /// All regions in display order as `(image index, region index)`.
    pub fn all_regions(&self) -> Vec<(usize, usize)> {
        self.images
            .iter()
            .enumerate()
            .flat_map(|(ii, img)| (0..img.regions.len()).map(move |ri| (ii, ri)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_ids_match_matlab() {
        let r = Region::new("CUL4, 5", Laterality::Left);
        assert_eq!(r.id("slide1"), "slide1__CUL4__5__LEFT");
        assert_eq!(Region::new("WI", Laterality::Right).id("a"), "a__WI__BILAT");
    }

    #[test]
    fn workspace_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mc_ws_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut ws = Workspace::create(&dir).unwrap();
        ws.add_images(&[PathBuf::from("/x/a.lif"), PathBuf::from("/y/a.lif"), PathBuf::from("/y/b.czi")]);
        assert_eq!(ws.images.len(), 2);
        let mut r = Region::new("CA1", Laterality::Right);
        r.result = Some(MicrocountResult { average_convexity: f64::NAN, ..Default::default() });
        ws.images[0].regions.push(r);
        ws.save().unwrap();
        let back = Workspace::load(&dir).unwrap();
        assert!(back.images[0].regions[0].result.as_ref().unwrap().average_convexity.is_nan());
        std::fs::remove_dir_all(&dir).ok();
    }
}
