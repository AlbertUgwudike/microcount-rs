use std::ops::Div;

use eframe::egui::{self, Color32, Rect, Scene, Ui};

use crate::{
    controller::{regions_controller::SelectionMode, RegionsController},
    view::{regions_view::Value::First, view_utils::egui_display_rgb},
};

pub fn ui_tab_regions(con: &mut RegionsController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.columns(2, |ui| {
            table_ui(con, &mut ui[0]);
            region_dashboard_ui(con, &mut ui[1])
        });

        ui.separator();

        ui.vertical(|ui| {
            image_viewer(con, ui);
        });
    });
}

fn black_box(ui: &mut Ui, name: &str, add_contents: impl FnOnce(&mut Ui) -> ()) {
    egui::containers::Window::new(name.to_string())
        .current_pos(ui.max_rect().min)
        .max_size(ui.max_rect().size())
        .min_size(ui.available_size())
        .interactable(false)
        .title_bar(false)
        .frame(
            egui::Frame::new()
                .corner_radius(0)
                .fill(Color32::BLACK)
                .outer_margin(0),
        )
        .show(ui.ctx(), add_contents);
}

fn image_viewer(con: &mut RegionsController, ui: &mut egui::Ui) {
    black_box(ui, "right", |ui| {
        let mut inner_rect = Rect::NAN;
        let mut tmp = con.scene_rect;

        let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);

        let r = scene.show(ui, &mut tmp, |ui: &mut Ui| {
            if let Some(im) = &con.image_data {
                egui_display_rgb(ui, im);
            }
            inner_rect = ui.min_rect();
        });

        con.scene_rect = tmp;

        if r.response.double_clicked() {
            con.scene_rect = inner_rect;
        }
    });
}

fn region_dashboard_ui(con: &mut RegionsController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui.button("Atlas").clicked() {
                if let Some(im_id) = &con.selected_img.clone() {
                    con.selection_mode = SelectionMode::Atlas;
                    con.on_image_selected(im_id, ui.ctx());
                }
            }

            if ui.button("Rect").clicked() {
                if let Some(im_id) = &con.selected_img.clone() {
                    con.selection_mode = SelectionMode::Rect;
                    con.on_image_selected(im_id, ui.ctx());
                }
            }
        });

        ui.separator();

        match con.selection_mode {
            SelectionMode::Atlas => atlas_selection_ui(con, ui),
            SelectionMode::Rect => rect_selection_ui(con, ui),
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
enum Value {
    First,
    Second,
    Third,
}

fn atlas_selection_ui(con: &mut RegionsController, ui: &mut egui::Ui) {
    let mut selected = First;
    let before = First;
    egui::ComboBox::from_label("Select one!")
        .selected_text(format!("{:?}", selected))
        .show_ui(ui, |ui| {
            egui::ComboBox::from_label("Select one!")
                .selected_text(format!("{:?}", selected))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut selected, Value::First, "First");
                    ui.selectable_value(&mut selected, Value::Second, "Second");
                    ui.selectable_value(&mut selected, Value::Third, "Third");
                });

            ui.selectable_value(&mut selected, Value::First, "First");
            ui.selectable_value(&mut selected, Value::Second, "Second");
            ui.selectable_value(&mut selected, Value::Third, "Third");
        });

    if selected != before {
        // Handle selection change
    }
}

fn rect_selection_ui(con: &mut RegionsController, ui: &mut egui::Ui) {}

fn table_ui(con: &mut RegionsController, ui: &mut egui::Ui) {
    use egui_extras::{Column, TableBuilder};

    let available_height = ui.available_height();
    let mut table = TableBuilder::new(ui)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder())
        .auto_shrink(false)
        .min_scrolled_height(available_height.div(5.0))
        .max_scroll_height(available_height.div(5.0));

    table = table.sense(egui::Sense::click());

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
        })
        .body(|body| {
            let img_ids = con.img_ids();
            body.rows(18.0, img_ids.len(), |mut row| {
                let idx = row.index();
                let id = &img_ids[idx];

                row.set_selected(con.selection.contains(id));
                row.set_overline(true);

                row.col(|ui| {
                    ui.label(id);
                });

                let mut modifier = false;
                let mut clicked = false;

                if row.response().clicked() {
                    clicked = true
                }

                row.response().ctx.input(|i| {
                    if i.key_down(egui::Key::Space) {
                        modifier = true;
                    }
                });

                if modifier && clicked {
                    con.toggle_selection(id);
                } else if clicked {
                    con.unselect_all();
                    con.toggle_selection(id);
                    con.on_image_selected(id, &row.response().ctx);
                }
            });
        });
}
