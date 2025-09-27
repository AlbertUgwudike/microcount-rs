pub mod algorithm;
pub mod controller;
pub mod model;
pub mod utility;
pub mod view;

use eframe::egui::{self};

use crate::controller::{HomeController, RegisterController, SelectImagesController};
use crate::model::Model;
use crate::utility::io::{read_tiff_region, save_as_luma16};
use crate::view::{ui_tab_home, ui_tab_register, ui_tab_select_images};

// fn main() {
//     let img_fn = "/Users/albert/projects/microcount-rs/src/assets/test.tiff";
//     let img_fn = "/Users/albert/Downloads/example_ws/ws_converted/24_3_21_7.2_conv.tiff";
//     let out_fn = "/Users/albert/projects/microcount-rs/src/assets/test_out.tiff";
//     read_tiff_region(img_fn, (5000, 5000, 4000, 4000), 2)
//         .map(|r| {
//             save_as_luma16(&r[2], out_fn);
//         })
//         .map_err(|err| println!("{:?}", err));
// }

fn main() -> eframe::Result {
    // env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Microcount",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::<MyApp>::default())
        }),
    )
}

enum Tab {
    Home,
    SelectImages,
    Register,
    SelectRegions,
    Analyse,
}

struct MyApp {
    selected_tab: Tab,
    model: model::Model,
    home_controller: HomeController,
    select_images_controller: SelectImagesController,
    register_controller: RegisterController,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            selected_tab: Tab::Home,
            model: Model::new("/Users/albert/projects/microcount-rs/src".to_string()),
            home_controller: HomeController::new(),
            select_images_controller: SelectImagesController::new(),
            register_controller: RegisterController::new(),
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
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
                Tab::Home => ui_tab_home(&mut self.model, &mut self.home_controller, ui),
                Tab::SelectImages => {
                    ui_tab_select_images(&mut self.model, &mut self.select_images_controller, ui)
                }
                Tab::Register => {
                    ui_tab_register(&mut self.model, &mut self.register_controller, ui);
                }
                Tab::SelectRegions => {}
                Tab::Analyse => {}
            }
        });
    }
}
