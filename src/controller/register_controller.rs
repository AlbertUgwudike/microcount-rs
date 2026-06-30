use std::{
    cell::{Ref, RefCell},
    rc::Rc,
    sync::Arc,
};

use eframe::{
    egui::{Context, Rect, Ui, Vec2},
    emath::TSTransform,
};
use ndarray::Array3;

use crate::{
    algorithm::{
        binary::grad,
        proc::{add_overlay, register_control_points, warp_image},
    },
    model::{
        atlas::Orientation,
        image_metadata::{DownFn, SourceFn},
        transformation::{Direction, RegistrationData},
        Model,
    },
    utility::{
        imops::gen_hex,
        io::load_image_from_path,
        types::{Matrix, Volume},
    },
    view::register_view,
    ThreadLabel, ThreadResponse,
};

pub struct RegisterController {
    model: Rc<RefCell<Model>>,
    pub selection: std::collections::HashSet<String>,
    pub right_scene_rect: Rect,
    pub left_scene_rect: Rect,
    pub hist_slice_data: Option<Volume<u8>>,
    pub atlas_slice_data: Option<Matrix<u8>>,
    pub selected_img: Option<String>,
    pub slider_pos: usize,
    pub atlas_orientation: Orientation,
    pub atlas_hex: [(f32, f32); 6],
    pub hist_hex: [(f32, f32); 6],
    pub atlas_scene_tf: TSTransform,
    pub hist_scene_tf: TSTransform,
    pub show_overlay: bool,
}

impl RegisterController {
    pub fn new(model: Rc<RefCell<Model>>) -> RegisterController {
        Self {
            model,
            selection: Default::default(),
            right_scene_rect: Rect::ZERO,
            left_scene_rect: Rect::ZERO,
            hist_slice_data: None,
            atlas_slice_data: None,
            selected_img: None,
            slider_pos: 25,
            atlas_orientation: Orientation::Axial,
            atlas_hex: [(0.0, 0.0); 6],
            hist_hex: [(0.0, 0.0); 6],
            atlas_scene_tf: TSTransform::default(),
            hist_scene_tf: TSTransform::default(),
            show_overlay: true,
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
        self.set_place_holder();

        let md = self.model.borrow();
        let im_md = md.get_converted_image(im_id).unwrap();
        let hw = im_md.down_size();
        let hw = ((hw.0 - 1) as u64, (hw.1 - 1) as u64);
        let dir = im_md.state.direction;
        let down_fn = im_md.down_fn().to_owned();

        if let Some(r) = &im_md.state.registration_data {
            self.hist_hex = r.hist_hex;
            self.atlas_hex = r.atlas_hex;
            self.slider_pos = r.slice_idx;
        } else {
            self.hist_hex = gen_hex(im_md.down_size());
            self.atlas_hex = gen_hex(md.atlas.size(self.atlas_orientation));
        }

        let overlay = if self.show_overlay {
            im_md.state.registration_data.as_ref().map(|r| {
                let idx = r.slice_idx as isize;
                let slice = md.atlas.get_annotation_img(r.orientation, idx);
                let slice = warp_image(slice, (hw.0 as usize, hw.1 as usize), r.affine_matrix);
                grad(&slice.map(|a| *a as f64)).map(|a| if *a == 0.0 { 0 } else { 1 })
            })
        } else {
            None
        };

        RegisterController::load_img(md, down_fn, hw, hw, dir, &ctx, overlay);
    }

    pub fn load_img(
        md: Ref<'_, Model>,
        down_fn: String,
        hw: (u64, u64),
        ihw: (u64, u64),
        d: Direction,
        ctx: &Context,
        overlay: Option<Matrix<u8>>,
    ) {
        let ttx = Arc::clone(&md.thread_sender);
        let ctx = ctx.clone();

        md.dispatch_exclusive(ThreadLabel::RegisterLoadPreview, true, async move {
            println!("Dispatch!");
            println!("({}, {})", hw.0, hw.1);
            let mut im = load_image_from_path(down_fn, (0, 0), hw, ihw, 1, &d)
                .await
                .unwrap();
            if let Some(overlay) = overlay {
                add_overlay(&mut im, overlay);
            }
            ctx.request_repaint();
            let _ = ttx.send(ThreadResponse::RegisterLoadPreview(im)).await;
        });
    }

    fn set_place_holder(&mut self) {
        let mat = Array3::from_elem((3, 1000, 1500), 0);
        self.hist_slice_data = Some(mat);
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

    pub fn on_atlas_interact(&mut self) {
        let model = self.model.borrow();
        let mat = model
            .atlas
            .get_reference_img(self.atlas_orientation, self.slider_pos as isize);
        self.atlas_slice_data = Some(mat);
    }

    pub fn n_atlas_slices(&self, orientation: Orientation) -> usize {
        let model = self.model.borrow();
        model.atlas.n_slices(orientation)
    }

    pub fn img_ids(&self) -> Vec<String> {
        let md = self.model.borrow();
        md.workspace
            .converted_images
            .values()
            .map(|i| i.id())
            .collect::<Vec<String>>()
    }

    pub fn reg_statuses(&self) -> Vec<String> {
        let md = self.model.borrow();
        md.workspace
            .converted_images
            .values()
            .map(|im| {
                if im.state.registration_data.is_none() {
                    "Not Registered".into()
                } else {
                    "Registered".into()
                }
            })
            .collect::<Vec<String>>()
    }

    pub fn rotate_image(&mut self, im_id: &String) {
        let mut md = self.model.borrow_mut();
        let im_md = md.workspace.converted_images.get_mut(im_id).unwrap();
        im_md.rotate();

        md.save_workspace();
    }

    pub fn register_button_pushed(&mut self) {
        let im_id = self.selected_img.as_ref().unwrap();
        let theta = register_control_points(self.atlas_hex, self.hist_hex);
        let mut model = self.model.borrow_mut();
        let im_md = model.workspace.converted_images.get_mut(im_id).unwrap();

        im_md.state.registration_data = Some(RegistrationData::new(
            theta.unwrap(),
            self.atlas_orientation,
            self.slider_pos,
            self.hist_hex,
            self.atlas_hex,
        ));

        model.save_workspace();
    }

    pub fn render(&mut self, ui: &mut Ui) {
        register_view::ui_tab_register(self, ui);
    }
}
