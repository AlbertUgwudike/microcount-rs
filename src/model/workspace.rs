use serde::{Deserialize, Serialize};

use crate::model::ImageMetadata;

#[derive(Serialize, Deserialize, Debug)]
pub struct Workspace {
    pub dir_name: String,
    pub images: Vec<ImageMetadata>,
}

impl Workspace {
    pub fn new(dir_name: String) -> Workspace {
        Workspace {
            images: vec![],
            dir_name,
        }
    }

    pub fn n_images(&self) -> usize {
        self.images.len()
    }

    pub fn get_image(&mut self, idx: usize) -> Option<&mut ImageMetadata> {
        self.images.get_mut(idx)
    }
}
