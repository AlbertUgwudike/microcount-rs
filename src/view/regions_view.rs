use std::ops::Div;

use eframe::egui::{self, Color32, Rect, Scene, ScrollArea, Sense, Ui};

use crate::controller::{
    regions_controller::{RegionSelectorState, SelectionMode},
    RegionsController,
};

pub fn ui_tab_regions(con: &mut RegionsController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        let aw = ui.available_width();
        ui.columns(2, |ui| {
            ui[0].set_max_width(0.3 * aw);
            table_ui(con, &mut ui[0]);

            ui[1].set_max_width(0.7 * aw);
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
        let mut tmp = con.state.scene_rect;

        let scene = Scene::new().zoom_range(0.0..=f32::INFINITY);

        let r = scene.show(ui, &mut tmp, |ui: &mut Ui| {
            if let Some(im) = &con.state.image_data {
                ui.image(&im.0);
            }
            inner_rect = ui.min_rect();
        });

        con.state.scene_rect = tmp;

        if r.response.double_clicked() {
            con.state.scene_rect = inner_rect;
        }
    });
}

fn region_dashboard_ui(con: &mut RegionsController, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui.button("Atlas").clicked() {
                if let Some(im_id) = &con.state.selected_img.clone() {
                    con.state.selection_mode = SelectionMode::Atlas;
                    con.on_image_selected(im_id, ui.ctx());
                }
            }

            if ui.button("Rect").clicked() {
                if let Some(im_id) = &con.state.selected_img.clone() {
                    con.state.selection_mode = SelectionMode::Rect;
                    con.on_image_selected(im_id, ui.ctx());
                }
            }
        });

        ui.separator();

        match con.state.selection_mode {
            SelectionMode::Atlas => atlas_selection_ui(con, ui),
            SelectionMode::Rect => rect_selection_ui(con, ui),
        }
    });
}

fn atlas_selection_ui(con: &mut RegionsController, ui: &mut egui::Ui) {
    ScrollArea::vertical().max_height(90.).show(ui, |ui| {
        if let Some(im_id) = &con.state.selected_img.clone() {
            let change = render_region_selector(
                &mut con.state.region_selector_tree,
                &mut con.state.left_lut,
                &mut con.state.right_lut,
                ui,
            );

            if change {
                con.on_region_selected();
                con.on_image_selected(&im_id, ui.ctx());
            }
        }
    });
}

fn render_region_selector(
    rss: &mut RegionSelectorState,
    llut: &mut [bool; 840],
    rlut: &mut [bool; 840],
    ui: &mut egui::Ui,
) -> bool {
    let mut change = false;
    let id = ui.make_persistent_id(rss.key.to_string());

    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);

    if rss.open {
        state.toggle(ui);
        rss.open = false;
    }

    state
        .show_header(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let response = ui.label(rss.key.name());
                    let id = ui.make_persistent_id(rss.key.to_string().to_string() + "_drop");
                    if ui.interact(response.rect, id, Sense::click()).clicked() {
                        rss.open = true;
                    }

                    let response = ui.checkbox(&mut llut[rss.key as usize], "");
                    let id = ui.make_persistent_id(rss.key.to_string().to_string() + "_left");
                    if ui.interact(response.rect, id, Sense::click()).clicked() {
                        llut[rss.key as usize] = !llut[rss.key as usize];
                        change = true
                    }

                    let response = ui.checkbox(&mut rlut[rss.key as usize], "");
                    let id = ui.make_persistent_id(rss.key.to_string().to_string() + "_right");
                    if ui.interact(response.rect, id, Sense::click()).clicked() {
                        rlut[rss.key as usize] = !rlut[rss.key as usize];
                        change = true
                    }
                });
                // ui.separator();
            });
        })
        .body(|ui| {
            for child in rss.children.iter_mut() {
                change = change || render_region_selector(child, llut, rlut, ui);
            }
        });
    change
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
                ui.strong("Image");
            });
        })
        .body(|body| {
            let img_ids = con.img_ids();
            body.rows(18.0, img_ids.len(), |mut row| {
                let idx = row.index();
                let id = &img_ids[idx];

                row.set_selected(con.state.selection.contains(id));
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
