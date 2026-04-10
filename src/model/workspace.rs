use std::{collections::HashMap, sync::Arc};

use eframe::egui::mutex::Mutex;
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
}
