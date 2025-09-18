use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{model::DIR_CONVERT, utility::io};

#[derive(Serialize, Deserialize, Debug)]
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

    pub fn set_metadata(&mut self) {
        let conv_fn = self.src_fn();
        let _ = io::tiff_info(&conv_fn).map(|info| {
            self.size = info.dimensions;
            self.channel_count = info.n_channels;
            self.registration_channel = 0;
            self.cell_channel = 1 % (1 + info.n_channels);
            self.comarker_channel = 2 % (1 + info.n_channels);
        });
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
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ConvertStatus {
    Unconverted,
    Converting,
    Converted,
}

impl ConvertStatus {
    pub fn to_str(&self) -> &str {
        match self {
            Self::Unconverted => "Unconverted",
            Self::Converting => "Converting",
            Self::Converted => "Concerted",
        }
    }
}
