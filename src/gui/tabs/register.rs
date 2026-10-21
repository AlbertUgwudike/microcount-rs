//! Register: align each section to an Allen CCF slice automatically or with six
//! control points (`RegisterView` / `RegisterController`).

use super::read_down;
use crate::core::image::Image;
use crate::core::imgproc::stats::imadjust_u16;
use crate::core::registration::{auto_register, fit_affine, Affine};
use crate::core::workspace::{ConvertStatus, Direction, ImageEntry, Orientation, TransformData, PAD};
use crate::gui::jobs::Msg;
use crate::gui::widgets::{
    button_row, color_image, column_widths, header, hexagon, image_in, pane, pane_message, pane_strip, pane_ui, status_label, Selection,
    StatusKind, Tex, ROW_H,
};
use crate::gui::App;
use egui::Color32;
use egui_extras::{Column, TableBuilder};

pub struct State {
    pub(crate) sel: Selection,
    ori: Orientation,
    slice: usize,
    atlas_hex: [[f64; 2]; 6],
    hist_hex: [[f64; 2]; 6],
    loaded_for: String,
    atlas_tex: Tex,
    hist_tex: Tex,
    show_overlay: bool,
    drag_a: Option<usize>,
    drag_h: Option<usize>,
    /// Auto-registration in progress: (image id, orientation, slice).
    pending: Option<(String, Orientation, usize)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sel: Selection::default(),
            ori: Orientation::Axial,
            slice: 1,
            atlas_hex: [[0.0; 2]; 6],
            hist_hex: [[0.0; 2]; 6],
            loaded_for: String::new(),
            atlas_tex: Tex::default(),
            hist_tex: Tex::default(),
            show_overlay: true,
            drag_a: None,
            drag_h: None,
            pending: None,
        }
    }
}

fn default_transform(app: &App, img: &ImageEntry, dir: Direction, ori: Orientation) -> Option<TransformData> {
    let atlas = app.atlas.as_ref()?;
    Some(TransformData::default_for(img.down_size, atlas.slice_size(ori), atlas.n_slices(ori), dir, ori))
}

/// Padded, rotated registration channel as shown in the histology view.
fn histology(app: &App, img: &ImageEntry, dir: Direction) -> anyhow::Result<Image<u16>> {
    let ws = app.ws.as_ref().unwrap();
    let d = read_down(&ws.paths().down_path(&img.id), img.registration_channel)?;
    Ok(d.rotate_ccw(dir.quarters()).pad(PAD, 0))
}

pub fn on_registered(app: &mut App, id: String, result: Result<Affine, String>) {
    let pending = app.register.pending.take();
    match result {
        Ok(t) => {
            let (ori, slice) = pending.filter(|p| p.0 == id).map(|p| (p.1, p.2)).unwrap_or((app.register.ori, app.register.slice));
            if let Some(img) = app.ws.as_mut().and_then(|w| w.image_mut(&id)) {
                if let Some(td) = img.transform.as_mut() {
                    td.matrix = t.0;
                    td.slice_idx = slice;
                    td.orientation = ori;
                    img.aligned = true;
                }
            }
            app.register.show_overlay = true;
            app.register.hist_tex.key.clear();
            app.mark_dirty();
            log::info!("{id}: automatic registration finished");
        }
        Err(e) => app.error(format!("{id}: registration failed: {e}")),
    }
}

/// Loads the editing state from the image's transform, creating a default one.
fn load_image(app: &mut App, s: &mut State, id: &str) {
    let Some(img) = app.ws.as_ref().and_then(|w| w.image(id)).cloned() else { return };
    let td = match img.transform.clone() {
        Some(td) => td,
        None => {
            let Some(td) = default_transform(app, &img, Direction::North, s.ori) else { return };
            app.ws.as_mut().unwrap().image_mut(id).unwrap().transform = Some(td.clone());
            app.mark_dirty();
            td
        }
    };
    s.ori = td.orientation;
    s.slice = td.slice_idx;
    s.atlas_hex = td.atlas_hex;
    s.hist_hex = td.hist_hex;
    s.loaded_for = id.to_string();
    s.hist_tex.key.clear();
    s.atlas_tex.key.clear();
}

/// Replaces the transform with a default one (after rotating or changing orientation).
fn reset_transform(app: &mut App, s: &mut State, id: &str, dir: Direction, ori: Orientation) {
    let Some(img) = app.ws.as_ref().and_then(|w| w.image(id)).cloned() else { return };
    if let Some(td) = default_transform(app, &img, dir, ori) {
        let img = app.ws.as_mut().unwrap().image_mut(id).unwrap();
        img.transform = Some(td);
        img.aligned = false;
        app.mark_dirty();
        load_image(app, s, id);
    }
}

pub fn ui(app: &mut App, ui: &mut egui::Ui, s: &mut State) {
    let Some(ws) = app.ws.as_ref() else { return };
    let images: Vec<&ImageEntry> = ws.images.iter().filter(|i| i.status == ConvertStatus::Converted).collect();
    let ids: Vec<String> = images.iter().map(|i| i.id.clone()).collect();
    s.sel.retain(&ids);
    if app.atlas.is_none() {
        let r = pane(ui, ui.available_size());
        pane_message(ui, r, &format!("Atlas: {}", app.atlas_status));
        return;
    }

    egui::TopBottomPanel::top("register_top")
        .resizable(true)
        .default_height(ui.available_height() * 0.3)
        .min_height(100.0)
        .show_inside(ui, |ui| {
            let widths = column_widths(ui, &[4.0, 1.0, 1.0]);
            let mut clicked = None;
            let mut tb = TableBuilder::new(ui)
                .id_salt("align")
                .striped(true)
                .sense(egui::Sense::click())
                .auto_shrink([false, false])
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for &w in &widths {
                tb = tb.column(Column::exact(w).clip(true));
            }
            tb.header(ROW_H, |mut h| {
                h.col(|ui| header(ui, "Image", ""));
                h.col(|ui| header(ui, "Atlas Aligned", "Registered to an atlas slice"));
                h.col(|ui| header(ui, "Skip Registration", "Analysed as a whole image, without atlas regions"));
            })
            .body(|body| {
                body.rows(ROW_H, images.len(), |mut row| {
                    let img = images[row.index()];
                    row.set_selected(s.sel.contains(&img.id));
                    row.col(|ui| {
                        ui.label(&img.id);
                    });
                    row.col(|ui| {
                        if img.aligned {
                            status_label(ui, "✔", StatusKind::Done);
                        }
                    });
                    row.col(|ui| {
                        if img.whole_aligned {
                            status_label(ui, "✔", StatusKind::Done);
                        }
                    });
                    if row.response().clicked() {
                        clicked = Some(row.index());
                    }
                });
            });
            if let Some(i) = clicked {
                s.sel.click(i, &ids, egui::Modifiers::NONE);
            }
        });
    ui.add_space(6.0);

    let avail = ui.available_size();
    let half = egui::vec2((avail.x - ui.spacing().item_spacing.x) / 2.0, avail.y);
    let (left, right) = ui.horizontal(|ui| (pane(ui, half), pane(ui, half))).inner;

    let Some(id) = s.sel.focus.clone() else {
        pane_message(ui, left, "Select an image to register");
        return;
    };
    if s.loaded_for != id {
        load_image(app, s, &id);
    }
    let atlas = app.atlas.clone().unwrap();
    let Some(img) = app.ws.as_ref().unwrap().image(&id).cloned() else { return };
    let Some(td) = img.transform.clone() else { return };
    let n = atlas.n_slices(s.ori);
    s.slice = s.slice.clamp(1, n);

    // Textures: atlas reference slice, and histology with the registered borders.
    let akey = format!("{:?}|{}", s.ori, s.slice);
    if !s.atlas_tex.is(&akey) {
        let r = imadjust_u16(&atlas.reference_slice(s.ori, s.slice));
        s.atlas_tex.set(ui.ctx(), akey, color_image(&[&r]));
    }
    let overlay = s.show_overlay && img.aligned;
    let hkey = format!("{id}|{:?}|{}|{overlay}|{:?}|{}|{:?}", td.direction, img.registration_channel, td.matrix, td.slice_idx, td.orientation);
    if !s.hist_tex.is(&hkey) {
        match histology(app, &img, td.direction) {
            Ok(h) => {
                let h = imadjust_u16(&h);
                let mut ci = color_image(&[&h]);
                if overlay {
                    if let Ok(ov) = atlas.overlay(&td, &[], &[]) {
                        for (p, &b) in ci.pixels.iter_mut().zip(&ov.borders.data) {
                            if b {
                                *p = Color32::from_rgb(255, 255, 255);
                            }
                        }
                    }
                }
                s.hist_tex.set(ui.ctx(), hkey, ci);
            }
            Err(e) => {
                pane_message(ui, right, &format!("{e:#}"));
                return;
            }
        }
    }

    // Left pane: atlas, orientation at the top, slice slider at the bottom.
    let (ltop, lbot) = (pane_strip(left, ROW_H, true), pane_strip(left, ROW_H, false));
    let larea = egui::Rect::from_min_max(egui::pos2(left.min.x + 8.0, ltop.max.y + 6.0), egui::pos2(left.max.x - 8.0, lbot.min.y - 6.0));
    if let Some((resp, fit)) = image_in(ui, &s.atlas_tex, larea, "atlas") {
        hexagon(ui, &resp, &fit, &mut s.atlas_hex, &mut s.drag_a, Color32::from_rgb(80, 200, 255));
    }
    let mut c = pane_ui(ui, ltop, egui::Layout::top_down(egui::Align::Center));
    if c.add(egui::Button::new(format!("Atlas Orientation: {:?}", s.ori)).min_size(egui::vec2(240.0, ROW_H))).on_hover_text("Cycle axial / sagittal / coronal").clicked() {
        reset_transform(app, s, &id, td.direction, s.ori.cycle());
    }
    let mut c = pane_ui(ui, lbot, egui::Layout::left_to_right(egui::Align::Center));
    let label = atlas.laterality_label(s.ori, s.slice);
    c.spacing_mut().slider_width = (lbot.width() - 200.0).max(80.0);
    c.add(egui::Slider::new(&mut s.slice, 1..=n)).on_hover_text("Atlas slice (depth)");
    c.label(egui::RichText::new(label).strong());

    // Right pane: registration actions at the top, view controls at the bottom.
    let (rtop, rbot) = (pane_strip(right, ROW_H, true), pane_strip(right, ROW_H, false));
    let rarea = egui::Rect::from_min_max(egui::pos2(right.min.x + 8.0, rtop.max.y + 6.0), egui::pos2(right.max.x - 8.0, rbot.min.y - 6.0));
    if let Some((resp, fit)) = image_in(ui, &s.hist_tex, rarea, "hist") {
        hexagon(ui, &resp, &fit, &mut s.hist_hex, &mut s.drag_h, Color32::from_rgb(255, 120, 200));
    }
    if img.aligned && (td.slice_idx != s.slice || td.orientation != s.ori) {
        let warn = egui::Rect::from_min_size(egui::pos2(rarea.min.x, rarea.min.y), egui::vec2(rarea.width(), 20.0));
        pane_ui(ui, warn, egui::Layout::top_down(egui::Align::Center))
            .colored_label(Color32::from_rgb(240, 190, 70), format!("Registered to slice {} — register again to use this slice", td.slice_idx));
    }
    let busy = s.pending.is_some();
    let mut c = pane_ui(ui, rtop, egui::Layout::left_to_right(egui::Align::Center));
    let clicked = button_row(
        &mut c,
        &[
            ("Register (Automatic)", !busy, "Align the atlas slice to the section automatically"),
            ("Register (Control Points)", !busy, "Fit the atlas so that each V1–V6 lands on the matching point of the section"),
            ("Skip Registration", !busy, "Analyse the whole image without atlas regions"),
        ],
    );
    match clicked {
        Some(0) => match histology(app, &img, td.direction) {
            Ok(hist) => {
                let reference = atlas.reference_slice(s.ori, s.slice);
                s.pending = Some((id.clone(), s.ori, s.slice));
                let idc = id.clone();
                app.jobs.spawn(move || Msg::Registered { result: auto_register(&reference, &hist).map_err(|e| format!("{e:#}")), id: idc });
            }
            Err(e) => app.error(format!("{e:#}")),
        },
        Some(1) => match fit_affine(&s.atlas_hex, &s.hist_hex) {
            Ok(t) => {
                let im = app.ws.as_mut().unwrap().image_mut(&id).unwrap();
                let td = im.transform.as_mut().unwrap();
                td.matrix = t.0;
                td.atlas_hex = s.atlas_hex;
                td.hist_hex = s.hist_hex;
                td.slice_idx = s.slice;
                td.orientation = s.ori;
                im.aligned = true;
                s.show_overlay = true;
                s.hist_tex.key.clear();
                app.mark_dirty();
            }
            Err(e) => app.error(format!("{e:#}")),
        },
        Some(2) => {
            app.ws.as_mut().unwrap().image_mut(&id).unwrap().whole_aligned = true;
            s.show_overlay = false;
            app.mark_dirty();
        }
        _ => {}
    }
    if busy {
        let sp = egui::Rect::from_center_size(rarea.center(), egui::vec2(32.0, 32.0));
        pane_ui(ui, sp, egui::Layout::top_down(egui::Align::Center)).spinner();
    }
    let mut c = pane_ui(ui, rbot, egui::Layout::left_to_right(egui::Align::Center));
    let clicked = button_row(
        &mut c,
        &[
            (if s.show_overlay { "Hide Overlay" } else { "Show Overlay" }, img.aligned, "Show the registered atlas borders"),
            ("Rotate Image", !busy, "Rotate the section 90° (resets the alignment)"),
        ],
    );
    match clicked {
        Some(0) => s.show_overlay = !s.show_overlay,
        Some(1) => reset_transform(app, s, &id, td.direction.rotate(), s.ori),
        _ => {}
    }
}
