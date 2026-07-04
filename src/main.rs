pub mod algorithm;
pub mod concurrency;
pub mod controller;
pub mod model;
pub mod utility;
pub mod view;

use eframe::egui::{self, Context};
use tokio::sync::mpsc::Receiver;

use crate::concurrency::ThreadPool;
use crate::controller::home_controller::HomeState;
use crate::controller::regions_controller::RegionsState;
use crate::controller::register_controller::RegisterState;
use crate::controller::select_images_controller::SelectImagesState;
use crate::controller::{HomeController, RegionsController, RegisterController, SelectController};
use crate::model::{ConvertStatus, Model};
use crate::utility::types::Volume;
use crate::view::view_utils::egui_display_rgb;

#[tokio::main]
async fn main() -> eframe::Result {
    // env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Microcount",
        options,
        Box::new(|cc| {
            let frame = cc.egui_ctx.clone();
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(MyApp::new(frame)))
        }),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ThreadLabel {
    SelectImagesLoadPreview,
    SelectImagesLoadImage,
    RegisterLoadPreview,
    RegionsLoadPreview,
}

pub enum ThreadResponse {
    SelectImagesLoadPreview(Volume<u8>),
    SelectImagesLoadImage(Volume<u8>),
    Convert(String, f64),
    Converted(String),
    Downsampled(String),
    RegisterLoadPreview(Volume<u8>),
    RegionsLoadPreview(Volume<u8>),
}

enum Tab {
    Home,
    Select,
    Register,
    Regions,
    Analyse,
}

struct MyApp {
    reciever: Receiver<ThreadResponse>,
    model: Model,
    selected_tab: Tab,
    home_state: HomeState,
    select_images_state: SelectImagesState,
    register_state: RegisterState,
    regions_state: RegionsState,
}

impl MyApp {
    fn new(context: Context) -> Self {
        let dir_name = "/Users/albert/projects/microcount-rs/src".into();
        let tp = ThreadPool::new(10, 10);
        let model = Model::new(dir_name, tp.sender, tp.thread_sender, context);

        Self {
            reciever: tp.reciever,
            selected_tab: Tab::Home,
            home_state: HomeState::new(model.get_dir_name()),
            select_images_state: SelectImagesState::default(),
            register_state: RegisterState::default(),
            regions_state: RegionsState::default(),
            model,
        }
    }

    fn workspace_loaded(&self) -> bool {
        self.model.workspace_loaded
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(msg) = self.reciever.try_recv() {
            match msg {
                ThreadResponse::SelectImagesLoadPreview(res) => {
                    let h = egui_display_rgb(ctx, &res);
                    self.select_images_state.preview_image_data = Some((h, res));
                }
                ThreadResponse::SelectImagesLoadImage(res) => {
                    let h = egui_display_rgb(ctx, &res);
                    self.select_images_state.image_data = Some((h, res));
                }
                ThreadResponse::Convert(im_id, progress) => {
                    let im_md = self.model.workspace.raw_images.get_mut(&im_id).unwrap();
                    im_md.state.conversion_status = ConvertStatus::Converting(progress);
                }
                ThreadResponse::Converted(im_id) => {
                    let im_md = self.model.workspace.raw_images.get_mut(&im_id).unwrap();
                    im_md.state.conversion_status = ConvertStatus::Converted;
                }
                ThreadResponse::Downsampled(im_id) => {
                    let _ = self.model.raw_to_converted(im_id);
                }
                ThreadResponse::RegisterLoadPreview(res) => {
                    let h = egui_display_rgb(ctx, &res);
                    self.register_state.hist_slice_data = Some((h, res));
                }
                ThreadResponse::RegionsLoadPreview(res) => {
                    let h = egui_display_rgb(ctx, &res);
                    self.regions_state.image_data = Some((h, res))
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Home").clicked() {
                    self.selected_tab = Tab::Home;
                }
                if ui.button("Select Images").clicked() {
                    self.selected_tab = Tab::Select;
                }
                if ui.button("Register").clicked() {
                    self.selected_tab = Tab::Register;
                }
                if ui.button("Select Regions").clicked() {
                    self.selected_tab = Tab::Regions;
                }
                if ui.button("Analyse").clicked() {
                    self.selected_tab = Tab::Analyse;
                }
            });

            ui.separator();

            if !self.workspace_loaded() {
                return HomeController::render(&mut self.model, &mut self.home_state, ui);
            }

            let md = &mut self.model;

            match self.selected_tab {
                Tab::Home => HomeController::render(md, &mut self.home_state, ui),
                Tab::Select => SelectController::render(md, &mut self.select_images_state, ui),
                Tab::Register => RegisterController::render(md, &mut self.register_state, ui),
                Tab::Regions => RegionsController::render(md, &mut self.regions_state, ui),
                Tab::Analyse => {}
            }
        });
    }
}
