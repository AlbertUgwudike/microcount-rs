use std::sync::Arc;

use eframe::egui::{self, Ui};

use crate::controller::HomeController;
use crate::model::Model;

pub fn ui_tab_home(model: &mut Model, con: &mut HomeController, ui: &mut Ui) {
    if ui.button("Load Workspace").clicked() {
        let r = con.load_workspace(model);
        println!("{:?}", r);
    }

    if ui.button("Create Workspace").clicked() {
        con.create_workspace(model);
    }

    ui.label(format!("{}", model.get_dir_name()));

    ui.image(egui::include_image!("../assets/microcount_logo.png"));
}
