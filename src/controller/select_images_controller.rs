use std::path::Path;

use eframe::egui::{Context, Rect, TextureHandle, Vec2};

use crate::{
    model::{ImageMetadata, Model, Workspace},
    utility::{io::egui_image_from_path, types::ROI},
};

pub struct SelectImagesController {
    pub selection: std::collections::HashSet<usize>,
    pub scene_rect: Rect,
    pub scene_rect2: Rect,
    pub image_data: Option<TextureHandle>,
    pub image_data2: Option<TextureHandle>,
    pub pos_offset: Vec2,
    pub sz_offset: Vec2,
    pub selected_img: Option<usize>,
}

impl SelectImagesController {
    pub fn new() -> SelectImagesController {
        Self {
            selection: Default::default(),
            scene_rect: Rect::ZERO,
            scene_rect2: Rect::ZERO,
            image_data: None,
            image_data2: None,
            pos_offset: Vec2::ZERO,
            sz_offset: Vec2::new(200.0, 200.0),
            selected_img: None,
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

    pub fn toggle_selection(&mut self, idx: usize) {
        if self.selection.contains(&idx) {
            self.selection.remove(&idx);
        } else {
            self.selection.insert(idx);
        }
    }

    pub fn on_image_selected(&mut self, idx: usize, model: &mut Model, ctx: &Context) {
        self.get_image(model, idx).map(|im_md| {
            im_md.set_metadata();
            self.selected_img = Some(idx);
            let bbox = (0, 0, im_md.size.1 - 1, im_md.size.0 - 1);
            let _ = egui_image_from_path(im_md.src_fn(), bbox, 25).map(|im| {
                let h = ctx.load_texture("screenshot_demo", im, Default::default());
                self.image_data = Some(h);
            });
        });
    }
}
