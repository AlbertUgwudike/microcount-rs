use std::{
    cell::{Ref, RefCell},
    rc::Rc,
    sync::Arc,
};

use eframe::{
    egui::{self, Color32, Context, Rect, TextureHandle, Ui, Vec2},
    emath::TSTransform,
};

use crate::{
    model::{
        atlas::Orientation,
        image_metadata::{Converted, DownFn, SourceFn},
        transformation::Direction,
        ImageMetadata, Model,
    },
    utility::{imops::egui_image_from_mat, io::egui_image_from_path},
    view::register_view,
    ThreadLabel, ThreadResponse,
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

    pub fn toggle_selection(&mut self, im_md: &String) {
        if self.selection.contains(im_md) {
            self.selection.remove(im_md);
        } else {
            self.selection.insert(im_md.to_string());
        }
    }

    pub fn on_image_selected(&mut self, im_id: &String, ctx: &Context) {
        self.selected_img = Some(im_id.clone());
        self.set_place_holder(&ctx);
        let md = self.model.borrow();
        if md.is_converted(im_id) {
            let im_md = md.get_converted_image(im_id).unwrap();
            let hw = im_md.down_size();
            let hw = ((hw.0 - 1) as u64, (hw.1 - 1) as u64);
            let dir = im_md.state.direction;
            let down_fn = im_md.down_fn().to_owned();
            RegisterController::load_img(md, down_fn, hw, hw, dir, &ctx);
        } else if md.is_registered(im_id) {
            let im_md = md.get_registered_image(im_id).unwrap();
            let hw = im_md.down_size();
            let hw = ((hw.0 - 1) as u64, (hw.1 - 1) as u64);
            let dir = im_md.state.direction;
            let down_fn = im_md.down_fn().to_owned();
            RegisterController::load_img(md, down_fn, hw, hw, dir, &ctx);
        }
    }

    pub fn load_img(
        md: Ref<'_, Model>,
        down_fn: String,
        hw: (u64, u64),
        ihw: (u64, u64),
        d: Direction,
        ctx: &Context,
    ) {
        let ttx = Arc::clone(&md.thread_sender);
        let ctx = ctx.clone();

        md.dispatch_exclusive(ThreadLabel::RegisterLoadPreview, true, async move {
            println!("Dispatch!");
            println!("({}, {})", hw.0, hw.1);
            let im = egui_image_from_path(down_fn, (0, 0), hw, ihw, 1, &d).await;
            ctx.request_repaint();
            ttx.send(ThreadResponse::RegisterLoadPreview(im)).await;
        });
    }

    fn set_place_holder(&mut self, ctx: &Context) {
        let im = egui::ColorImage::filled([1000, 1500], Color32::BLACK);
        let h = ctx.load_texture("placeholder", im, Default::default());
        self.image_data2 = Some(h);
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

    pub fn on_atlas_interact(&mut self, ctx: &Context) {
        let model = self.model.borrow();
        let mat = model
            .atlas
            .get_reference_img(self.atlas_orientation, self.slider_pos as isize);
        let image = egui_image_from_mat(mat);
        let h = ctx.load_texture("atlas", image, Default::default());
        self.image_data2 = Some(h);
    }

    pub fn n_atlas_slices(&self, orientation: Orientation) -> usize {
        let model = self.model.borrow();
        model.atlas.n_slices(orientation)
    }

    pub fn img_ids(&self) -> Vec<String> {
        let md = self.model.borrow();
        vec![
            md.workspace
                .converted_images
                .values()
                .map(|i| i.id())
                .collect::<Vec<String>>(),
            md.workspace
                .registered_images
                .values()
                .map(|i| i.id())
                .collect(),
        ]
        .concat()
    }

    pub fn reg_statuses(&self) -> Vec<String> {
        let md = self.model.borrow();
        vec![
            md.workspace
                .converted_images
                .values()
                .map(|_| "Not Registered".into())
                .collect::<Vec<String>>(),
            md.workspace
                .registered_images
                .values()
                .map(|i| i.state.registration_status.to_str().to_string())
                .collect(),
        ]
        .concat()
    }

    pub fn rotate_image(&mut self, im_id: &String) {
        let mut md = self.model.borrow_mut();

        if md.is_registered(im_id) {
            let im_md = md.workspace.registered_images.get_mut(im_id).unwrap();
            im_md.rotate();
        } else if md.is_converted(im_id) {
            let im_md = md.workspace.converted_images.get_mut(im_id).unwrap();
            im_md.rotate();
        }

        md.save_workspace();
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
        register_view::ui_tab_register(self, ui);
    }
}
