use std::ops::Div;

use eframe::egui::{self, Color32, Pos2, Rect, Scene, Sense, Stroke, TextureHandle, Ui, Vec2};

use crate::controller::SelectImagesController;
use crate::model::Model;
use crate::utility::io::egui_image_from_path;

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
        if ui.button("Convert Selected").clicked() {
            model.convert_and_downsample(&con.selection);
        }
    });

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

fn image_viewer(model: &mut Model, con: &mut SelectImagesController, ui: &mut egui::Ui) {
    ui.columns(2, |ui| {
        black_box(&mut ui[0], "left", |ui| {
            let mut inner_rect = Rect::NAN;
            let img = con
                .selected_img
                .as_ref()
                .and_then(|idx| con.get_image(model, idx.as_str()));

            let f = |ui: &mut Ui| {
                if let Some(im) = &con.image_data {
                    let src_fn_opt = img.map(|im_md| im_md.src_fn());
                    if let Some(src_fn) = src_fn_opt {
                        ui.image(im);

                        let pos = &mut con.pos_offset;
                        let sz = &mut con.sz_offset;
                        let data = &mut con.image_data2;

                        bounding_box(ui, src_fn, pos, sz, data);
                    }
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

        black_box(&mut ui[1], "right", |ui| {
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
        });
    });
}

fn bounding_box(
    ui: &mut egui::Ui,
    src_fn: &str,
    pos_offset: &mut Vec2,
    sz_offset: &mut Vec2,
    data: &mut Option<TextureHandle>,
) {
    let r = ui.min_rect();
    let painter = ui.painter_at(r);
    let response = ui.interact(painter.clip_rect(), ui.id(), Sense::all());
    let bbox_min = r.min + *pos_offset;
    let bbox_max = r.min + *sz_offset + *pos_offset;

    let bbox_rect = Rect::from_min_max(bbox_min, bbox_max);
    painter.rect(
        bbox_rect,
        1.0,
        Color32::TRANSPARENT,
        Stroke::new(20.0, Color32::RED),
        egui::StrokeKind::Middle,
    );

    let circ_rect = Rect::from_center_size(bbox_max, Vec2::new(40.0, 40.0));
    painter.circle(bbox_max, 20.0, Color32::GREEN, Stroke::NONE);

    let h_res = ui.interact(circ_rect, response.id.with(0), Sense::drag());
    let _ = ui.interact(bbox_rect, response.id.with(1), Sense::drag());
    let r_res = ui.interact(bbox_rect, response.id.with(1), Sense::click());

    *pos_offset += r_res.drag_delta();
    *sz_offset += h_res.drag_delta();

    pos_offset.x = r
        .x_range()
        .clamp(r.x_range().clamp(pos_offset.x + sz_offset.x) - sz_offset.x);
    pos_offset.y = r
        .y_range()
        .clamp(r.y_range().clamp(pos_offset.y + sz_offset.y) - sz_offset.y);

    if r_res.double_clicked() {
        let scaled_offset = *pos_offset * 25.0;
        let scaled_sz_offset = *sz_offset * 25.0;
        let dn_bbox = (
            scaled_offset.y as usize,
            scaled_offset.x as usize,
            scaled_sz_offset.y as usize,
            scaled_sz_offset.x as usize,
        );

        let _ = egui_image_from_path(src_fn, dn_bbox, 1).map(|im| {
            let h = ui
                .ctx()
                .load_texture("screenshot_demo2", im, Default::default());
            *data = Some(h);
        });
    }
}

fn table_ui(model: &mut Model, con: &mut SelectImagesController, ui: &mut egui::Ui) {
    use egui_extras::{Column, TableBuilder};

    let available_height = ui.available_height();
    let mut table = TableBuilder::new(ui)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder())
        .column(Column::remainder())
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
                row.col(|ui| {
                    ui.label(img.comarker_channel.to_string());
                });
                row.col(|ui| {
                    ui.label(img.conversion_status.to_str());
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
