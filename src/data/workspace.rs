use crate::data::ImageMetadata;

#[derive(Debug)]
pub struct Workspace {
    images: Vec<ImageMetadata>,
}

impl Workspace {
    pub fn new() -> Workspace {
        Workspace { images: vec![] }
    }
}
