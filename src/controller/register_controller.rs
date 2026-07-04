use std::sync::Arc;

use eframe::{
    egui::{Context, Rect, TextureHandle, Ui},
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
    view::{
        register_view,
        view_utils::{egui_display_gray, egui_display_rgb},
    },
    ThreadLabel, ThreadResponse,
};

pub struct RegisterState {
    pub selection: std::collections::HashSet<String>,
    pub right_scene_rect: Rect,
    pub left_scene_rect: Rect,
    pub hist_slice_data: Option<(TextureHandle, Volume<u8>)>,
    pub atlas_slice_data: Option<(TextureHandle, Matrix<u8>)>,
    pub selected_img: Option<String>,
    pub slider_pos: usize,
    pub atlas_orientation: Orientation,
    pub atlas_hex: [(f32, f32); 6],
    pub hist_hex: [(f32, f32); 6],
    pub atlas_scene_tf: TSTransform,
    pub hist_scene_tf: TSTransform,
    pub show_overlay: bool,
}

impl Default for RegisterState {
    fn default() -> Self {
        Self {
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
}

pub struct RegisterController<'a> {
    model: &'a mut Model,
    pub state: &'a mut RegisterState,
}

impl<'a> RegisterController<'a> {
    pub fn render(model: &'a mut Model, state: &'a mut RegisterState, ui: &mut Ui) {
        let mut con = Self { model, state };
        register_view::ui_tab_register(&mut con, ui);
    }

    pub fn toggle_selection(&mut self, im_md: &String) {
        if self.state.selection.contains(im_md) {
            self.state.selection.remove(im_md);
        } else {
            self.state.selection.insert(im_md.to_string());
        }
    }

    pub fn on_image_selected(&mut self, im_id: &String, ctx: &Context) {
        self.state.selected_img = Some(im_id.clone());
        self.set_place_holder(ctx);

        let im_md = self.model.get_converted_image(im_id).unwrap();
        let hw = im_md.down_size();
        let hw = ((hw.0 - 1) as u64, (hw.1 - 1) as u64);
        let dir = im_md.state.direction;
        let down_fn = im_md.down_fn().to_owned();

        if let Some(r) = &im_md.state.registration_data {
            self.state.hist_hex = r.hist_hex;
            self.state.atlas_hex = r.atlas_hex;
            self.state.slider_pos = r.slice_idx;
        } else {
            self.state.hist_hex = gen_hex(im_md.down_size());
            self.state.atlas_hex = gen_hex(self.model.atlas.size(self.state.atlas_orientation));
        }

        let overlay = if self.state.show_overlay {
            im_md.state.registration_data.as_ref().map(|r| {
                let idx = r.slice_idx as isize;
                let slice = self.model.atlas.get_annotation_img(r.orientation, idx);
                let slice = warp_image(slice, (hw.0 as usize, hw.1 as usize), r.affine_matrix);
                grad(&slice.map(|a| *a as f64)).map(|a| if *a == 0.0 { 0 } else { 1 })
            })
        } else {
            None
        };

        RegisterController::load_img(self.model, down_fn, hw, hw, dir, &ctx, overlay);
    }

    pub fn load_img(
        md: &mut Model,
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

    fn set_place_holder(&mut self, ctx: &Context) {
        let im = Volume::from_elem((3, 1000, 1500), 0);
        let h = egui_display_rgb(ctx, &im);
        self.state.hist_slice_data = Some((h, im));
    }

    pub fn unselect_all(&mut self) {
        self.state.selection.clear();
    }

    pub fn toggle_atlas_orientation(&mut self) {
        self.state.atlas_orientation = match self.state.atlas_orientation {
            Orientation::Axial => Orientation::Coronal,
            Orientation::Coronal => Orientation::Sagittal,
            Orientation::Sagittal => Orientation::Axial,
        }
    }

    pub fn on_atlas_interact(&mut self, ctx: &Context) {
        let mat = self
            .model
            .atlas
            .get_reference_img(self.state.atlas_orientation, self.state.slider_pos as isize);

        let h = egui_display_gray(ctx, &mat);
        self.state.atlas_slice_data = Some((h, mat));
    }

    pub fn n_atlas_slices(&self, orientation: Orientation) -> usize {
        self.model.atlas.n_slices(orientation)
    }

    pub fn img_ids(&self) -> Vec<String> {
        self.model
            .workspace
            .converted_images
            .values()
            .map(|i| i.id())
            .collect::<Vec<String>>()
    }

    pub fn reg_statuses(&self) -> Vec<String> {
        self.model
            .workspace
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
        let im_md = self
            .model
            .workspace
            .converted_images
            .get_mut(im_id)
            .unwrap();

        im_md.rotate();

        self.model.save_workspace();
    }

    pub fn register_button_pushed(&mut self) {
        let im_id = self.state.selected_img.as_ref().unwrap();
        let theta = register_control_points(self.state.atlas_hex, self.state.hist_hex);

        let im_md = self
            .model
            .workspace
            .converted_images
            .get_mut(im_id)
            .unwrap();

        im_md.state.registration_data = Some(RegistrationData::new(
            theta.unwrap(),
            self.state.atlas_orientation,
            self.state.slider_pos,
            self.state.hist_hex,
            self.state.atlas_hex,
        ));

        self.model.save_workspace();
    }
}
