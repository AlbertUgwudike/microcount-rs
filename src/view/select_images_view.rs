use eframe::egui;

use crate::controller::SelectImagesController;
use crate::model::Model;

pub fn ui_tab_select_images(
    model: &mut Model,
    con: &mut SelectImagesController,
    ui: &mut egui::Ui,
) {
    ui.horizontal(|ui| {
        if ui.button("Add Images").clicked() {
            model.add_images();
        }
        if ui.button("Remove Selected").clicked() {
            model.add_images();
        }
    });
    table_ui(model, con, ui);
}

fn table_ui(model: &mut Model, con: &mut SelectImagesController, ui: &mut egui::Ui) {
    use egui_extras::{Column, TableBuilder};

    let text_height = egui::TextStyle::Body
        .resolve(ui.style())
        .size
        .max(ui.spacing().interact_size.y);

    let available_height = ui.available_height();
    let mut table = TableBuilder::new(ui)
        // .striped(self.striped)
        // .resizable(self.resizable)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder())
        .column(Column::remainder())
        .column(Column::remainder())
        .column(Column::remainder())
        .column(Column::remainder())
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height);

    table = table.sense(egui::Sense::click());

    let row_count = con.n_images(model);

    table
        .header(20.0, |mut header| {
            header.col(|ui| {
                egui::Sides::new().show(
                    ui,
                    |ui| {
                        ui.strong("Image");
                    },
                    |ui| {
                        ui.strong("⬇").clicked();
                    },
                );
            });
            header.col(|ui| {
                ui.strong("Registration Channel");
            });
            header.col(|ui| {
                ui.strong("Cell Channel");
            });
            header.col(|ui| {
                ui.strong("CoMarker Channel");
            });
            header.col(|ui| {
                ui.strong("Status");
            });
        })
        .body(|body| {
            body.rows(18.0, row_count, |mut row| {
                let img = con.get_image(model, row.index());
                row.set_selected(con.selection.contains(&row.index()));
                row.set_overline(true);
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |i| i.src_fn()));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |i| "0"));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |i| "0"));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |i| "0"));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |i| "Unconverted"));
                });
            });
        });
}
