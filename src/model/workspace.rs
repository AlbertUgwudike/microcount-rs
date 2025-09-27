use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::ImageMetadata;

#[derive(Serialize, Deserialize, Debug)]
pub struct Workspace {
    pub dir_name: String,
    pub images: HashMap<String, ImageMetadata>,
}

impl Workspace {
    pub fn new(dir_name: String) -> Workspace {
        Workspace {
            images: HashMap::new(),
            dir_name,
        }
    }

    pub fn n_images(&self) -> usize {
        self.images.len()
    }

    // pub fn get_image(&mut self, idx: usize) -> Option<&mut ImageMetadata> {
    //     let keys = self.images.values();
    //     self.images.get_mut(k)
    //     self.images.get_mut(idx)
    // }

    pub fn get_image(&mut self, id: &str) -> Option<&mut ImageMetadata> {
        self.images.get_mut(id)
    }
}
