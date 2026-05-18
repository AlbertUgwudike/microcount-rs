use std::{io, path::Path};

use ome_bioformats_rs::format_in::{tiff_reader::TiffReader, FormatReader};
use serde::{Deserialize, Serialize};

use crate::model::DIR_CONVERT;

struct Raw {
    pub conversion_status: ConvertStatus,
}

struct Converted {
    pub size: (usize, usize),
    down_size: (usize, usize),

    pub channel_count: usize,
    pub registration_channel: usize,
    pub cell_channel: usize,
    pub comarker_channel: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ImageMetadata {
    source_fn: String,
    img_id: String,
    img_ws_dir: String,

    pub size: (usize, usize),
    down_size: (usize, usize),

    pub channel_count: usize,
    pub registration_channel: usize,
    pub cell_channel: usize,
    pub comarker_channel: usize,

    pub conversion_status: ConvertStatus,
}

impl ImageMetadata {
    pub fn new(source_fn: &str, ws_dir: &str) -> Self {
        Self {
            source_fn: source_fn.to_owned(),
            img_id: Path::new(source_fn)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned(),
            img_ws_dir: ws_dir.to_owned(),
            size: (0, 0),
            down_size: (0, 0),
            channel_count: 0,
            cell_channel: 0,
            comarker_channel: 0,
            registration_channel: 0,
            conversion_status: ConvertStatus::Unconverted,
        }
    }

    pub fn set_metadata(&mut self) -> io::Result<()> {
        let conv_fn = self.src_fn();
        TiffReader::new(conv_fn.into()).map(|im| {
            let md = im.metadata();
            let dim_zero = md.dimensions(0).unwrap();
            self.size = (dim_zero.w as usize, dim_zero.h as usize);
            self.channel_count = if md.series_count() > 0 {
                md.series_count()
            } else {
                dim_zero.c as usize
            };
            self.registration_channel = 0;
            self.cell_channel = 1 % (1 + self.channel_count);
            self.comarker_channel = 2 % (1 + self.channel_count);
        })
    }

    pub fn src_fn(&self) -> &str {
        &self.source_fn
    }

    pub fn id(&self) -> &str {
        &self.img_id
    }

    pub fn ws_dir(&self) -> &str {
        &self.img_ws_dir
    }

    pub fn conv_fn(&self) -> String {
        format!(
            "{}/{}/{}_conv.tiff",
            self.img_ws_dir, DIR_CONVERT, self.img_id
        )
    }

    pub fn update_cell_channel(&mut self, v: String) {
        let _ = str::parse::<usize>(&v).map(|v| {
            if v < self.channel_count {
                self.cell_channel = v
            }
        });
    }

    pub fn update_comarker_channel(&mut self, v: String) {
        let _ = str::parse::<usize>(&v).map(|v| {
            if v < self.channel_count {
                self.comarker_channel = v
            }
        });
    }

    pub fn update_reg_channel(&mut self, v: String) {
        let _ = str::parse::<usize>(&v).map(|v| {
            if v < self.channel_count {
                self.registration_channel = v
            }
        });
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ConvertStatus {
    Unconverted,
    Converting(f64),
    Converted,
}

impl ConvertStatus {
    pub fn to_str(&self) -> String {
        match self {
            Self::Unconverted => "Unconverted".into(),
            Self::Converting(p) => format!("{}", p.to_string()),
            Self::Converted => "Converted".into(),
        }
    }
}
