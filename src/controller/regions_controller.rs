use std::{
    cell::{Ref, RefCell},
    rc::Rc,
    sync::Arc,
};

use eframe::egui::{Context, Rect, Ui};
use ndarray::Array3;

use crate::{
    algorithm::{
        binary::grad,
        proc::{add_overlay, warp_image},
    },
    model::{
        image_metadata::{DownFn, SourceFn},
        transformation::Direction,
        Model,
    },
    utility::{
        io::load_image_from_path,
        types::{Matrix, Volume},
    },
    view::regions_view,
    ThreadLabel, ThreadResponse,
};

pub struct RegionsController {
    model: Rc<RefCell<Model>>,
    pub selection: std::collections::HashSet<String>,
    pub scene_rect: Rect,
    pub image_data: Option<Volume<u8>>,
    pub selected_img: Option<String>,
    pub selection_mode: SelectionMode,
}

impl RegionsController {
    pub fn new(model: Rc<RefCell<Model>>) -> RegionsController {
        Self {
            model,
            selection: Default::default(),
            scene_rect: Rect::ZERO,
            image_data: None,
            selected_img: None,
            selection_mode: SelectionMode::Rect,
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

        let overlay = if self.selection_mode == SelectionMode::Atlas {
            im_md.state.registration_data.as_ref().map(|r| {
                let idx = r.slice_idx as isize;
                let slice = md.atlas.get_annotation_img(r.orientation, idx);
                let slice = warp_image(slice, (hw.0 as usize, hw.1 as usize), r.affine_matrix);
                grad(&slice.map(|a| *a as f64)).map(|a| if *a == 0.0 { 0 } else { 1 })
            })
        } else {
            None
        };

        RegionsController::load_img(md, down_fn, hw, hw, dir, &ctx, overlay);
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

        md.dispatch_exclusive(ThreadLabel::RegionsLoadPreview, true, async move {
            println!("Dispatch!");
            println!("({}, {})", hw.0, hw.1);
            let mut im = load_image_from_path(down_fn, (0, 0), hw, ihw, 1, &d)
                .await
                .unwrap();
            if let Some(overlay) = overlay {
                add_overlay(&mut im, overlay);
            }
            ctx.request_repaint();
            let _ = ttx.send(ThreadResponse::RegionsLoadPreview(im)).await;
        });
    }

    fn set_place_holder(&mut self) {
        let mat = Array3::from_elem((3, 1000, 1500), 0);
        self.image_data = Some(mat);
    }

    pub fn unselect_all(&mut self) {
        self.selection.clear();
    }

    pub fn img_ids(&self) -> Vec<String> {
        let md = self.model.borrow();
        md.workspace
            .converted_images
            .values()
            .map(|i| i.id())
            .collect::<Vec<String>>()
    }

    pub fn render(&mut self, ui: &mut Ui) {
        regions_view::ui_tab_regions(self, ui);
    }
}

#[derive(PartialEq, Eq)]
pub enum SelectionMode {
    Atlas,
    Rect,
}
