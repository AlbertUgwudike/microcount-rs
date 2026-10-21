//! Segmentation and morphometry of glial cells.

pub mod algorithms;
pub mod cells;
pub mod data;
pub mod render;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Algorithm {
    #[default]
    MicroFluor,
    MicroDab,
    MicroPlaque,
    AstroFluor,
    AstroDab,
    AstroSingle,
}

impl Algorithm {
    pub const ALL: [Algorithm; 6] = [
        Algorithm::MicroFluor,
        Algorithm::MicroDab,
        Algorithm::AstroFluor,
        Algorithm::AstroDab,
        Algorithm::MicroPlaque,
        Algorithm::AstroSingle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Algorithm::MicroFluor => "Fluorescent Microglia",
            Algorithm::MicroDab => "DAB-Stained Microglia",
            Algorithm::AstroFluor => "Fluorescent Astrocytes",
            Algorithm::AstroDab => "DAB-Stained Astrocytes",
            Algorithm::MicroPlaque => "Fluorescent Microglia + Plaques",
            Algorithm::AstroSingle => "Single Astrocyte",
        }
    }

    pub fn cli_name(self) -> &'static str {
        match self {
            Algorithm::MicroFluor => "micro-fluor",
            Algorithm::MicroDab => "micro-dab",
            Algorithm::AstroFluor => "astro-fluor",
            Algorithm::AstroDab => "astro-dab",
            Algorithm::MicroPlaque => "micro-plaque",
            Algorithm::AstroSingle => "astro-single",
        }
    }
}

/// Thresholds of one region (`Region.m` properties) and the image pixel size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegionSettings {
    /// Cell-marker branch threshold (MATLAB `Iba1Threshold`).
    pub cell_threshold: f64,
    /// Co-marker threshold (`CD68Threshold`).
    pub comarker_threshold: f64,
    /// Largest co-marker object kept, in pixels (`MaxCD68Size`).
    pub max_comarker_size: u64,
    /// Minimum co-marker overlap for a cell to count as positive, % (`OverlapPercentage`).
    pub overlap_percentage: f64,
    pub soma_threshold: f64,
}

impl Default for RegionSettings {
    fn default() -> Self {
        Self {
            cell_threshold: 0.35,
            comarker_threshold: 0.5,
            max_comarker_size: 10_000,
            overlap_percentage: 2.0,
            soma_threshold: 0.45,
        }
    }
}

/// Everything an algorithm needs besides the pixels (`MicrocountSettings.m`).
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// `[height, width]` of a pixel in µm.
    pub pixel_dims: [f64; 2],
    pub cell_threshold: f64,
    pub comarker_threshold: f64,
    pub max_comarker_size: u64,
    pub min_overlap: f64,
    pub soma_threshold: f64,
}

impl Settings {
    pub fn new(r: &RegionSettings, pixel_dims: [f64; 2]) -> Self {
        Self {
            pixel_dims,
            cell_threshold: r.cell_threshold,
            comarker_threshold: r.comarker_threshold,
            max_comarker_size: r.max_comarker_size,
            min_overlap: r.overlap_percentage,
            soma_threshold: r.soma_threshold,
        }
    }
}
