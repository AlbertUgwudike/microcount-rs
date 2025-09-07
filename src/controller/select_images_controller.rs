use crate::model::{ImageMetadata, Model, Workspace};

pub struct SelectImagesController {
    pub selection: std::collections::HashSet<usize>,
}

impl SelectImagesController {
    pub fn new() -> SelectImagesController {
        Self {
            selection: Default::default(),
        }
    }

    pub fn add_images(&mut self, model: &mut Model) {
        model.add_images();
    }

    pub fn n_images(&self, model: &Model) -> usize {
        model.workspace.as_ref().map(|w| w.n_images()).unwrap_or(0)
    }

    pub fn get_image<'a>(&self, model: &'a mut Model, idx: usize) -> Option<&'a mut ImageMetadata> {
        match model.workspace.as_mut() {
            Some(ws) => ws.get_image(idx),
            None => None,
        }
    }
}
