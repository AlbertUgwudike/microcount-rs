use eframe::egui::{
    self, Align, Color32, Layout, Rect, Scene, Sense, Sides, Stroke, StrokeKind, Ui, Vec2,
};
use std::ops::Div;

use crate::{
    controller::SelectController,
    view::{
        black_box,
        view_utils::{egui_display_gray, egui_display_rgb},
    },
};

pub fn ui_tab_select_images(con: &mut SelectController, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui.button("Add Images").clicked() {
            con.add_images();
        }
        if ui.button("Remove Selected").clicked() {
            con.remove_images();
        }
        if ui.button("Convert Selected").clicked() {
            con.convert_and_downsample();
        }
        if ui.button("Select All").clicked() {
            // let curr_count = Arc::clone(&model.counter);

            // model.dispatch(true, async move {
            //     // simulate work
            //     sleep(Duration::from_millis(1));
            //     *curr_count.lock().await += 1;
            // });
        }
    });

    ui.vertical(|ui| {
        table_ui(con, ui);
        ui.separator();
        image_viewer(con, ui);
    });
}

pub fn table_ui(con: &mut SelectController, ui: &mut Ui) {
    use egui_extras::{Column, TableBuilder};

    let available_height = ui.available_height();

    TableBuilder::new(ui)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::remainder())
        .column(Column::remainder())
        .column(Column::remainder())
        .column(Column::remainder())
        .column(Column::remainder())
        .auto_shrink(false)
        .min_scrolled_height(available_height.div(5.0))
        .max_scroll_height(available_height.div(5.0))
        .striped(true)
        .sense(Sense::click())
        .header(20.0, |mut header| {
            header.col(|ui| {
                Sides::new().show(
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
            let img_ids = con.raw_img_ids();
            let con_ids = con.con_img_ids();
            let img_cons = con.img_cons();

            body.rows(18.0, img_ids.len() + con_ids.len(), |mut row| {
                let idx = row.index();

                let img = img_ids
                    .get(idx)
                    .unwrap_or_else(|| con_ids.get(idx - img_ids.len()).unwrap());

                let def = "Finished".to_string();
                let im_con = img_cons.get(idx).unwrap_or(&def);

                row.set_selected(con.selection_contains(img));
                row.set_overline(true);

                row.col(|ui| {
                    ui.label(img);
                });
                row.col(|ui| {
                    if idx >= img_ids.len() {
                        let res = ui.text_edit_singleline(con.reg_buffer(img));
                        if res.clicked_elsewhere() {
                            con.persist()
                        }
                    }
                });
                row.col(|ui| {
                    if idx >= img_ids.len() {
                        let res = ui.text_edit_singleline(con.cell_buffer(img));
                        if res.clicked_elsewhere() {
                            con.persist()
                        }
                    }
                });
                row.col(|ui| {
                    if idx >= img_ids.len() {
                        let res = ui.text_edit_singleline(con.co_buffer(img));
                        if res.clicked_elsewhere() {
                            con.persist()
                        }
                    }
                });
                row.col(|ui| {
                    ui.label(im_con);
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
                    con.toggle_selection(img);
                } else if clicked {
                    con.unselect_all();
                    con.toggle_selection(img);
                    con.on_image_selected(img, &row.response().ctx);
                }
            });
        });
}

pub fn image_viewer(con: &mut SelectController, ui: &mut Ui) {
    ui.columns(2, |ui| {
        black_box(&mut ui[0], "left", |ui| {
            let mut inner_rect = Rect::NAN;
            let mut tmp = con.state.preview_image_rect;

            let scene = Scene::new()
                .zoom_range(0.0..=f32::INFINITY)
                .show(ui, &mut tmp, |ui| {
                    if let Some(im) = &con.state.preview_image_data {
                        ui.image(&im.0);
                    }
                    bounding_box(con, ui);
                    inner_rect = ui.min_rect();
                });

            con.state.preview_image_rect = tmp;

            if scene.response.double_clicked() {
                con.state.preview_image_rect = inner_rect;
            }
        });

        black_box(&mut ui[1], "right", |ui| {
            let mut inner_rect = Rect::NAN;
            let mut tmp = con.state.image_rect;

            let response = Scene::new()
                .zoom_range(0.0..=f32::INFINITY)
                .show(ui, &mut tmp, |ui| {
                    if let Some(im) = &con.state.image_data {
                        ui.image(&im.0);
                    }
                    inner_rect = ui.min_rect();
                })
                .response;

            con.state.image_rect = tmp;

            if response.double_clicked() {
                con.state.image_rect = inner_rect;
            }
        });
    });
}

pub fn bounding_box(con: &mut SelectController, ui: &mut Ui) {
    let r = ui.min_rect();
    let painter = ui.painter_at(r);
    let response = ui.interact(painter.clip_rect(), ui.id(), Sense::all());
    let bbox_min = r.min + con.state.pos_offset;
    let bbox_max = r.min + con.state.sz_offset + con.state.pos_offset;

    let bbox_rect = Rect::from_min_max(bbox_min, bbox_max);
    painter.rect(
        bbox_rect,
        1.0,
        Color32::TRANSPARENT,
        Stroke::new(20.0, Color32::RED),
        StrokeKind::Middle,
    );

    let circ_rect = Rect::from_center_size(bbox_max, Vec2::new(40.0, 40.0));
    painter.circle(bbox_max, 20.0, Color32::GREEN, Stroke::NONE);

    let h_res = ui.interact(circ_rect, response.id.with(0), Sense::drag());
    let _ = ui.interact(bbox_rect, response.id.with(1), Sense::drag());
    let r_res = ui.interact(bbox_rect, response.id.with(1), Sense::click());

    con.state.pos_offset += r_res.drag_delta();
    con.state.sz_offset += h_res.drag_delta();

    con.state.pos_offset.x = r.x_range().clamp(
        r.x_range()
            .clamp(con.state.pos_offset.x + con.state.sz_offset.x)
            - con.state.sz_offset.x,
    );

    con.state.pos_offset.y = r.y_range().clamp(
        r.y_range()
            .clamp(con.state.pos_offset.y + con.state.sz_offset.y)
            - con.state.sz_offset.y,
    );

    if r_res.double_clicked() {
        let scaled_offset = con.state.pos_offset * 25.0;
        let scaled_sz_offset = con.state.sz_offset * 25.0;
        let origin = (scaled_offset.y as u64, scaled_offset.x as u64); // <-- convert to (r, c)
        let hw = (scaled_sz_offset.y as u64, scaled_sz_offset.x as u64);
        let im_id = con.state.selected_img.clone().unwrap(); //<---
        con.on_subregion_selected(&im_id, hw, origin, ui.ctx());
    };
}
