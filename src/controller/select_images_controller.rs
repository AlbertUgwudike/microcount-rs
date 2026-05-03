use eframe::egui::{self, Color32, Context, Rect, TextureHandle, Vec2};

use crate::{
    model::{ImageMetadata, Model},
    utility::io::egui_image_from_path,
    ThreadLabel, ThreadResponse,
};

pub struct SelectImagesController {
    pub selection: std::collections::HashSet<String>,
    pub preview_image_rect: Rect,
    pub image_rect: Rect,
    pub preview_image_data: Option<TextureHandle>,
    pub image_data: Option<TextureHandle>,
    pub pos_offset: Vec2,
    pub sz_offset: Vec2,
    pub selected_img: Option<String>,
}

impl SelectImagesController {
    pub fn new() -> SelectImagesController {
        Self {
            selection: Default::default(),
            preview_image_rect: Rect::ZERO,
            image_rect: Rect::ZERO,
            preview_image_data: None,
            image_data: None,
            pos_offset: Vec2::ZERO,
            sz_offset: Vec2::new(200.0, 200.0),
            selected_img: None,
        }
    }

    pub fn add_images(&mut self, model: &mut Model) {
        model.add_images();
    }

    pub fn n_images(&self, model: &Model) -> usize {
        model.get_all_images().map(|ims| ims.len()).unwrap_or(0)
    }

    pub fn get_image<'a>(&self, model: &Model, idx: &str) -> Option<ImageMetadata> {
        model.get_image(idx)
    }

    pub fn toggle_selection(&mut self, im_md: &ImageMetadata, ctx: &Context) {
        if self.selection.contains(im_md.src_fn()) {
            self.selection.remove(im_md.src_fn());
        } else {
            self.selection.insert(im_md.src_fn().to_string());
        }
    }

    pub fn on_image_selected(&mut self, im_md: &ImageMetadata, model: &mut Model, ctx: &Context) {
        self.selected_img = Some(im_md.src_fn().to_string().clone());
        let hw = ((im_md.size.1 - 1) as u64, (im_md.size.0 - 1) as u64);
        let src_fn = im_md.src_fn().to_owned();

        self.set_place_holder(&ctx);

        let ctx = ctx.clone();

        model.dispatch_exclusive(ThreadLabel::SelectImagesLoadPreview, true, async move {
            println!("Dispatch!");
            let im = egui_image_from_path(src_fn, (0, 0), hw, 25).await;
            ctx.request_repaint();
            println!("Repaint, requested");
            ThreadResponse::SelectImagesLoadPreview(im)
        });
    }

    pub fn on_subregion_selected(
        model: &mut Model,
        ctx: &Context,
        src_fn: String,
        hw: (u64, u64),
        origin: (u64, u64),
        data: &mut Option<TextureHandle>,
    ) {
        let ctx = ctx.clone();
        Self::set_data_place_holder(data, &ctx);
        model.dispatch_exclusive(ThreadLabel::SelectImagesLoadImage, true, async move {
            let im = egui_image_from_path(src_fn, origin, hw, 1).await;
            ctx.request_repaint();
            ThreadResponse::SelectImagesLoadImage(im)
        });
    }

    fn set_place_holder(&mut self, ctx: &Context) {
        let im = egui::ColorImage::filled([1000, 1500], Color32::BLACK);
        let h = ctx.load_texture("placeholder", im, Default::default());
        self.preview_image_data = Some(h);
    }

    fn set_data_place_holder(data: &mut Option<TextureHandle>, ctx: &Context) {
        let im = egui::ColorImage::filled([1000, 1500], Color32::BLACK);
        let h = ctx.load_texture("placeholder", im, Default::default());
        *data = Some(h);
    }

    pub fn unselect_all(&mut self) {
        self.selection.clear();
    }
}
