use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::model::{
    image_metadata::{Converted, Raw, Registered, Unregistered},
    ImageMetadata,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Workspace {
    pub dir_name: String,
    pub raw_images: HashMap<String, ImageMetadata<Raw>>,
    pub converted_images: HashMap<String, ImageMetadata<Converted<Unregistered>>>,
    pub registered_images: HashMap<String, ImageMetadata<Converted<Registered>>>,
}

impl Workspace {
    pub fn new(dir_name: String) -> Workspace {
        Workspace {
            raw_images: HashMap::new(),
            converted_images: HashMap::new(),
            registered_images: HashMap::new(),
            dir_name,
        }
    }

    pub fn default() -> Workspace {
        Workspace {
            raw_images: HashMap::new(),
            converted_images: HashMap::new(),
            registered_images: HashMap::new(),
            dir_name: String::new(),
        }
    }
}
