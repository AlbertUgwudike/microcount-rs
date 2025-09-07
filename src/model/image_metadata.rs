use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ImageMetadata {
    source_fn: String,
    img_id: String,
    img_ws_dir: String,

    size: (usize, usize),
    down_size: (usize, usize),

    pub channel_count: usize,
    pub registration_channel: usize,
    pub cell_channel: usize,
    pub comarker_channel: usize,
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
        }
    }

    pub fn set_metadata(&mut self) {}

    pub fn src_fn(&self) -> &str {
        &self.source_fn
    }

    pub fn id(&self) -> &str {
        &self.img_id
    }

    pub fn ws_dir(&self) -> &str {
        &self.img_ws_dir
    }
}
