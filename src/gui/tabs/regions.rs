//! Select Regions: choose atlas regions (per hemisphere) for each registered image
//! (`RegionsView` / `RegionsController`).

use super::read_down;
use crate::core::atlas::{Atlas, WHOLE_IMAGE};
use crate::core::imgproc::stats::imadjust_u16;
use crate::core::pipeline;
use crate::core::workspace::{ImageEntry, Laterality, Region, PAD};
use crate::gui::jobs::Msg;
use crate::gui::widgets::{button_row, color_image, column_widths, header, image_in, pane, pane_message, pane_strip, pane_ui, Selection, Tex, ROW_H};
use crate::gui::App;
use egui::Color32;
use egui_extras::{Column, TableBuilder};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct State {
    pub(crate) sel: Selection,
    checked: BTreeSet<String>,
    filter: String,
    preview: Tex,
}

const ROOT_ID: u32 = 997;

fn tree(ui: &mut egui::Ui, atlas: &Atlas, id: u32, checked: &mut BTreeSet<String>, parent_checked: bool) {
    let Some(s) = atlas.by_id(id) else { return };
    let kids = atlas.children_of(id);
    let label = format!("{} ({})", s.name, s.acronym);
    let mut on = checked.contains(&s.acronym) || parent_checked;
    let mut toggle = |ui: &mut egui::Ui, checked: &mut BTreeSet<String>| {
        let resp = ui.add_enabled(!parent_checked, egui::Checkbox::new(&mut on, label.as_str()));
        if resp.changed() {
            if on {
                checked.insert(s.acronym.clone());
            } else {
                checked.remove(&s.acronym);
            }
        }
    };
    if kids.is_empty() {
        toggle(ui, checked);
    } else {
        let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(("tree", id)), id == ROOT_ID);
        state
            .show_header(ui, |ui| toggle(ui, checked))
            .body(|ui| {
                for &k in kids {
                    tree(ui, atlas, k, checked, on);
                }
            });
    }
}

fn add_regions(app: &mut App, s: &State, lat: Laterality) {
    let Some(atlas) = app.atlas.clone() else { return };
    let paths = app.ws.as_ref().unwrap().paths();
    let keys: Vec<String> = s.checked.iter().cloned().collect();
    if keys.is_empty() {
        log::warn!("tick one or more regions first");
        return;
    }
    let mut added = 0;
    let ws = app.ws.as_mut().unwrap();
    for img in ws.images.iter_mut().filter(|i| s.sel.contains(&i.id)) {
        for key in &keys {
            if key != WHOLE_IMAGE && !img.aligned {
                log::warn!("{}: not registered to the atlas, only the whole image can be added", img.id);
                continue;
            }
            let mut region = Region::new(key, lat);
            let rid = region.id(&img.id);
            if img.regions.iter().any(|r| r.id(&img.id) == rid) {
                continue;
            }
            match pipeline::write_mask_thumbnail(&paths, &atlas, img, &region) {
                Ok(dn) => {
                    region.dn_rect = Some(dn);
                    img.regions.push(region);
                    added += 1;
                }
                Err(e) => log::warn!("{rid}: {e:#}"),
            }
        }
    }
    log::info!("added {added} region(s)");
    app.mark_dirty();
}

fn remove_regions(app: &mut App, s: &State, lat: Laterality) {
    let ws = app.ws.as_mut().unwrap();
    for img in ws.images.iter_mut().filter(|i| s.sel.contains(&i.id)) {
        img.regions.retain(|r| !(s.checked.contains(&r.key) && (r.laterality == lat || r.laterality == Laterality::Bilat)));
    }
    app.mark_dirty();
}

fn preview(app: &App, img: &ImageEntry) -> anyhow::Result<egui::ColorImage> {
    let ws = app.ws.as_ref().unwrap();
    let td = img.transform.as_ref().filter(|_| img.aligned);
    let dir = td.map(|t| t.direction).unwrap_or_default();
    let d = read_down(&ws.paths().down_path(&img.id), img.registration_channel)?;
    let h = imadjust_u16(&d.rotate_ccw(dir.quarters()).pad(PAD, 0));
    let mut ci = color_image(&[&h]);
    if let (Some(td), Some(atlas)) = (td, &app.atlas) {
        let keys = |lat: Laterality| -> Vec<&str> {
            img.regions.iter().filter(|r| r.laterality == lat && r.key != WHOLE_IMAGE).map(|r| r.key.as_str()).collect()
        };
        let ov = atlas.overlay(td, &keys(Laterality::Left), &keys(Laterality::Right))?;
        let tint = |p: &mut Color32, c: [u8; 3]| {
            *p = Color32::from_rgb(
                ((p.r() as u16 + c[0] as u16) / 2) as u8,
                ((p.g() as u16 + c[1] as u16) / 2) as u8,
                ((p.b() as u16 + c[2] as u16) / 2) as u8,
            )
        };
        for (i, p) in ci.pixels.iter_mut().enumerate() {
            if ov.left.data[i] {
                tint(p, [0, 220, 255]);
            }
            if ov.right.data[i] {
                tint(p, [255, 60, 200]);
            }
            if ov.borders.data[i] {
                *p = Color32::WHITE;
            }
        }
    }
    Ok(ci)
}

pub fn ui(app: &mut App, ui: &mut egui::Ui, s: &mut State) {
    let Some(ws) = app.ws.as_ref() else { return };
    let images: Vec<&ImageEntry> = ws.images.iter().filter(|i| i.aligned || i.whole_aligned).collect();
    let ids: Vec<String> = images.iter().map(|i| i.id.clone()).collect();
    s.sel.retain(&ids);
    let Some(atlas) = app.atlas.clone() else {
        let r = pane(ui, ui.available_size());
        pane_message(ui, r, &format!("Atlas: {}", app.atlas_status));
        return;
    };

    let mut action = None;
    egui::TopBottomPanel::top("regions_top")
        .resizable(true)
        .default_height(ui.available_height() * 0.42)
        .min_height(160.0)
        .show_inside(ui, |ui| {
            let half = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            let height = ui.available_height();
            ui.horizontal_top(|ui| {
                // Images and their regions.
                ui.allocate_ui_with_layout(egui::vec2(half, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(half);
                    let some = !s.sel.keys.is_empty();
                    match button_row(
                        ui,
                        &[
                            ("Select All", true, ""),
                            ("Erase Regions", some, "Remove every region of the selected images (and their results)"),
                            ("Export Regions", some, "Write each region of the selected images as a masked full-resolution TIFF"),
                        ],
                    ) {
                        Some(0) => s.sel.select_all(&ids),
                        Some(1) => action = Some(Action::Erase),
                        Some(2) => action = Some(Action::Export),
                        _ => {}
                    }
                    ui.add_space(4.0);
                    let widths = column_widths(ui, &[1.0, 2.0]);
                    let mut clicked = None;
                    TableBuilder::new(ui)
                        .id_salt("region_images")
                        .striped(true)
                        .sense(egui::Sense::click())
                        .auto_shrink([false, false])
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::exact(widths[0]).clip(true))
                        .column(Column::exact(widths[1]).clip(true))
                        .header(ROW_H, |mut h| {
                            h.col(|ui| header(ui, "Images", "Registered images, or images analysed whole"));
                            h.col(|ui| header(ui, "Regions", ""));
                        })
                        .body(|body| {
                            body.rows(ROW_H, images.len(), |mut row| {
                                let img = images[row.index()];
                                row.set_selected(s.sel.contains(&img.id));
                                row.col(|ui| {
                                    ui.label(&img.id);
                                });
                                let regions: Vec<String> = img.regions.iter().map(|r| r.location()).collect();
                                row.col(|ui| {
                                    let text = if regions.is_empty() { "--".to_string() } else { regions.join(", ") };
                                    ui.label(&text).on_hover_text(regions.join("\n"));
                                });
                                if row.response().clicked() {
                                    clicked = Some(row.index());
                                }
                            });
                        });
                    if let Some(i) = clicked {
                        let mods = ui.input(|i| i.modifiers);
                        s.sel.click(i, &ids, mods);
                    }
                });
                // Atlas region tree.
                ui.allocate_ui_with_layout(egui::vec2(half, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(half);
                    let ok = !s.sel.keys.is_empty() && !s.checked.is_empty();
                    match button_row(
                        ui,
                        &[
                            ("Add - L", ok, "Add the ticked regions in the left hemisphere"),
                            ("Add - R", ok, "Add the ticked regions in the right hemisphere"),
                            ("Del - L", ok, "Remove the ticked regions from the left hemisphere"),
                            ("Del - R", ok, "Remove the ticked regions from the right hemisphere"),
                        ],
                    ) {
                        Some(0) => action = Some(Action::Add(Laterality::Left)),
                        Some(1) => action = Some(Action::Add(Laterality::Right)),
                        Some(2) => action = Some(Action::Remove(Laterality::Left)),
                        Some(3) => action = Some(Action::Remove(Laterality::Right)),
                        _ => {}
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let w = ui.available_width() - 110.0;
                        ui.add(egui::TextEdit::singleline(&mut s.filter).hint_text("Search regions…").desired_width(w));
                        if ui.add_enabled(!s.checked.is_empty(), egui::Button::new("Clear Ticks")).clicked() {
                            s.checked.clear();
                        }
                    });
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("tree").auto_shrink([false; 2]).show(ui, |ui| {
                            let mut whole = s.checked.contains(WHOLE_IMAGE);
                            if ui.checkbox(&mut whole, "Whole Image").changed() {
                                if whole {
                                    s.checked.insert(WHOLE_IMAGE.into());
                                } else {
                                    s.checked.remove(WHOLE_IMAGE);
                                }
                            }
                            let f = s.filter.trim().to_lowercase();
                            if f.is_empty() {
                                tree(ui, &atlas, ROOT_ID, &mut s.checked, false);
                            } else {
                                for st in atlas.structures.iter().filter(|st| st.name.to_lowercase().contains(&f) || st.acronym.to_lowercase().contains(&f)) {
                                    let mut on = s.checked.contains(&st.acronym);
                                    if ui.checkbox(&mut on, format!("{} ({})", st.name, st.acronym)).changed() {
                                        if on {
                                            s.checked.insert(st.acronym.clone());
                                        } else {
                                            s.checked.remove(&st.acronym);
                                        }
                                    }
                                }
                            }
                        });
                    });
                });
            });
        });

    match action {
        Some(Action::Add(lat)) => add_regions(app, s, lat),
        Some(Action::Remove(lat)) => remove_regions(app, s, lat),
        Some(Action::Erase) => {
            for img in app.ws.as_mut().unwrap().images.iter_mut().filter(|i| s.sel.contains(&i.id)) {
                img.regions.clear();
            }
            app.mark_dirty();
        }
        Some(Action::Export) => {
            if let Some(dir) = rfd::FileDialog::new().set_title("Select Export Directory").pick_folder() {
                let ws = app.ws.as_ref().unwrap();
                let imgs: Vec<ImageEntry> = ws.images.iter().filter(|i| s.sel.contains(&i.id)).cloned().collect();
                let (paths, atlas, token) = (ws.paths(), atlas.clone(), app.jobs.process_token());
                app.jobs.spawn(move || match pipeline::export_regions(&paths, &atlas, &imgs, &dir, &token) {
                    Ok(n) => Msg::Done(format!("exported {n} region(s) to {}", dir.display())),
                    Err(e) => Msg::Failed(format!("export failed: {e:#}")),
                });
            }
        }
        None => {}
    }

    ui.add_space(6.0);
    let area = pane(ui, ui.available_size());
    let Some(id) = s.sel.focus.clone() else {
        pane_message(ui, area, "Select an image");
        return;
    };
    let Some(img) = app.ws.as_ref().unwrap().image(&id).cloned() else { return };
    let regions: Vec<String> = img.regions.iter().map(|r| r.location()).collect();
    let key = format!("{id}|{}|{:?}|{}", img.aligned, img.transform.as_ref().map(|t| t.matrix), regions.join(","));
    if !s.preview.is(&key) {
        match preview(app, &img) {
            Ok(ci) => s.preview.set(ui.ctx(), key, ci),
            Err(e) => log::warn!("{id}: {e:#}"),
        }
    }
    let legend = pane_strip(area, 20.0, true);
    image_in(ui, &s.preview, area.shrink(8.0), "regions_preview");
    let mut c = pane_ui(ui, legend, egui::Layout::left_to_right(egui::Align::Center));
    c.colored_label(Color32::from_rgb(0, 220, 255), "■ left");
    c.colored_label(Color32::from_rgb(255, 60, 200), "■ right");
    if !img.aligned {
        c.label("not registered — only the whole image can be analysed");
    }
}

enum Action {
    Add(Laterality),
    Remove(Laterality),
    Erase,
    Export,
}
