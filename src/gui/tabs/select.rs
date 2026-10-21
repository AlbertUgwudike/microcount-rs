//! Select Images: add images, convert them, set channel roles and pixel size, and
//! inspect them at full resolution (`SelectImagesView` / `SelectImagesController`).

use super::read_down;
use crate::core::convert;
use crate::core::image::{Image, Rect};
use crate::core::imgproc::stats::imadjust_u16;
use crate::core::imageio::{self, ImageFile};
use crate::core::workspace::{ImageEntry, Workspace};
use crate::core::workspace::ConvertStatus;
use crate::gui::jobs::{ConvertTask, Msg};
use crate::gui::widgets::{
    aligned_row, button_row, color_image, column_widths, header, image_in, pane, pane_message, pane_strip, pane_ui, status_label,
    subview_rect, centred_rect, fill, Selection, StatusKind, Tex, ROW_H,
};
use crate::gui::App;
use egui_extras::{Column, TableBuilder};

#[derive(Default)]
pub struct State {
    pub(crate) sel: Selection,
    channel: usize,
    thumb: Tex,
    rect: [f64; 4],
    rect_for: String,
    view: Tex,
    view_req: u64,
    view_wanted: String,
    apply_ch: [usize; 3],
    apply_px: [f64; 2],
}

pub fn on_pixels(app: &mut App, req: u64, result: &Result<Vec<[Image<u16>; 3]>, String>) -> bool {
    if req != app.select.view_req {
        return false;
    }
    match result {
        Ok(p) => {
            let c = &p[0];
            let key = app.select.view_wanted.clone();
            app.select.view.set(&app.jobs.ctx, key, color_image(&[&c[0], &c[1], &c[2]]));
        }
        Err(e) => log::warn!("preview: {e}"),
    }
    true
}

fn request_view(app: &mut App, s: &mut State, id: &str) {
    let Some(ws) = &app.ws else { return };
    let Some(img) = ws.image(id) else { return };
    let key = format!("{id}|{}|{:?}", s.channel, s.rect.map(|v| v.round() as i64));
    if s.view_wanted == key {
        return;
    }
    s.view_wanted = key;
    let (sx, sy) = (img.size[1] as f64 / img.down_size[1] as f64, img.size[0] as f64 / img.down_size[0] as f64);
    let r = s.rect;
    let x0 = (r[0] * sx).round().max(0.0) as usize;
    let y0 = (r[1] * sy).round().max(0.0) as usize;
    let x1 = (((r[0] + r[2]) * sx).round() as usize).min(img.size[1]);
    let y1 = (((r[1] + r[3]) * sy).round() as usize).min(img.size[0]);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let rect = Rect::new(x0, y0, x1 - x0, y1 - y0);
    let (path, ch) = (ws.paths().conv_path(id), s.channel);
    s.view_req = app.jobs.request_id();
    let req = s.view_req;
    app.jobs.spawn(move || {
        let result = ImageFile::open(&path)
            .and_then(|tf| tf.read_channel(ch, rect))
            .map(|c| {
                let a = imadjust_u16(&c);
                vec![[a.clone(), a.clone(), a]]
            })
            .map_err(|e| format!("{e:#}"));
        Msg::Pixels { req, result }
    });
}

fn convert_selected(app: &mut App, ids: &[String]) {
    let token = app.jobs.convert_token();
    let mut tasks = vec![];
    let Some(ws) = app.ws.as_mut() else { return };
    let paths = ws.paths();
    for id in ids {
        let Some(img) = ws.image_mut(id) else { continue };
        if img.status != ConvertStatus::Unconverted {
            continue;
        }
        let (conv, down) = (paths.conv_path(id), paths.down_path(id));
        // Files from an earlier, interrupted session only need their metadata read.
        if let Ok(md) = convert::read_metadata(&conv, &down) {
            img.size = md.size;
            img.down_size = md.down_size;
            img.channel_count = md.channel_count;
            img.set_default_channels();
            if let Some(px) = md.pixel_dims {
                img.pixel_dims = px;
            }
            img.status = ConvertStatus::Converted;
            continue;
        }
        img.status = ConvertStatus::Converting;
        img.progress = 0.0;
        tasks.push(ConvertTask { id: id.clone(), source: img.source.clone(), series: img.series, conv, down, cancel: token.clone() });
    }
    app.mark_dirty();
    if !tasks.is_empty() {
        app.jobs.convert(tasks);
    }
}

fn cancel_all(app: &mut App) {
    app.jobs.convert_cancel.cancel();
    let cleared = app.jobs.convert.clear();
    if let Some(ws) = app.ws.as_mut() {
        for t in cleared {
            if let Some(img) = ws.image_mut(&t.id) {
                img.status = ConvertStatus::Unconverted;
                img.progress = 0.0;
            }
        }
    }
    app.mark_dirty();
}

/// Column weights shared by the apply row and the table.
const WEIGHTS: [f32; 8] = [2.6, 1.0, 1.0, 1.0, 1.1, 1.1, 1.0, 1.0];
const HEADERS: [(&str, &str); 8] = [
    ("Image", ""),
    ("Registration", "Channel used to register the section to the atlas (e.g. DAPI)"),
    ("Cell Marker", "Channel segmented into cells (e.g. IBA1 or GFAP)"),
    ("Co-Marker", "Channel measured inside cells (e.g. CD68 or S100B)"),
    ("Pixel Height (µm)", "Read from the image metadata when available; check it"),
    ("Pixel Width (µm)", "Read from the image metadata when available; check it"),
    ("Converted", ""),
    ("Progress", ""),
];

pub fn ui(app: &mut App, ui: &mut egui::Ui, s: &mut State) {
    let Some(ws) = app.ws.as_ref() else { return };
    let ids: Vec<String> = ws.images.iter().map(|i| i.id.clone()).collect();
    s.sel.retain(&ids);
    if s.apply_ch == [0, 0, 0] {
        s.apply_ch = [1, 2, 3];
        s.apply_px = [1.0, 1.0];
    }

    egui::TopBottomPanel::top("select_top")
        .resizable(true)
        .default_height(ui.available_height() * 0.42)
        .min_height(140.0)
        .show_inside(ui, |ui| {
            let widths = column_widths(ui, &WEIGHTS);
            let mut apply = false;
            aligned_row(ui, &widths, |i, ui| match i {
                0 => {
                    apply = ui
                        .button("Apply Channels")
                        .on_hover_text("Apply these channels and pixel size to the selected images")
                        .clicked()
                }
                1..=3 => {
                    ui.add(egui::DragValue::new(&mut s.apply_ch[i - 1]).range(1..=32));
                }
                4 | 5 => {
                    ui.add(egui::DragValue::new(&mut s.apply_px[i - 4]).speed(0.001).range(0.0001..=1000.0).max_decimals(4));
                }
                _ => {}
            });
            if apply {
                apply_channels(app, s);
            }
            ui.add_space(4.0);
            image_table(app, ui, s, &ids, &widths);
        });

    ui.add_space(6.0);
    let clicked = button_row(
        ui,
        &[
            ("Add Images", true, "Add microscopy images; each image in a multi-image file (e.g. a Leica project) is added separately"),
            ("Select All", true, ""),
            ("Remove Selected", !s.sel.keys.is_empty(), "Remove from the workspace (files on disk are kept)"),
            ("Convert Selected", !s.sel.keys.is_empty(), "Convert to TIFF and make the downsampled overview"),
            ("Cancel All", app.jobs.convert.busy(), "Stop all conversions"),
            ("Refresh Metadata", !s.sel.keys.is_empty(), "Re-read size, channels and pixel size"),
        ],
    );
    match clicked {
        Some(0) => {
            if let Some(files) = rfd::FileDialog::new().set_title("Select your images").pick_files() {
                add_images(app, files);
            }
        }
        Some(1) => s.sel.select_all(&ids),
        Some(2) => {
            let ws = app.ws.as_mut().unwrap();
            ws.images.retain(|i| !s.sel.contains(&i.id) || i.status == ConvertStatus::Converting);
            app.mark_dirty();
        }
        Some(3) => {
            let sel = s.sel.ordered(&ids);
            convert_selected(app, &sel);
        }
        Some(4) => cancel_all(app),
        Some(5) => refresh_metadata(app, s),
        _ => {}
    }
    ui.add_space(6.0);
    viewers(app, ui, s);
}

/// Adds files, one entry per image they contain. Files no reader recognises are
/// reported instead of failing later at conversion.
fn add_images(app: &mut App, files: Vec<std::path::PathBuf>) {
    let mut entries = vec![];
    let mut unreadable = vec![];
    for f in files {
        match imageio::probe(&f) {
            Ok((format, series)) => {
                log::info!("{}: {format}, {} image(s)", f.display(), series.len());
                let multi = series.len() > 1;
                for s in series {
                    let name = multi.then(|| s.name.clone().unwrap_or_else(|| format!("series {}", s.index + 1)));
                    entries.push(ImageEntry::new_series(f.clone(), s.index, name, multi));
                }
            }
            Err(e) => unreadable.push(format!("{}: {e:#}", f.display())),
        }
    }
    let skipped = app.ws.as_mut().unwrap().add_entries(entries);
    if !skipped.is_empty() {
        log::warn!("already in the workspace: {}", skipped.join(", "));
    }
    if !unreadable.is_empty() {
        app.error(format!(
            "These files could not be read (supported formats: {}):\n{}",
            imageio::readable_formats().join(", "),
            unreadable.join("\n")
        ));
    }
    app.mark_dirty();
}

fn apply_channels(app: &mut App, s: &State) {
    let ws = app.ws.as_mut().unwrap();
    let mut bad = vec![];
    for img in ws.images.iter_mut().filter(|i| s.sel.contains(&i.id)) {
        if img.valid_channels(s.apply_ch) {
            [img.registration_channel, img.cell_channel, img.co_channel] = s.apply_ch;
            img.pixel_dims = s.apply_px;
        } else {
            bad.push(img.id.clone());
        }
    }
    if !bad.is_empty() {
        log::warn!("channel indices out of range for: {}", bad.join(", "));
    }
    app.mark_dirty();
}

fn refresh_metadata(app: &mut App, s: &State) {
    let ws = app.ws.as_mut().unwrap();
    let paths = ws.paths();
    let mut errs = vec![];
    for img in ws.images.iter_mut().filter(|i| s.sel.contains(&i.id) && i.status == ConvertStatus::Converted) {
        match convert::read_metadata(&paths.conv_path(&img.id), &paths.down_path(&img.id)) {
            Ok(md) => {
                img.size = md.size;
                img.down_size = md.down_size;
                img.channel_count = md.channel_count;
                img.set_default_channels();
                img.pixel_dims = md.pixel_dims.unwrap_or([1.0, 1.0]);
            }
            Err(e) => errs.push(format!("{}: {e:#}", img.id)),
        }
    }
    for e in errs {
        app.error(e);
    }
    app.mark_dirty();
}

fn image_table(app: &mut App, ui: &mut egui::Ui, s: &mut State, ids: &[String], widths: &[f32]) {
    let mut clicked = None;
    let mut changed = false;
    let ws = app.ws.as_mut().unwrap();
    let mut tb = TableBuilder::new(ui)
        .id_salt("images")
        .striped(true)
        .sense(egui::Sense::click())
        .auto_shrink([false, false])
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    for &w in widths {
        tb = tb.column(Column::exact(w).clip(true));
    }
    tb.header(ROW_H, |mut h| {
        for (t, tip) in HEADERS {
            h.col(|ui| header(ui, t, tip));
        }
    })
    .body(|body| {
        body.rows(ROW_H, ws.images.len(), |mut row| {
            let i = row.index();
            let img = &mut ws.images[i];
            row.set_selected(s.sel.contains(&img.id));
            let editable = img.status == ConvertStatus::Converted;
            let n = img.channel_count.max(1);
            row.col(|ui| {
                ui.label(Workspace::display_name(img)).on_hover_text(img.source.display().to_string());
            });
            for ch in [&mut img.registration_channel, &mut img.cell_channel, &mut img.co_channel] {
                row.col(|ui| {
                    ui.add_enabled_ui(editable, |ui| changed |= fill(ui, egui::DragValue::new(ch).range(1..=n)).on_hover_text(format!("1 – {n}")).changed());
                });
            }
            for k in 0..2 {
                row.col(|ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut img.pixel_dims[k]).speed(0.001).range(0.0001..=1000.0).max_decimals(4))
                        .changed();
                });
            }
            row.col(|ui| match img.status {
                ConvertStatus::Unconverted => status_label(ui, "Unconverted", StatusKind::Idle),
                ConvertStatus::Converting => status_label(ui, "Converting", StatusKind::Busy),
                ConvertStatus::Converted => status_label(ui, "Converted", StatusKind::Done),
            });
            row.col(|ui| {
                let p = if img.status == ConvertStatus::Converted { 100.0 } else { img.progress };
                ui.add(egui::ProgressBar::new(p / 100.0).text(format!("{p:.0}%")));
            });
            if row.response().clicked() {
                clicked = Some(i);
            }
        });
    });
    if changed {
        app.mark_dirty();
    }
    if let Some(i) = clicked {
        let mods = ui.input(|i| i.modifiers);
        s.sel.click(i, ids, mods);
    }
}

/// Overview (left, with the movable inspection rectangle) and full-resolution view
/// (right) of the focused image.
fn viewers(app: &mut App, ui: &mut egui::Ui, s: &mut State) {
    let avail = ui.available_size();
    let half = egui::vec2((avail.x - ui.spacing().item_spacing.x) / 2.0, avail.y);
    let (left, right) = ui
        .horizontal(|ui| (pane(ui, half), pane(ui, half)))
        .inner;

    let focus = s.sel.focus.clone();
    let img = focus.as_ref().and_then(|id| app.ws.as_ref().unwrap().image(id).cloned());
    let Some(img) = img else {
        pane_message(ui, left, "Select an image");
        return;
    };
    let id = img.id.clone();
    if img.status != ConvertStatus::Converted {
        pane_message(ui, left, "Convert the image to view it");
        return;
    }
    let n = img.channel_count.max(1);
    s.channel %= n;
    let key = format!("{id}|{}", s.channel);
    if !s.thumb.is(&key) {
        match read_down(&app.ws.as_ref().unwrap().paths().down_path(&id), s.channel + 1) {
            Ok(d) => s.thumb.set(ui.ctx(), key, color_image(&[&d])),
            Err(e) => {
                s.thumb.clear();
                log::warn!("{id}: {e:#}");
            }
        }
    }
    if s.rect_for != id {
        s.rect = centred_rect(img.down_size[1] as f64, img.down_size[0] as f64, 0.1);
        s.rect_for = id.clone();
        s.view.clear();
        s.view_wanted.clear();
    }

    let bar = pane_strip(left, ROW_H, false);
    let img_area = egui::Rect::from_min_max(left.min + egui::vec2(8.0, 8.0), egui::pos2(left.max.x - 8.0, bar.min.y - 6.0));
    let mut moved = false;
    if let Some((resp, fit)) = image_in(ui, &s.thumb, img_area, "select_thumb") {
        let bounds = [s.thumb.size[0] as f64, s.thumb.size[1] as f64];
        moved = subview_rect(ui, &resp, &fit, &mut s.rect, bounds);
        resp.on_hover_text("Drag the rectangle to inspect; scroll to resize");
    }
    let mut c = pane_ui(ui, bar, egui::Layout::left_to_right(egui::Align::Center));
    if c.add(egui::Button::new(format!("Channel {}/{}", s.channel + 1, n)).min_size(egui::vec2(140.0, ROW_H))).clicked() {
        s.channel = (s.channel + 1) % n;
    }
    c.label(egui::RichText::new(format!("{} × {} px", img.size[1], img.size[0])).color(egui::Color32::from_gray(170)));

    if moved || s.view_wanted.is_empty() || !s.view_wanted.starts_with(&format!("{id}|{}|", s.channel)) {
        request_view(app, s, &id);
    }
    if image_in(ui, &s.view, right.shrink(8.0), "select_view").is_none() {
        pane_message(ui, right, "loading…");
    }
}
