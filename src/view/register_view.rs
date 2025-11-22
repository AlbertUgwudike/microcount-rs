use std::ops::Div;

use eframe::egui::{
    self, Color32, Pos2, Rect, Response, Scene, Sense, Shape, Stroke, TextureHandle, Ui, Vec2,
};

use crate::controller::RegisterController;
use crate::model::Model;
use crate::utility::imops::egui_image_from_mat;
use crate::utility::io::egui_image_from_path;

pub fn ui_tab_register(model: &mut Model, con: &mut RegisterController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        table_ui(model, con, ui);

        ui.separator();

        if ui.button("Register").clicked() {
            con.register_button_pushed(model);
        }

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
                let toggle_ori_button = ui.button("Orientation");

                if toggle_ori_button.clicked() {
                    con.toggle_atlas_orientation();
                    con.on_atlas_interact(model, &toggle_ori_button.ctx);
                }

                let mut inner_rect = Rect::NAN;

                let f = |ui: &mut Ui| {
                    if let Some(im) = &con.image_data2 {
                        ui.image(im);
                    }

                    inner_rect = ui.min_rect();
                    draw_hex(&mut con.atlas_hex, con.transform.scaling, ui);
                };

                let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);

                let mut response = scene.show(ui, &mut con.scene_rect2, f).response;

                scene.register_pan_and_zoom(ui, &mut response, &mut con.transform);

                if response.double_clicked() {
                    con.scene_rect2 = inner_rect;
                }

                let n_slices = model.atlas.n_slices(con.atlas_orientation);

                let slider = egui::Slider::new(&mut con.slider_pos, 0..=(n_slices - 1));
                let slider = ui.add(slider).interact(Sense::click_and_drag());

                if slider.dragged() {
                    con.on_atlas_interact(model, &slider.ctx);
                }
            });
        });

        black_box(&mut ui[1], "right", |ui| {
            let mut inner_rect = Rect::NAN;

            let f = |ui: &mut Ui| {
                if let Some(im) = &con.image_data {
                    ui.image(im);
                    let dim = (*im.size().iter().max().unwrap() as f32) / 250.0;
                    draw_hex(&mut con.hist_hex, con.transform2.scaling / dim, ui);
                }
                inner_rect = ui.min_rect();
            };

            let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);

            let mut response = scene.show(ui, &mut con.scene_rect, f).response;

            scene.register_pan_and_zoom(ui, &mut response, &mut con.transform2);

            if response.double_clicked() {
                con.scene_rect = inner_rect;
            }
        });
    });
}

fn draw_hex(pos: &mut [(f32, f32); 6], scale: f32, ui: &mut egui::Ui) {
    let r = ui.min_rect();
    let painter = ui.painter_at(r);
    let response = ui.interact(painter.clip_rect(), ui.id(), Sense::all());

    let vertices = pos.map(Pos2::from).to_vec();
    let hex = Shape::convex_polygon(
        vertices,
        Color32::TRANSPARENT,
        Stroke::new(4.0 / scale, Color32::BLUE),
    );

    let res_1 = ui.interact(
        hex.visual_bounding_rect(),
        response.id.with(6),
        Sense::drag(),
    );

    painter.add(hex);

    for i in 0..6 {
        let mut start = Pos2::from(pos[i]);

        let circ_rect = Rect::from_center_size(start, Vec2::new(10.0, 10.0) / scale);
        painter.circle(start, 5.0 / scale, Color32::GREEN, Stroke::NONE);

        let res = ui.interact(circ_rect, response.id.with(i), Sense::drag());
        start += res.drag_delta() + res_1.drag_delta();
        pos[i] = (start.x, start.y);
    }
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
