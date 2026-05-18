pub mod algorithm;
pub mod concurrency;
pub mod controller;
pub mod model;
pub mod utility;
pub mod view;

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use eframe::egui::{self, ColorImage, Context};
use ome_bioformats_rs::convert::format_converter::ConverterProgress;
use ome_bioformats_rs::convert::FormatConverter;
use tokio::sync::mpsc::Receiver;

use crate::concurrency::ThreadPool;
use crate::controller::{HomeController, RegisterController, SelectImagesController};
use crate::model::{ConvertStatus, Model};
use crate::view::{ui_tab_home, ui_tab_register, ui_tab_select_images};

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
}

pub enum ThreadResponse {
    SelectImagesLoadPreview(io::Result<ColorImage>),
    SelectImagesLoadImage(io::Result<ColorImage>),
    Convert(String, f64),
}

enum Tab {
    Home,
    SelectImages,
    Register,
    SelectRegions,
    Analyse,
}

struct MyApp {
    reciever: Receiver<ThreadResponse>,
    model: Rc<RefCell<Model>>,
    selected_tab: Tab,
    home_controller: HomeController,
    select_images_controller: SelectImagesController,
    register_controller: RegisterController,
}

impl MyApp {
    fn new(frame: Context) -> Self {
        let dir_name = "/Users/albert/projects/microcount-rs/src".into();
        let tp = ThreadPool::new(10, 10);
        let model = Rc::new(RefCell::new(Model::new(
            dir_name,
            tp.sender,
            tp.thread_sender,
        )));

        Self {
            reciever: tp.reciever,
            selected_tab: Tab::Home,
            home_controller: HomeController::new(Rc::clone(&model)),
            select_images_controller: SelectImagesController::new(Rc::clone(&model)),
            register_controller: RegisterController::new(Rc::clone(&model)),
            model,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(msg) = self.reciever.try_recv() {
            match msg {
                ThreadResponse::SelectImagesLoadPreview(res) => {
                    let h = ctx.load_texture("screenshot_demo", res.unwrap(), Default::default());
                    self.select_images_controller.state.preview_image_data = Some(h)
                }
                ThreadResponse::SelectImagesLoadImage(res) => {
                    let h = ctx.load_texture("screenshot_demo2", res.unwrap(), Default::default());
                    self.select_images_controller.state.image_data = Some(h)
                }
                ThreadResponse::Convert(im_id, progress) => {
                    let mut md = self.model.borrow_mut();
                    md.workspace.as_mut().map(|ws| {
                        let im_md = ws.images.get_mut(&im_id).unwrap();
                        im_md.conversion_status = ConvertStatus::Converting(progress);
                    });
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Home").clicked() {
                    self.selected_tab = Tab::Home;
                }
                if ui.button("Select Images").clicked() {
                    self.selected_tab = Tab::SelectImages;
                }
                if ui.button("Register").clicked() {
                    self.selected_tab = Tab::Register;
                }
                if ui.button("Select Regions").clicked() {
                    self.selected_tab = Tab::SelectRegions;
                }
                if ui.button("Analyse").clicked() {
                    self.selected_tab = Tab::Analyse;
                }
            });

            ui.separator();

            match self.selected_tab {
                Tab::Home => self.home_controller.render(ui),
                Tab::SelectImages => self.select_images_controller.render(ui),
                Tab::Register => self.register_controller.render(ui),
                Tab::SelectRegions => {}
                Tab::Analyse => {}
            }
        });
    }
}
