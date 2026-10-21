use crate::core::workspace::Workspace;
use crate::gui::App;

pub fn ui(app: &mut App, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        if let Some(logo) = &app.logo {
            let s = logo.size_vec2();
            let scale = (360.0 / s.x).min(1.0);
            ui.image((logo.id(), s * scale));
        } else {
            ui.heading("Microcount");
        }
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 330.0).max(0.0) / 2.0);
            if ui.add_sized([160.0, 36.0], egui::Button::new("Create Workspace")).clicked() {
                if let Some(path) = rfd::FileDialog::new().set_title("Create Microcount Workspace").set_file_name("mc_ws").save_file() {
                    match Workspace::create(&path) {
                        Ok(ws) => app.open_workspace(ws),
                        Err(e) => app.error(format!("{e:#}")),
                    }
                }
            }
            if ui.add_sized([160.0, 36.0], egui::Button::new("Load Workspace")).clicked() {
                if let Some(dir) = rfd::FileDialog::new().set_title("Select Microcount Workspace").pick_folder() {
                    match Workspace::load(&dir) {
                        Ok(ws) => app.open_workspace(ws),
                        Err(e) => app.error(format!("{e:#}")),
                    }
                }
            }
        });
        ui.add_space(16.0);
        match &app.ws {
            Some(ws) => ui.label(egui::RichText::new(ws.dir.display().to_string()).strong()),
            None => ui.label("No Workspace Selected"),
        };
        ui.add_space(30.0);
        egui::Grid::new("status").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
            ui.label("Atlas");
            ui.label(&app.atlas_status);
            ui.end_row();
            ui.label("Import formats");
            ui.label(app.formats.join(", ")).on_hover_text("Provided by the bioformats crate (Rust port of OME Bio-Formats)");
            ui.end_row();
            ui.label("Worker threads");
            ui.label(rayon::current_num_threads().to_string());
            ui.end_row();
        });
    });
}
