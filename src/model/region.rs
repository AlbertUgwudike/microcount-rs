use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::model::transformation::MaskGenerator;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Region {
    pub id: String,
    pub image_id: String,
    pub mask_generator: MaskGenerator,
    pub needs_reprocess: bool,
}
