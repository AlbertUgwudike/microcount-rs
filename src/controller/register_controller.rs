use std::path::Path;

use eframe::egui::{Context, Rect, TextureHandle, Vec2};

use crate::{
    model::{ImageMetadata, Model, Workspace},
    utility::{io::egui_image_from_path, types::ROI},
};

pub struct RegisterController {
    pub selection: std::collections::HashSet<String>,
    pub scene_rect: Rect,
    pub scene_rect2: Rect,
    pub image_data: Option<TextureHandle>,
    pub image_data2: Option<TextureHandle>,
    pub selected_img: Option<String>,
    pub slider_pos: usize,
}

impl RegisterController {
    pub fn new() -> RegisterController {
        Self {
            selection: Default::default(),
            scene_rect: Rect::ZERO,
            scene_rect2: Rect::ZERO,
            image_data: None,
            image_data2: None,
            selected_img: None,
            slider_pos: 25,
        }
    }

    pub fn n_images(&self, model: &Model) -> usize {
        model.workspace.as_ref().map(|w| w.n_images()).unwrap_or(0)
    }

    pub fn get_image<'a>(&self, model: &'a mut Model, idx: &str) -> Option<&'a mut ImageMetadata> {
        match model.workspace.as_mut() {
            Some(ws) => ws.get_image(idx),
            None => None,
        }
    }

    pub fn toggle_selection(&mut self, im_md: &ImageMetadata, ctx: &Context) {
        if self.selection.contains(im_md.src_fn()) {
            self.selection.remove(im_md.src_fn());
        } else {
            self.selection.insert(im_md.src_fn().to_string());
        }
    }

    pub fn on_image_selected(&mut self, im_md: &ImageMetadata, ctx: &Context) {
        self.selected_img = Some(im_md.src_fn().to_string());
        let bbox = (0, 0, im_md.size.1 - 1, im_md.size.0 - 1);
        let _ = egui_image_from_path(im_md.src_fn(), bbox, 25).map(|im| {
            let h = ctx.load_texture("screenshot_demo", im, Default::default());
            self.image_data = Some(h);
        });
    }

    pub fn unselect_all(&mut self) {
        self.selection.clear();
    }
}
