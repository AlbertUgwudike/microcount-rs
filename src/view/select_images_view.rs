use std::ops::Div;
use std::path::Path;

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

    table_ui(model, con, ui);

    ui.separator();

    egui::Frame::new().show(ui, |ui| {
        image_viewer(model, con, ui);
    });
}

fn image_viewer(model: &mut Model, con: &mut SelectImagesController, ui: &mut egui::Ui) {
    let w = ui.available_width();
    egui::containers::Area::new("my. gird".into())
        .fixed_pos(ui.min_rect().min)
        .default_width(w.div(2.0))
        .show(ui.ctx(), |ui| {
            let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);

            let mut inner_rect = Rect::NAN;
            let img = con.selected_img.and_then(|idx| con.get_image(model, idx));

            let f = |ui: &mut Ui| {
                if let Some(im) = &con.image_data {
                    let src_fn_opt = img.map(|im_md| im_md.src_fn());
                    if let Some(src_fn) = src_fn_opt {
                        ui.image(im);
                        bounding_box(con, ui, src_fn);
                    }
                }
                inner_rect = ui.min_rect();
            };

            let mut sr = Rect::ZERO;
            let response = scene.show(ui, &mut sr, f).response;

            con.scene_rect = sr;

            if response.double_clicked() {
                con.scene_rect = inner_rect;
            }
        });

    egui::containers::Area::new("my. gird2".into())
        .fixed_pos(Pos2::new(w.div(2.0), ui.min_rect().min.y))
        .default_width(w.div(2.0))
        .show(ui.ctx(), |ui| {
            let scene2 = Scene::new().zoom_range(0.0..=f32::INFINITY);

            let mut inner_rect2 = Rect::NAN;

            let response2 = scene2
                .show(ui, &mut con.scene_rect2, |ui| {
                    if let Some(im) = &con.image_data2 {
                        ui.image(im);
                    }

                    inner_rect2 = ui.min_rect();
                })
                .response;

            if response2.double_clicked() {
                con.scene_rect2 = inner_rect2;
            }
        });
}

fn bounding_box(con: &mut SelectImagesController, ui: &mut egui::Ui, src_fn: &str) {
    let r = ui.min_rect();
    let painter = ui.painter_at(r);
    let response = ui.interact(painter.clip_rect(), ui.id(), Sense::all());
    let bbox_min = r.min + con.pos_offset;
    let bbox_max = r.min + con.sz_offset + con.pos_offset;

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
    let r_res = ui.interact(bbox_rect, response.id.with(1), Sense::drag());
    let r_res = ui.interact(bbox_rect, response.id.with(1), Sense::click());

    con.pos_offset += r_res.drag_delta();
    con.sz_offset += h_res.drag_delta();

    con.pos_offset.x = r
        .x_range()
        .clamp(r.x_range().clamp(con.pos_offset.x + con.sz_offset.x) - con.sz_offset.x);
    con.pos_offset.y = r
        .y_range()
        .clamp(r.y_range().clamp(con.pos_offset.y + con.sz_offset.y) - con.sz_offset.y);

    if r_res.double_clicked() {
        let scaled_offset = con.pos_offset * 25.0;
        let scaled_sz_offset = con.sz_offset * 25.0;
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
            con.image_data2 = Some(h);
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
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height.div(5.0));

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
                    ui.label(img.as_ref().map_or("", |_| "0"));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |_| "0"));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |_| "0"));
                });
                row.col(|ui| {
                    ui.label(img.as_ref().map_or("", |i| i.conversion_status.to_str()));
                });

                if row.response().clicked() {
                    con.on_image_selected(row.index(), model, &row.response().ctx);
                }
            });
        });
}
