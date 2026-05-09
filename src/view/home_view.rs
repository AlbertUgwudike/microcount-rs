use eframe::egui::{self, Ui};

use crate::controller::HomeController;

pub fn ui_tab_home(con: &mut HomeController, ui: &mut Ui) {
    if ui.button("Load Workspace").clicked() {
        let r = con.load_workspace();
        println!("{:?}", r);
    }

    if ui.button("Create Workspace").clicked() {
        con.create_workspace();
    }

    ui.label(format!("{}", con.state.dir_name));

    ui.image(egui::include_image!("../assets/microcount_logo.png"));
}
