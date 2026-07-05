use std::{ops::Add, sync::Arc};

use eframe::egui::{Context, Rect, TextureHandle, Ui};
use strum::IntoEnumIterator;

use crate::{
    algorithm::{
        binary::grad,
        proc::{add_overlay, warp_image},
    },
    model::{
        atlas::StructureRow,
        image_metadata::{DownFn, SourceFn},
        transformation::{Direction, Laterality, MaskGenerator},
        Model, RegionKey,
    },
    utility::{
        io::load_image_from_path,
        types::{Matrix, Volume},
    },
    view::regions_view,
    ThreadLabel, ThreadResponse,
};

pub struct RegionsState {
    pub selection: std::collections::HashSet<String>,
    pub scene_rect: Rect,
    pub image_data: Option<(TextureHandle, Volume<u8>)>,
    pub selected_img: Option<String>,
    pub selection_mode: SelectionMode,
    pub region_selector_tree: RegionSelectorState,
    pub left_lut: [bool; 840],
    pub right_lut: [bool; 840],
}

impl RegionsState {
    pub fn new(s_table: &Vec<StructureRow>) -> Self {
        Self {
            selection: Default::default(),
            scene_rect: Rect::ZERO,
            image_data: None,
            selected_img: None,
            selection_mode: SelectionMode::Rect,
            region_selector_tree: RegionSelectorState::from_structure_table(s_table),
            left_lut: [false; 840],
            right_lut: [false; 840],
        }
    }
}

pub struct RegionsController<'a> {
    model: &'a mut Model,
    pub state: &'a mut RegionsState,
}

impl<'a> RegionsController<'a> {
    pub fn render(model: &'a mut Model, state: &'a mut RegionsState, ui: &mut Ui) {
        let mut con = Self { model, state };
        regions_view::ui_tab_regions(&mut con, ui);
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
        self.update_luts();
        // self.set_place_holder(ctx);

        let regions = self
            .model
            .workspace
            .regions
            .entry(im_id.clone())
            .or_insert(vec![])
            .clone();

        let im_md = self.model.get_converted_image(im_id).unwrap();
        let hw = im_md.down_size();
        let hw = ((hw.0 - 1) as u64, (hw.1 - 1) as u64);
        let dir = im_md.state.direction;
        let down_fn = im_md.down_fn().to_owned();

        let overlay = if self.state.selection_mode == SelectionMode::Atlas {
            im_md.state.registration_data.as_ref().map(|r| {
                let idx = r.slice_idx as isize;

                let slice = self.model.atlas.get_annotation_img(r.orientation, idx);
                let slice = warp_image(slice, (hw.0 as usize, hw.1 as usize), r.affine_matrix);
                let g = grad(&slice.map(|a| *a as f64)).map(|a| if *a == 0.0 { 0 } else { 1 });

                let r_slice = self.model.atlas.region_image(r.orientation, idx, &regions);
                let r_slice = warp_image(r_slice, (hw.0 as usize, hw.1 as usize), r.affine_matrix);

                g.add(&r_slice).map(|v| *v as u8)
            })
        } else {
            None
        };

        RegionsController::load_img(self.model, down_fn, hw, hw, dir, &ctx, overlay);
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

        md.dispatch_exclusive(ThreadLabel::RegionsLoadPreview, true, async move {
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

    fn update_luts(&mut self) {
        let regions = self
            .model
            .workspace
            .regions
            .get(&self.state.selected_img.as_ref().unwrap().clone());

        for i in 0..840 {
            self.state.left_lut[i] = false;
            self.state.right_lut[i] = false;
        }

        if regions.is_none() {
            return;
        }

        for region in regions.unwrap() {
            match &region.mask_generator {
                MaskGenerator::Atlas {
                    region_key,
                    laterality,
                } => {
                    if *laterality == Laterality::Left {
                        self.state.left_lut[*region_key as usize] = true;
                    } else {
                        self.state.right_lut[*region_key as usize] = true;
                    }
                }
                MaskGenerator::Whole => {}
            };
        }
    }

    pub fn on_region_selected(&mut self) {
        self.region_selected(&Laterality::Left);
        self.region_selected(&Laterality::Right);
        self.model.save_workspace();
    }

    fn region_selected(&mut self, lat: &Laterality) {
        let lut = if *lat == Laterality::Left {
            self.state.left_lut
        } else {
            self.state.right_lut
        };

        let im_id = self.state.selected_img.as_ref().unwrap().clone();

        let regions = self
            .model
            .workspace
            .regions
            .entry(im_id.clone())
            .or_insert(vec![])
            .to_vec();

        // Check for removed regions
        for region in &regions {
            match &region.mask_generator {
                MaskGenerator::Whole => {}
                MaskGenerator::Atlas {
                    region_key,
                    laterality,
                } => {
                    if *laterality == *lat && !lut[*region_key as usize] {
                        self.model.remove_region(&im_id, region_key, laterality);
                    }
                }
            };
        }

        // Check for newly added regions
        for rk in RegionKey::iter() {
            // skip regions that have either been removed (above)
            // or have not been selected
            if !lut[rk as usize] {
                continue;
            }

            let r = regions
                .iter()
                .filter_map(|r| match &r.mask_generator {
                    MaskGenerator::Atlas {
                        region_key,
                        laterality,
                    } => {
                        if *region_key == rk && *laterality == *lat {
                            Some((region_key, laterality))
                        } else {
                            None
                        }
                    }
                    MaskGenerator::Whole => None,
                })
                .nth(0);

            // region does not already exist
            if r.is_none() {
                self.model.add_region(&im_id, &rk, lat)
            }
        }
    }

    pub fn unselect_all(&mut self) {
        self.state.selection.clear();
    }

    pub fn img_ids(&self) -> Vec<String> {
        self.model
            .workspace
            .converted_images
            .values()
            .map(|i| i.id())
            .collect::<Vec<String>>()
    }
}

#[derive(PartialEq, Eq)]
pub enum SelectionMode {
    Atlas,
    Rect,
}

pub struct RegionSelectorState {
    pub key: RegionKey,
    pub open: bool,
    pub children: Vec<RegionSelectorState>,
}

impl RegionSelectorState {
    pub fn from_structure_table(s_table: &Vec<StructureRow>) -> Self {
        Self {
            key: RegionKey::Root,
            open: false,
            children: Self::create_region_selector_state(s_table, RegionKey::Root),
        }
    }

    fn create_region_selector_state(s_table: &Vec<StructureRow>, id: RegionKey) -> Vec<Self> {
        s_table
            .iter()
            .filter(|r| r.parent_structure_id as u64 == id.to_id())
            .map(|r| Self {
                key: RegionKey::from_string(&r.acronym),
                open: false,
                children: Self::create_region_selector_state(
                    s_table,
                    RegionKey::from_string(&r.acronym),
                ),
            })
            .collect()
    }
}
