use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use eframe::egui::{self, Color32, Context, Rect, TextureHandle, Ui, Vec2};
use ome_bioformats_rs::tools::{FormatConverter, FormatDownsampler, Progress};

use crate::{
    model::{
        image_metadata::{ConvFn, DownFn, SourceFn},
        Model,
    },
    utility::io::egui_image_from_path,
    view::ui_tab_select_images,
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
    pub preview_image_data: Option<TextureHandle>,
    pub image_data: Option<TextureHandle>,
}

impl SelectImagesState {
    pub fn new() -> Self {
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

pub struct SelectImagesController {
    model: Rc<RefCell<Model>>,
    pub state: SelectImagesState,
}

impl SelectImagesController {
    pub fn new(model: Rc<RefCell<Model>>) -> SelectImagesController {
        Self {
            model,
            state: SelectImagesState::new(),
        }
    }

    pub fn add_images(&self) {
        let _ = self.model.borrow_mut().add_images();
    }

    pub fn remove_images(&self) {
        let mut md = self.model.borrow_mut();
        for im_id in &self.state.selection {
            md.workspace.raw_images.remove(im_id);
            md.workspace.converted_images.remove(im_id);
            md.save_workspace();
        }
    }

    pub fn convert_and_downsample(&mut self) {
        for idx in self.state.selection.clone() {
            self.convert_and_downsample_img(&idx);
        }
    }

    fn convert_and_downsample_img(&mut self, im_id: &String) {
        let md = self.model.borrow_mut();
        let img = md.get_raw_image(im_id).unwrap();
        let im_id = im_id.to_string();
        let input = img.src_fn().to_string();
        let output_conv = img.conv_fn();
        let output_down = img.down_fn();
        let t_sender = Arc::clone(&md.thread_sender);
        let ctx = md.context.clone();

        md.dispatch(false, async move {
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
        let md = self.model.borrow();
        let im_md = md.get_converted_image(im_id).unwrap();
        self.state
            .reg_hm
            .entry(im_id.clone())
            .or_insert(im_md.state.registration_channel.to_string())
    }

    pub fn co_buffer(&mut self, im_id: &String) -> &mut String {
        let md = self.model.borrow();
        let im_md = md.get_converted_image(im_id).unwrap();
        self.state
            .co_hm
            .entry(im_id.clone())
            .or_insert(im_md.state.comarker_channel.to_string())
    }

    pub fn cell_buffer(&mut self, im_id: &String) -> &mut String {
        let md = self.model.borrow();
        let im_md = md.get_converted_image(im_id).unwrap();
        self.state
            .cell_hm
            .entry(im_id.clone())
            .or_insert(im_md.state.cell_channel.to_string())
    }

    pub fn persist(&mut self) {
        self.persist_cell_channel_hm();
        self.persist_co_channel_hm();
        self.persist_reg_channel_hm();
        self.model.borrow().save_workspace();
        self.state.cell_hm.clear();
        self.state.co_hm.clear();
        self.state.reg_hm.clear();
    }

    fn persist_cell_channel_hm(&self) {
        let mut model = self.model.borrow_mut();
        for (im_id, s) in self.state.cell_hm.iter() {
            let im_md = model.workspace.converted_images.get_mut(im_id).unwrap();
            im_md.update_cell_channel(s.to_string());
        }
    }

    fn persist_co_channel_hm(&self) {
        let mut model = self.model.borrow_mut();
        for (im_id, s) in self.state.co_hm.iter() {
            let im_md = model.workspace.converted_images.get_mut(im_id).unwrap();
            im_md.update_comarker_channel(s.to_string());
        }
    }

    fn persist_reg_channel_hm(&self) {
        let mut model = self.model.borrow_mut();
        for (im_id, s) in self.state.reg_hm.iter() {
            let im_md = model.workspace.converted_images.get_mut(im_id).unwrap();
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
        let md = self.model.borrow();
        md.workspace
            .raw_images
            .values()
            .map(|i| i.id())
            .collect::<Vec<String>>()
    }

    pub fn con_img_ids(&self) -> Vec<String> {
        let md = self.model.borrow();
        md.workspace
            .converted_images
            .values()
            .map(|i| i.id())
            .collect()
    }

    pub fn img_cons(&self) -> Vec<String> {
        let md = self.model.borrow();
        md.workspace
            .raw_images
            .values()
            .map(|i| i.state.conversion_status.to_str().to_string())
            .collect::<Vec<String>>()
    }

    pub fn on_image_selected(&mut self, im_id: &String, ctx: &Context) {
        self.set_place_holder(&ctx);
        self.state.selected_img = Some(im_id.clone());

        let md = self.model.borrow();
        if !md.is_converted(im_id) {
            return;
        }

        let im_md = md.get_converted_image(im_id).unwrap();
        let hw = (
            (im_md.state.down_size.1 - 1) as u64,
            (im_md.state.down_size.0 - 1) as u64,
        );
        let src_fn = im_md.down_fn().to_owned();
        let ctx = ctx.clone();
        let ttx = Arc::clone(&md.thread_sender);

        md.dispatch_exclusive(ThreadLabel::SelectImagesLoadPreview, true, async move {
            println!("Dispatch!");
            println!("({}, {})", hw.0, hw.1);
            let im = egui_image_from_path(src_fn, (0, 0), hw, 1).await;
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
        self.set_data_place_holder(&ctx);

        let md = self.model.borrow();
        let im_md = md.get_converted_image(im_id).unwrap();
        let src_fn = im_md.conv_fn().to_owned();
        let ctx = ctx.clone();
        let ttx = Arc::clone(&md.thread_sender);

        self.model.borrow().dispatch_exclusive(
            ThreadLabel::SelectImagesLoadImage,
            true,
            async move {
                let im = egui_image_from_path(src_fn, origin, hw, 1).await;
                ctx.request_repaint();
                ttx.send(ThreadResponse::SelectImagesLoadImage(im)).await;
            },
        );
    }

    fn set_place_holder(&mut self, ctx: &Context) {
        let im = egui::ColorImage::filled([1000, 1500], Color32::BLACK);
        let h = ctx.load_texture("placeholder", im, Default::default());
        self.state.preview_image_data = Some(h);
    }

    fn set_data_place_holder(&mut self, ctx: &Context) {
        let im = egui::ColorImage::filled([1000, 1500], Color32::BLACK);
        let h = ctx.load_texture("placeholder", im, Default::default());
        self.state.image_data = Some(h);
    }

    pub fn unselect_all(&mut self) {
        self.state.selection.clear();
    }

    pub fn render(&mut self, ui: &mut Ui) {
        ui_tab_select_images(self, ui);
    }
}
