use std::{path::Path, sync::Arc, time::Duration};

use eframe::egui::{self, Context, Rect, TextureHandle, Vec2};
use tokio::sync::Mutex;

use crate::{
    model::{ImageMetadata, Model, Workspace},
    utility::{io::egui_image_from_path, types::ROI},
    ThreadLabel,
};

pub struct SelectImagesController {
    pub selection: std::collections::HashSet<String>,
    pub preview_image_rect: Rect,
    pub image_rect: Rect,
    pub preview_image_data: Arc<Mutex<Option<TextureHandle>>>,
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
            preview_image_data: Arc::new(Mutex::new(None)),
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

    pub fn on_image_selected(&mut self, im_md: &ImageMetadata, model: &mut Model) {
        self.selected_img = Some(im_md.src_fn().to_string());
        let bbox = (0, 0, im_md.size.1 - 1, im_md.size.0 - 1);

        let id = Arc::clone(&self.preview_image_data);
        let ctx = Arc::clone(&model.frame);
        let src_fn = im_md.src_fn().to_owned();

        model.dispatch_exclusive(ThreadLabel::SelectImagesLoadPreview, true, async move {
            // for _ in 0..100 {
            //     tokio::time::sleep(Duration::from_millis(10)).await;
            // }
            // return;

            // let im = egui::ColorImage::from_rgb([1, 1], &[0u8, 0u8, 0u8]);

            // let h = ctx
            //     .lock()
            //     .await
            //     .load_texture("placeholder", im, Default::default());

            // *id.lock().await = Some(h);

            let im = egui_image_from_path(&src_fn, bbox, 25);
            let ctx = ctx.lock().await;

            if im.is_ok() {
                let im = im.unwrap();
                println!("{:?}", im.size);
                let h = ctx.load_texture("screenshot_demo", im, Default::default());

                *id.lock().await = Some(h);
            } else {
                println!("{:?}", im.err().unwrap().to_string());
            };

            ctx.request_repaint();

            println!("Repaint, requested")
        });
    }

    async fn set_place_holder(&self, ctx: Arc<Mutex<Context>>) {
        let im = egui::ColorImage::from_rgb([1, 1], &[0u8, 0u8, 0u8]);
        let h = ctx
            .lock()
            .await
            .load_texture("placeholder", im, Default::default());

        *self.preview_image_data.lock().await = Some(h);
    }

    pub fn unselect_all(&mut self) {
        self.selection.clear();
    }
}
