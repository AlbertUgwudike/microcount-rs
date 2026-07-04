use std::{collections::HashMap, sync::Arc};

use eframe::egui::{Context, Rect, TextureHandle, Ui, Vec2};
use ome_bioformats_rs::tools::{FormatConverter, FormatDownsampler, Progress};

use crate::{
    model::{
        image_metadata::{ConvFn, DownFn, SourceFn},
        Model,
    },
    utility::{io::load_image_from_path, types::Volume},
    view::{select_images_view, view_utils::egui_display_rgb},
    ThreadLabel, ThreadResponse,
};

pub struct SelectImagesState {
    pub preview_image_rect: Rect,
    pub image_rect: Rect,
    pub pos_offset: Vec2,
    pub sz_offset: Vec2,
    pub selected_img: Option<String>,
    pub reg_hm: HashMap<String, String>,
    pub co_hm: HashMap<String, String>,
    pub cell_hm: HashMap<String, String>,
    pub selection: std::collections::HashSet<String>,
    pub preview_image_data: Option<(TextureHandle, Volume<u8>)>,
    pub image_data: Option<(TextureHandle, Volume<u8>)>,
}

impl Default for SelectImagesState {
    fn default() -> Self {
        Self {
            preview_image_rect: Rect::ZERO,
            image_rect: Rect::ZERO,
            pos_offset: Vec2::ZERO,
            sz_offset: Vec2::new(200.0, 200.0),
            selected_img: None,
            reg_hm: HashMap::new(),
            co_hm: HashMap::new(),
            cell_hm: HashMap::new(),
            selection: Default::default(),
            preview_image_data: None,
            image_data: None,
        }
    }
}

pub struct SelectController<'a> {
    model: &'a mut Model,
    pub state: &'a mut SelectImagesState,
}

impl<'a> SelectController<'a> {
    pub fn render(model: &'a mut Model, state: &'a mut SelectImagesState, ui: &mut Ui) {
        let mut con = Self { model, state };
        select_images_view::ui_tab_select_images(&mut con, ui);
    }

    pub fn add_images(&mut self) {
        let _ = self.model.add_images();
    }

    pub fn remove_images(&mut self) {
        for im_id in &self.state.selection {
            self.model.workspace.raw_images.remove(im_id);
            self.model.workspace.converted_images.remove(im_id);
            self.model.save_workspace();
        }
    }

    pub fn convert_and_downsample(&mut self) {
        for idx in self.state.selection.clone() {
            self.convert_and_downsample_img(&idx);
        }
    }

    fn convert_and_downsample_img(&mut self, im_id: &String) {
        let img = self.model.get_raw_image(im_id).unwrap();
        let im_id = im_id.to_string();
        let input = img.src_fn().to_string();
        let output_conv = img.conv_fn();
        let output_down = img.down_fn();
        let t_sender = Arc::clone(&self.model.thread_sender);
        let ctx = self.model.context.clone();

        self.model.dispatch(false, async move {
            let mut converter = FormatConverter::new(&input, &output_conv, 100).unwrap();
            let mut progress = converter.step().unwrap();
            while let Progress::Running(a, b) = progress {
                let percentage = 100.0 * a as f64 / b as f64;
                ctx.request_repaint();
                let _ = t_sender
                    .send(ThreadResponse::Convert(im_id.clone(), percentage))
                    .await;
                progress = converter.step().unwrap();
            }

            ctx.request_repaint();
            let _ = t_sender
                .send(ThreadResponse::Converted(im_id.clone()))
                .await;

            FormatDownsampler::downsample(&output_conv, &output_down, 25).unwrap();

            ctx.request_repaint();
            let _ = t_sender
                .send(ThreadResponse::Downsampled(im_id.clone()))
                .await;
        });
    }

    pub fn selection_contains(&self, id: &str) -> bool {
        self.state.selection.contains(id)
    }

    pub fn toggle_selection(&mut self, im_md: &String) {
        if self.state.selection.contains(im_md) {
            self.state.selection.remove(im_md);
        } else {
            self.state.selection.insert(im_md.to_string());
        }
    }

    pub fn reg_buffer(&mut self, im_id: &String) -> &mut String {
        let im_md = self.model.get_converted_image(im_id).unwrap();
        self.state
            .reg_hm
            .entry(im_id.clone())
            .or_insert(im_md.state.registration_channel.to_string())
    }

    pub fn co_buffer(&mut self, im_id: &String) -> &mut String {
        let im_md = self.model.get_converted_image(im_id).unwrap();
        self.state
            .co_hm
            .entry(im_id.clone())
            .or_insert(im_md.state.comarker_channel.to_string())
    }

    pub fn cell_buffer(&mut self, im_id: &String) -> &mut String {
        let im_md = self.model.get_converted_image(im_id).unwrap();
        self.state
            .cell_hm
            .entry(im_id.clone())
            .or_insert(im_md.state.cell_channel.to_string())
    }

    pub fn persist(&mut self) {
        self.persist_cell_channel_hm();
        self.persist_co_channel_hm();
        self.persist_reg_channel_hm();
        self.model.save_workspace();
        self.state.cell_hm.clear();
        self.state.co_hm.clear();
        self.state.reg_hm.clear();
    }

    fn persist_cell_channel_hm(&mut self) {
        for (im_id, s) in self.state.cell_hm.iter() {
            let im_md = self
                .model
                .workspace
                .converted_images
                .get_mut(im_id)
                .unwrap();

            im_md.update_cell_channel(s.to_string());
        }
    }

    fn persist_co_channel_hm(&mut self) {
        for (im_id, s) in self.state.co_hm.iter() {
            let im_md = self
                .model
                .workspace
                .converted_images
                .get_mut(im_id)
                .unwrap();

            im_md.update_comarker_channel(s.to_string());
        }
    }

    fn persist_reg_channel_hm(&mut self) {
        for (im_id, s) in self.state.reg_hm.iter() {
            let im_md = self
                .model
                .workspace
                .converted_images
                .get_mut(im_id)
                .unwrap();

            im_md.update_reg_channel(s.to_string());
        }
    }

    pub fn preview_image_rect(&mut self) -> &mut Rect {
        &mut self.state.preview_image_rect
    }

    pub fn image_rect(&mut self) -> &mut Rect {
        &mut self.state.image_rect
    }

    pub fn raw_img_ids(&self) -> Vec<String> {
        self.model
            .workspace
            .raw_images
            .values()
            .map(|i| i.id())
            .collect::<Vec<String>>()
    }

    pub fn con_img_ids(&mut self) -> Vec<String> {
        self.model
            .workspace
            .converted_images
            .values()
            .map(|i| i.id())
            .collect()
    }

    pub fn img_cons(&mut self) -> Vec<String> {
        self.model
            .workspace
            .raw_images
            .values()
            .map(|i| i.state.conversion_status.to_str().to_string())
            .collect::<Vec<String>>()
    }

    pub fn on_image_selected(&mut self, im_id: &String, ctx: &Context) {
        self.set_place_holder(ctx);
        self.state.selected_img = Some(im_id.clone());

        if !self.model.is_converted(im_id) {
            return;
        }

        let im_md = self.model.get_converted_image(im_id).unwrap();
        let hw = im_md.down_size();
        let hw = ((hw.0 - 1) as u64, (hw.1 - 1) as u64);
        let dir = im_md.state.direction;
        let src_fn = im_md.down_fn().to_owned();
        let ctx = ctx.clone();
        let ttx = Arc::clone(&self.model.thread_sender);

        self.model
            .dispatch_exclusive(ThreadLabel::SelectImagesLoadPreview, true, async move {
                let im = load_image_from_path(src_fn, (0, 0), hw, hw, 1, &dir)
                    .await
                    .unwrap();
                ctx.request_repaint();
                ttx.send(ThreadResponse::SelectImagesLoadPreview(im)).await;
            });
    }

    pub fn on_subregion_selected(
        &mut self,
        im_id: &String,
        hw: (u64, u64),
        origin: (u64, u64),
        ctx: &Context,
    ) {
        self.set_data_place_holder(ctx);

        let im_md = self.model.get_converted_image(im_id).unwrap();
        let ihw = im_md.size();
        let ihw = ((ihw.0 - 1) as u64, (ihw.1 - 1) as u64);
        let dir = im_md.state.direction;
        let src_fn = im_md.conv_fn().to_owned();
        let ctx = ctx.clone();
        let ttx = Arc::clone(&self.model.thread_sender);

        self.model
            .dispatch_exclusive(ThreadLabel::SelectImagesLoadImage, true, async move {
                let im = load_image_from_path(src_fn, origin, hw, ihw, 1, &dir)
                    .await
                    .unwrap();
                ctx.request_repaint();
                ttx.send(ThreadResponse::SelectImagesLoadImage(im)).await;
            });
    }

    fn set_place_holder(&mut self, ctx: &Context) {
        let im = Volume::from_elem((3, 1000, 1500), 0);
        let h = egui_display_rgb(ctx, &im);
        self.state.preview_image_data = Some((h, im));
    }

    fn set_data_place_holder(&mut self, ctx: &Context) {
        let im = Volume::from_elem((3, 1000, 1500), 0);
        let h = egui_display_rgb(ctx, &im);
        self.state.image_data = Some((h, im));
    }

    pub fn unselect_all(&mut self) {
        self.state.selection.clear();
    }
}
