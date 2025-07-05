use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::ImageMetadata;

#[derive(Serialize, Deserialize, Debug)]
pub struct Workspace {
    pub dir_name: String,
    images: Vec<ImageMetadata> ,
}


impl Workspace {
    pub fn new(dir_name: String) -> Workspace {
        Workspace { images: vec![], dir_name }
    }
}
