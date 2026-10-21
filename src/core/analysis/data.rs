//! Segmentation output of one region and its summary (`MicrocountData`,
//! `data2result.m`, `plaque_partition.m`, `Utility.write_tables`).

use super::cells::Cells;
use crate::core::image::{Image, Mask};
use crate::core::imgproc::stats::{mean, mean_omit_nan, perc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

pub struct Data {
    /// Cell-marker image shown in the output (one channel, or RGB for DAB stains).
    pub iba1: Arc<Vec<Image<u16>>>,
    /// Co-marker image shown in the output.
    pub cd68: Arc<Image<u16>>,
    pub cd68_mask: Arc<Mask>,
    pub soma_mask: Mask,
    pub cells: Cells,
    /// Pixels in the cell-marker foreground (somas ∪ processes).
    pub iba1_area_px: u64,
    pub overlap_pcs: Vec<f64>,
    pub n_activated: usize,
    pub n_microglia: usize,
    /// Pixels inside the region mask.
    pub n_pixels: u64,
}

/// Summary statistics of one region; field names are the CSV export columns.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MicrocountResult {
    #[serde(with = "crate::core::nan_serde")]
    pub cell_density_per_mm2: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub percentage_cell_area: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub percentage_comarker_area: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub percentage_comarker_num: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_convexity: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_soma_size_um2: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_branch_point_count: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_branch_length_um_per_branch: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_scholl_index: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_branch_area_um2: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub average_branch_length_per_cell_um: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub percentage_branch_area: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub process_length_um_per_mm2: f64,
    #[serde(with = "crate::core::nan_serde")]
    pub tissue_area_mm2: f64,
    #[serde(default)]
    pub cell_count: usize,
}

impl MicrocountResult {
    pub const COLUMNS: [&'static str; 15] = [
        "cell_density_per_mm2",
        "percentage_cell_area",
        "percentage_comarker_area",
        "percentage_comarker_num",
        "average_convexity",
        "average_soma_size_um2",
        "average_branch_point_count",
        "average_branch_length_um_per_branch",
        "average_scholl_index",
        "average_branch_area_um2",
        "average_branch_length_per_cell_um",
        "percentage_branch_area",
        "process_length_um_per_mm2",
        "tissue_area_mm2",
        "cell_count",
    ];

    pub fn values(&self) -> [f64; 15] {
        [
            self.cell_density_per_mm2,
            self.percentage_cell_area,
            self.percentage_comarker_area,
            self.percentage_comarker_num,
            self.average_convexity,
            self.average_soma_size_um2,
            self.average_branch_point_count,
            self.average_branch_length_um_per_branch,
            self.average_scholl_index,
            self.average_branch_area_um2,
            self.average_branch_length_per_cell_um,
            self.percentage_branch_area,
            self.process_length_um_per_mm2,
            self.tissue_area_mm2,
            self.cell_count as f64,
        ]
    }
}

/// Per-cell tables written to the Scholl workbook.
pub struct Tables {
    pub scholl: Vec<f64>,
    pub crossings: Vec<Vec<u32>>,
    pub branch_lengths: Vec<Vec<u32>>,
    pub branch_counts: Vec<f64>,
    pub soma_sizes: Vec<f64>,
    pub rotundities: Vec<f64>,
    pub cell_areas: Vec<f64>,
    pub activations: Vec<f64>,
    pub branch_areas: Vec<f64>,
    pub branch_length_av: Vec<f64>,
}

/// `data2result.m` (statistics part). `pixel_dims` is `[height, width]` in µm.
pub fn summarise(d: &Data, pixel_dims: [f64; 2]) -> (MicrocountResult, Tables) {
    let mm2_per_px = pixel_dims[0] * pixel_dims[1] / 1e6;
    let um_per_px = (pixel_dims[0] + pixel_dims[1]) / 2.0;
    let um2_per_px = pixel_dims[0] * pixel_dims[1];
    let c = &d.cells;
    let n = d.n_microglia as f64;

    let image_area = d.n_pixels as f64 * mm2_per_px;
    let activated_area = d.cd68_mask.count() as f64 * mm2_per_px;
    let iba1_area = d.iba1_area_px as f64 * mm2_per_px;
    let soma_area_um2 = d.soma_mask.count() as f64 * mm2_per_px * 1e6;
    let total_branch_points = d.cells.detected.count() as f64;
    let n_branches: usize = c.branch_lengths.iter().map(|b| b.len()).sum();
    let total_length: f64 = c.branch_lengths.iter().flatten().map(|&l| l as f64).sum();
    let branch_areas: Vec<f64> = c.cell_areas.iter().zip(&c.soma_areas).map(|(a, s)| a - s).collect();
    let av_per_cell: Vec<f64> = c
        .branch_lengths
        .iter()
        .map(|b| b.iter().map(|&l| l as f64).sum::<f64>() / b.len() as f64)
        .collect();
    let branch_area_fraction = branch_areas.iter().sum::<f64>() / d.n_pixels as f64;

    let result = MicrocountResult {
        cell_density_per_mm2: perc(n, image_area) / 100.0,
        percentage_cell_area: perc(iba1_area, image_area),
        percentage_comarker_area: perc(activated_area, iba1_area),
        percentage_comarker_num: perc(d.n_activated as f64, n),
        average_convexity: mean_omit_nan(&c.rotundities),
        average_soma_size_um2: perc(soma_area_um2, n) / 100.0,
        average_branch_point_count: perc(total_branch_points, n) / 100.0,
        average_branch_length_um_per_branch: total_length / n_branches as f64 * um_per_px,
        average_scholl_index: mean_omit_nan(&c.scholl),
        average_branch_area_um2: mean(&branch_areas) * um2_per_px,
        average_branch_length_per_cell_um: total_length / n * um_per_px,
        percentage_branch_area: perc(branch_area_fraction, 1.0),
        process_length_um_per_mm2: total_length * um_per_px / image_area,
        tissue_area_mm2: image_area,
        cell_count: d.n_microglia,
    };
    let tables = Tables {
        scholl: c.scholl.clone(),
        crossings: c.crossings.clone(),
        branch_lengths: c.branch_lengths.clone(),
        branch_counts: c.branch_counts.clone(),
        soma_sizes: c.soma_areas.clone(),
        rotundities: c.rotundities.clone(),
        cell_areas: c.cell_areas.clone(),
        activations: d.overlap_pcs.clone(),
        branch_areas,
        branch_length_av: av_per_cell,
    };
    (result, tables)
}

fn pick<T: Clone>(v: &[T], keep: &[bool]) -> Vec<T> {
    v.iter().zip(keep).filter(|(_, &k)| k).map(|(x, _)| x.clone()).collect()
}

/// `plaque_partition.m`: splits cells into those overlapping a plaque (`in_plaque`) and
/// those that do not. Unlike the MATLAB version the cell and activation counts are those
/// of the subset rather than of all cells.
pub fn partition(d: &Data, in_plaque: bool, min_overlap: f64) -> Data {
    let keep: Vec<bool> = d.overlap_pcs.iter().map(|&p| if in_plaque { p > 0.0 } else { p == 0.0 }).collect();
    let mut relabel = vec![0u32; d.cells.n_cells + 1];
    let mut next = 1;
    for (i, &k) in keep.iter().enumerate() {
        if k {
            relabel[i + 1] = next;
            next += 1;
        }
    }
    let map = |l: u32| relabel.get(l as usize).copied().unwrap_or(0);
    let segmented = d.cells.segmented.map(map);
    let kept = |img: &Image<u32>| img.map(map);
    let c = &d.cells;
    let keep_seed: Vec<bool> = (0..c.n_seeds).map(|i| keep.get(i).copied().unwrap_or(false)).collect();
    let keep_scholl: Vec<bool> = (0..c.scholl.len()).map(|i| keep.get(i).copied().unwrap_or(false)).collect();
    let in_subset = segmented.map(|l| l > 0);
    let overlap_pcs = pick(&d.overlap_pcs, &keep);
    let n_activated = overlap_pcs.iter().filter(|&&p| p >= min_overlap).count();
    let cells = Cells {
        n_seeds: keep_seed.iter().filter(|&&k| k).count(),
        n_cells: next as usize - 1,
        cell_areas: pick(&c.cell_areas, &keep_seed),
        soma_areas: pick(&c.soma_areas, &keep_seed),
        rotundities: pick(&c.rotundities, &keep_seed),
        poly: kept(&c.poly),
        detected: c.detected.and(&in_subset),
        branch_counts: pick(&c.branch_counts, &keep),
        skeleton: kept(&c.skeleton),
        branch_lengths: pick(&c.branch_lengths, &keep_seed),
        scholl: pick(&c.scholl, &keep_scholl),
        crossings: pick(&c.crossings, &keep_scholl),
        segmented,
    };
    Data {
        iba1: d.iba1.clone(),
        cd68: d.cd68.clone(),
        cd68_mask: d.cd68_mask.clone(),
        soma_mask: d.soma_mask.and(&in_subset),
        iba1_area_px: d.iba1_area_px,
        n_microglia: cells.n_cells,
        cells,
        overlap_pcs,
        n_activated,
        n_pixels: d.n_pixels,
    }
}

/// Writes the per-cell tables as one worksheet each.
pub fn write_tables(t: &Tables, path: &Path) -> Result<()> {
    use rust_xlsxwriter::Workbook;
    let mut wb = Workbook::new();
    let column = |wb: &mut Workbook, name: &str, v: &[f64]| -> Result<()> {
        let ws = wb.add_worksheet().set_name(name)?;
        for (i, &x) in v.iter().enumerate() {
            if x.is_finite() {
                ws.write_number(i as u32, 0, x)?;
            } else {
                ws.write_string(i as u32, 0, "NaN")?;
            }
        }
        Ok(())
    };
    let rows = |wb: &mut Workbook, name: &str, v: &[Vec<u32>]| -> Result<()> {
        let ws = wb.add_worksheet().set_name(name)?;
        for (i, r) in v.iter().enumerate() {
            for (j, &x) in r.iter().enumerate() {
                ws.write_number(i as u32, j as u16, x as f64)?;
            }
        }
        Ok(())
    };
    column(&mut wb, "Scholl Coefficients", &t.scholl)?;
    rows(&mut wb, "Branch Lengths", &t.branch_lengths)?;
    column(&mut wb, "Branch Counts", &t.branch_counts)?;
    rows(&mut wb, "Scholl Crossings", &t.crossings)?;
    column(&mut wb, "Soma Sizes", &t.soma_sizes)?;
    column(&mut wb, "Rotundities", &t.rotundities)?;
    column(&mut wb, "Cell Areas", &t.cell_areas)?;
    column(&mut wb, "Overlap Per Cell", &t.activations)?;
    column(&mut wb, "Branch Areas", &t.branch_areas)?;
    column(&mut wb, "Average Branch Length per Cell", &t.branch_length_av)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    wb.save(path)?;
    Ok(())
}
