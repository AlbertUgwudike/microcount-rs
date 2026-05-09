use std::{cell::RefCell, rc::Rc};

use eframe::{
    egui::{Context, Rect, TextureHandle, Ui, Vec2},
    emath::TSTransform,
};

use crate::{
    model::{atlas::Orientation, ImageMetadata, Model},
    utility::{imops::egui_image_from_mat, io::egui_image_from_path},
    view::{register_view, select_images_view},
};

pub struct RegisterController {
    model: Rc<RefCell<Model>>,
    pub selection: std::collections::HashSet<String>,
    pub scene_rect: Rect,
    pub scene_rect2: Rect,
    pub image_data: Option<TextureHandle>,
    pub image_data2: Option<TextureHandle>,
    pub selected_img: Option<String>,
    pub slider_pos: usize,
    pub atlas_orientation: Orientation,
    pub atlas_hex: [(f32, f32); 6],
    pub hist_hex: [(f32, f32); 6],
    pub transform: TSTransform,
    pub transform2: TSTransform,
}

impl RegisterController {
    pub fn new(model: Rc<RefCell<Model>>) -> RegisterController {
        Self {
            model,
            selection: Default::default(),
            scene_rect: Rect::ZERO,
            scene_rect2: Rect::ZERO,
            image_data: None,
            image_data2: None,
            selected_img: None,
            slider_pos: 25,
            atlas_orientation: Orientation::Axial,
            atlas_hex: [
                (30.0, 10.0),
                (70.0, 10.0),
                (95.0, 50.0),
                (70.0, 90.0),
                (30.0, 90.0),
                (5.0, 50.0),
            ],
            hist_hex: [
                (30.0, 10.0),
                (70.0, 10.0),
                (95.0, 50.0),
                (70.0, 90.0),
                (30.0, 90.0),
                (5.0, 50.0),
            ],
            transform: TSTransform {
                scaling: 1.0,
                translation: Vec2::ZERO,
            },
            transform2: TSTransform {
                scaling: 1.0,
                translation: Vec2::ZERO,
            },
        }
    }

    pub fn n_images(&self) -> usize {
        self.model
            .borrow()
            .workspace
            .as_ref()
            .map(|ws| ws.images.len())
            .unwrap_or(0)
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
        let hw = ((im_md.size.1 - 1) as u64, (im_md.size.0 - 1) as u64);
        tokio::task::block_in_place(async || {
            egui_image_from_path(im_md.src_fn().into(), (0, 0), hw, 25)
                .await
                .map(|im| {
                    let h = ctx.load_texture("screenshot_demo", im, Default::default());
                    self.image_data = Some(h);
                });
        });
    }

    pub fn unselect_all(&mut self) {
        self.selection.clear();
    }

    pub fn toggle_atlas_orientation(&mut self) {
        self.atlas_orientation = match self.atlas_orientation {
            Orientation::Axial => Orientation::Coronal,
            Orientation::Coronal => Orientation::Sagittal,
            Orientation::Sagittal => Orientation::Axial,
        }
    }

    pub fn on_atlas_interact(&mut self, model: &Model, ctx: &Context) {
        let mat = model
            .atlas
            .get_reference_img(self.atlas_orientation, self.slider_pos as isize);
        let image = egui_image_from_mat(mat);
        let h = ctx.load_texture("atlas", image, Default::default());
        self.image_data2 = Some(h);
    }

    pub fn register_button_pushed(&mut self) {
        // let moving = model
        //     .atlas
        //     .get_reference_img(self.atlas_orientation, self.slider_pos as isize)
        //     .map(|&a| a as f32);

        // self.selected_img.as_ref().map(|id| {
        //     model.get_image(&id).map(|img_md| {
        //         let bbox = (0, 0, img_md.size.1 - 1, img_md.size.0 - 1);
        //         let res = read_tiff_region(img_md.src_fn(), bbox, 25);
        //         match res {
        //             Ok(ims) => {
        //                 let fixed = array2buff(ims[0].map(|&a| a as f32));
        //                 let moving = array2buff(moving.t().to_owned());
        //                 iter_align(&moving, &fixed);
        //             }
        //             Err(_) => {}
        //         }
        //     });
        // });
    }

    pub fn render(&mut self, ui: &mut Ui) {
        let model = Rc::clone(&self.model);
        register_view::ui_tab_register(self, ui, &model.borrow());
    }
}
