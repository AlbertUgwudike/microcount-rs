use std::ops::Div;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Scene, Sense, Shape, Stroke, Ui, Vec2,
};

use crate::{
    controller::RegisterController,
    view::view_utils::{egui_display_gray, egui_display_rgb},
};

pub fn ui_tab_register(con: &mut RegisterController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        table_ui(con, ui);

        ui.separator();

        ui.horizontal(|ui| {
            if let Some(im_id) = &con.state.selected_img.clone() {
                if ui.button("Register").clicked() {
                    con.register_button_pushed();
                    con.on_image_selected(im_id, ui.ctx());
                }

                if ui.button("Rotate").clicked() {
                    con.rotate_image(im_id);
                    con.on_image_selected(im_id, ui.ctx());
                }

                if ui.button("Toggle Overlay").clicked() {
                    con.state.show_overlay = !con.state.show_overlay;
                    con.on_image_selected(im_id, ui.ctx());
                }
            }
        });

        ui.separator();

        image_viewer(con, ui);
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

fn image_viewer(con: &mut RegisterController, ui: &mut egui::Ui) {
    ui.columns(2, |ui| {
        black_box(&mut ui[0], "left", |ui| {
            ui.vertical(|ui| {
                ui.set_max_size(ui.available_size_before_wrap() - Vec2::new(0., 30.));

                ui.add_space(10.);

                ui.horizontal(|ui| {
                    ui.add_space(10.);

                    let toggle_ori_button = ui.button("Orientation");

                    if toggle_ori_button.clicked() {
                        con.toggle_atlas_orientation();
                        con.on_atlas_interact(ui.ctx());
                    }
                });

                let mut inner_rect = Rect::NAN;
                let mut tmp = con.state.left_scene_rect;

                let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);
                let mut r = scene.show(ui, &mut tmp, |ui: &mut Ui| {
                    if let Some(im) = &con.state.atlas_slice_data {
                        ui.image(&im.0);
                    }

                    inner_rect = ui.min_rect();
                    draw_hex(
                        &mut con.state.atlas_hex,
                        con.state.atlas_scene_tf.scaling,
                        ui,
                    );
                });

                con.state.left_scene_rect = tmp;

                scene.register_pan_and_zoom(ui, &mut r.response, &mut con.state.atlas_scene_tf);

                if r.response.double_clicked() {
                    con.state.left_scene_rect = inner_rect;
                    con.state.atlas_scene_tf.scaling = 1.0;
                }

                ui.horizontal(|ui| {
                    ui.add_space(10.);
                    ui.spacing_mut().slider_width = ui.available_width() - 70.;

                    let n_slices = con.n_atlas_slices(con.state.atlas_orientation);
                    let slider = egui::Slider::new(&mut con.state.slider_pos, 0..=(n_slices - 1));
                    let slider = ui.add(slider).interact(Sense::click_and_drag());

                    if slider.dragged() {
                        con.on_atlas_interact(ui.ctx());
                    }
                });

                ui.add_space(10.);
            });
        });

        black_box(&mut ui[1], "right", |ui| {
            let mut inner_rect = Rect::NAN;
            let mut tmp = con.state.right_scene_rect;

            let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);

            let mut r = scene.show(ui, &mut tmp, |ui: &mut Ui| {
                if let Some(im) = &con.state.hist_slice_data {
                    ui.image(&im.0);
                    let dim = std::cmp::max(im.1.dim().0, im.1.dim().1) as f32 / 250.0;
                    draw_hex(
                        &mut con.state.hist_hex,
                        con.state.hist_scene_tf.scaling / dim,
                        ui,
                    );
                }
                inner_rect = ui.min_rect();
            });

            con.state.right_scene_rect = tmp;

            scene.register_pan_and_zoom(ui, &mut r.response, &mut con.state.hist_scene_tf);

            if r.response.double_clicked() {
                con.state.right_scene_rect = inner_rect;
                con.state.hist_scene_tf.scaling = 1.0;
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
        painter.text(
            start,
            Align2::CENTER_CENTER,
            (i + 1).to_string(),
            FontId::new(f32::max(10.0 / scale, 6.0), egui::FontFamily::Monospace),
            Color32::BLUE,
        );

        let res = ui.interact(circ_rect, response.id.with(i), Sense::drag());
        start += res.drag_delta() + res_1.drag_delta();
        pos[i] = (start.x, start.y);
    }
}

fn table_ui(con: &mut RegisterController, ui: &mut egui::Ui) {
    use egui_extras::{Column, TableBuilder};

    let available_height = ui.available_height();
    let mut table = TableBuilder::new(ui)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
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
                ui.strong("Registeration Status");
            });
        })
        .body(|body| {
            let img_ids = con.img_ids();
            let reg_statuses = con.reg_statuses();
            body.rows(18.0, img_ids.len(), |mut row| {
                let idx = row.index();
                let id = &img_ids[idx];
                let rs = &reg_statuses[idx];

                row.set_selected(con.state.selection.contains(id));
                row.set_overline(true);

                row.col(|ui| {
                    ui.label(id);
                });
                row.col(|ui| {
                    ui.label(rs);
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
                    con.on_atlas_interact(&row.response().ctx);
                }
            });
        });
}
