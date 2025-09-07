use eframe::egui::Ui;

use crate::controller::HomeController;
use crate::model::Model;

pub fn ui_tab_home(model: &mut Model, con: &mut HomeController, ui: &mut Ui) {
    // ui.add(egui::Slider::new(&mut con.age, 0..=120).text("age"));
    // if ui.button("Increment").clicked() {
    //     con.increment_age(model);
    // }
    if ui.button("Load Workspace").clicked() {
        con.load_workspace(model);
    }
    if ui.button("Create Workspace").clicked() {
        con.create_workspace(model);
    }
    // ui.label(format!("Hello '{}', age {}", con.name, con.age));
    ui.label(format!(
        "{}",
        model
            .workspace
            .as_ref()
            .map(|w| w.dir_name.as_ref())
            .unwrap_or("None")
    ));
    // ui.image(egui::include_image!("./assets/microcount_logo.png"));
}
