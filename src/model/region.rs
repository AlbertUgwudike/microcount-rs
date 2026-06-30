use serde::{Deserialize, Serialize};

use crate::model::{image_metadata::Converted, transformation::MaskGenerator, ImageMetadata};

#[derive(Serialize, Deserialize, Debug)]
pub struct Region {
    pub image: ImageMetadata<Converted>,
    pub id: String,
    pub mask_generator: MaskGenerator,
}
