use std::ops::Div;

use eframe::egui::{self, Color32, Pos2, Rect, Scene, Sense, Stroke, TextureHandle, Ui, Vec2};

use crate::controller::RegisterController;
use crate::model::Model;
use crate::utility::imops::egui_image_from_mat;
use crate::utility::io::egui_image_from_path;

pub fn ui_tab_register(model: &mut Model, con: &mut RegisterController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        table_ui(model, con, ui);

        ui.separator();

        image_viewer(model, con, ui);
    });
}

fn black_box(ui: &mut Ui, name: &str, add_contents: impl FnOnce(&mut Ui) -> ()) {
    egui::containers::Window::new(name.to_string())
        .current_pos(ui.max_rect().min)
        .max_size(ui.available_size())
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

fn image_viewer(model: &mut Model, con: &mut RegisterController, ui: &mut egui::Ui) {
    ui.columns(2, |ui| {
        black_box(&mut ui[0], "left", |ui| {
            ui.vertical(|ui| {
                let mut inner_rect = Rect::NAN;

                let f = |ui: &mut Ui| {
                    if let Some(im) = &con.image_data2 {
                        ui.image(im);
                    }

                    inner_rect = ui.min_rect();
                };

                let response = Scene::new()
                    .zoom_range(0.0..=f32::INFINITY)
                    .show(ui, &mut con.scene_rect2, f)
                    .response;

                if response.double_clicked() {
                    con.scene_rect2 = inner_rect;
                }

                let slider = egui::Slider::new(&mut con.slider_pos, 0..=50);
                let res = ui.add(slider).interact(Sense::click_and_drag());

                if res.dragged() {
                    let mat = model.atlas.get_reference_img(
                        crate::model::atlas::Orientation::Axial,
                        con.slider_pos as isize,
                    );
                    let image = egui_image_from_mat(mat);
                    let h = res.ctx.load_texture("atlas", image, Default::default());
                    con.image_data2 = Some(h);
                }
            });
        });

        black_box(&mut ui[1], "right", |ui| {
            let mut inner_rect = Rect::NAN;

            let f = |ui: &mut Ui| {
                if let Some(im) = &con.image_data {
                    ui.image(im);
                }
                inner_rect = ui.min_rect();
            };

            let response = Scene::new()
                .zoom_range(0.0..=f32::INFINITY)
                .show(ui, &mut con.scene_rect, f)
                .response;

            if response.double_clicked() {
                con.scene_rect = inner_rect;
            }
        });
    });
}

fn table_ui(model: &mut Model, con: &mut RegisterController, ui: &mut egui::Ui) {
    use egui_extras::{Column, TableBuilder};

    let available_height = ui.available_height();
    let mut table = TableBuilder::new(ui)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder())
        .column(Column::remainder())
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
            header.col(|ui| {
                ui.strong("Atlas Registered");
            });
            header.col(|ui| {
                ui.strong("Whole Registered");
            });
        })
        .body(|body| {
            let img_ids = model.get_all_image_ids();
            body.rows(18.0, img_ids.len(), |mut row| {
                let idx = row.index();
                let img = img_ids[idx];

                row.set_selected(con.selection.contains(img.src_fn()));
                row.set_overline(true);

                row.col(|ui| {
                    ui.label(img.src_fn());
                });
                row.col(|ui| {
                    ui.label(img.registration_channel.to_string());
                });
                row.col(|ui| {
                    ui.label(img.cell_channel.to_string());
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
                    con.toggle_selection(img, &row.response().ctx);
                } else if clicked {
                    con.unselect_all();
                    con.toggle_selection(img, &row.response().ctx);
                    con.on_image_selected(img, &row.response().ctx);
                }
            });
        });
}
