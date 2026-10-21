//! Analyse: per-region thresholds, previews, batch processing, result inspection and
//! export (`AnalyseView` / `AnalyseController`).

use super::fmt;
use crate::core::analysis::data::MicrocountResult;
use crate::core::analysis::{Algorithm, RegionSettings};
use crate::core::image::{Image, Mask};
use crate::core::imgproc::morph;
use crate::core::pipeline::{self, NormCache, Preview};
use crate::core::tiff::TiffFile;
use crate::core::workspace::{ProcessStatus, Region};
use crate::gui::jobs::{Msg, ProcessTask};
use crate::gui::widgets::{
    aligned_row, button_row, column_widths, header, image_in, pane, pane_message, pane_strip, pane_ui, status_label, subview_rect,
    centred_rect, fill, Selection, StatusKind, Tex, ROW_H,
};
use crate::gui::App;
use egui_extras::{Column, TableBuilder};

const OVERLAYS: [&str; 4] = ["Cell Marker", "CoMarker", "Cell Marker Poly", "CoMarker Poly"];

pub struct State {
    pub(crate) sel: Selection,
    thumb: Tex,
    thumb_for: String,
    rect: [f64; 4],
    view: Tex,
    wanted: String,
    view_req: u64,
    pages: Option<(String, Vec<[Image<u16>; 3]>)>,
    overlay: [bool; 4],
    preview: Option<Preview>,
    preview_req: u64,
    preview_busy: bool,
    norm: Option<NormCache>,
    apply: RegionSettings,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sel: Selection::default(),
            thumb: Tex::default(),
            thumb_for: String::new(),
            rect: [0.0; 4],
            view: Tex::default(),
            wanted: String::new(),
            view_req: 0,
            pages: None,
            overlay: [true, false, true, false],
            preview: None,
            preview_req: 0,
            preview_busy: false,
            norm: None,
            apply: RegionSettings::default(),
        }
    }
}

impl State {
    pub fn invalidate_view(&mut self, rid: &str) {
        if self.wanted.starts_with(&format!("{rid}|")) {
            self.wanted.clear();
            self.pages = None;
        }
        if self.preview.as_ref().is_some_and(|p| p.region_id == rid) {
            self.preview = None;
        }
    }
}

pub fn on_pixels(app: &mut App, req: u64, result: Result<Vec<[Image<u16>; 3]>, String>) {
    let s = &mut app.analyse;
    if req != s.view_req {
        return;
    }
    match result {
        Ok(p) => s.pages = Some((s.wanted.clone(), p)),
        Err(e) => log::warn!("{e}"),
    }
}

pub fn on_preview(app: &mut App, req: u64, result: Result<(Preview, NormCache), String>) {
    if req != app.analyse.preview_req {
        return;
    }
    app.analyse.preview_busy = false;
    match result {
        Ok((p, cache)) => {
            let (rid, crop) = (p.region_id.clone(), p.crop);
            app.analyse.preview = Some(p);
            app.analyse.norm = Some(cache);
            if let Some(r) = app.region_mut(&rid) {
                if r.crop.is_none() {
                    r.crop = Some(crop);
                    app.mark_dirty();
                }
            }
        }
        Err(e) => {
            if e != "cancelled" {
                app.error(format!("Preview failed: {e}"))
            }
        }
    }
}

fn compose(pages: &[[Image<u16>; 3]], flags: [bool; 4]) -> egui::ColorImage {
    let (w, h) = (pages[0][0].w, pages[0][0].h);
    let co_poly = if flags[3] { Some(morph::dilate(&pages[3][0].map(|v| v > 0), 2)) } else { None };
    let mut rgba = Vec::with_capacity(w * h * 4);
    for i in 0..w * h {
        let mut c = [0u32; 3];
        if flags[0] {
            c = [pages[0][0].data[i] as u32, pages[0][1].data[i] as u32, pages[0][2].data[i] as u32];
        }
        if flags[1] {
            c[1] = (c[1] + pages[1][0].data[i] as u32).min(65535);
        }
        if flags[2] {
            if pages[2].iter().any(|p| p.data[i] > 0) {
                c = [3 * 257, 86 * 257, 252 * 257];
            }
            if pages[4][0].data[i] > 0 {
                c = [0, 65535, 0];
            }
        }
        if co_poly.as_ref().is_some_and(|m: &Mask| m.data[i]) {
            c = [60000; 3];
        }
        rgba.extend_from_slice(&[(c[0] >> 8) as u8, (c[1] >> 8) as u8, (c[2] >> 8) as u8, 255]);
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

fn rect_key(r: &[f64; 4]) -> [i64; 4] {
    r.map(|v| (v * 10.0).round() as i64)
}

fn status_text(r: &Region) -> &'static str {
    match r.status {
        ProcessStatus::Unprocessed => "Unprocessed",
        ProcessStatus::Queued => "Queued",
        ProcessStatus::Processing => "Processing",
        ProcessStatus::Processed => "Processed",
    }
}

fn process_selected(app: &mut App, rids: &[String]) {
    let Some(atlas) = app.atlas.clone() else { return };
    let token = app.jobs.process_token();
    let ws = app.ws.as_mut().unwrap();
    let (paths, algo) = (ws.paths(), ws.algorithm);
    let mut tasks = vec![];
    for rid in rids {
        let Some((ii, ri)) = ws.find_region(rid) else { continue };
        let img = ws.images[ii].clone();
        let region = &mut ws.images[ii].regions[ri];
        if matches!(region.status, ProcessStatus::Queued | ProcessStatus::Processing) {
            continue;
        }
        region.status = ProcessStatus::Queued;
        let (region, paths, atlas) = (region.clone(), paths.clone(), atlas.clone());
        tasks.push(ProcessTask {
            rid: rid.clone(),
            run: Box::new(move |cancel| pipeline::process_region(&paths, &atlas, &img, &region, algo, cancel)),
            cancel: token.clone(),
        });
    }
    log::info!("queued {} region(s) with {}", tasks.len(), algo.label());
    app.mark_dirty();
    app.jobs.process(tasks);
}

fn cancel(app: &mut App) {
    app.jobs.process_cancel.cancel();
    let cleared = app.jobs.process.clear();
    for t in cleared {
        if let Some(r) = app.region_mut(&t.rid) {
            r.status = if r.result.is_some() { ProcessStatus::Processed } else { ProcessStatus::Unprocessed };
        }
    }
    app.mark_dirty();
}

fn export(app: &mut App, s: &State, rids: &[String]) {
    let ws = app.ws.as_ref().unwrap();
    let chosen: Vec<&String> = if s.sel.keys.is_empty() { rids.iter().collect() } else { rids.iter().filter(|r| s.sel.contains(r)).collect() };
    let rows: Vec<_> = chosen
        .iter()
        .filter_map(|rid| ws.find_region(rid))
        .map(|(ii, ri)| (&ws.images[ii], &ws.images[ii].regions[ri]))
        .filter(|(_, r)| r.result.is_some())
        .collect();
    if rows.is_empty() {
        app.error("No processed regions to export.");
        return;
    }
    let start = ws.dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    if let Some(path) = rfd::FileDialog::new().set_title("Export Processed Image Data").set_directory(start).set_file_name("data.csv").add_filter("CSV", &["csv"]).save_file() {
        match pipeline::export_csv(&ws.paths(), &rows, &path) {
            Ok(()) => log::info!("exported {} region(s) to {}", rows.len(), path.display()),
            Err(e) => app.error(format!("{e:#}")),
        }
    }
}

const WEIGHTS: [f32; 7] = [2.4, 1.0, 1.0, 1.0, 1.0, 1.0, 1.3];
const TIP_BRANCH: &str = "Normalised intensity threshold for branches/processes (values mostly lie between −2.5 and +2.5). Higher = fewer processes.";
const TIP_CO: &str = "Threshold for the co-marker channel. Higher = less background but weaker co-marker signal is missed.";
const TIP_MAX: &str = "Largest connected co-marker object kept, in pixels. Larger objects are treated as background or artefacts.";
const TIP_OVERLAP: &str = "Minimum % of a cell's area covered by co-marker for the cell to count as co-marker positive.";
const TIP_SOMA: &str = "Normalised intensity threshold for somas. Higher = smaller or fewer somas. Set this first: it decides which cells are counted.";
const HEADERS: [(&str, &str); 7] = [
    ("RegionID", ""),
    ("Branch Threshold", TIP_BRANCH),
    ("CoMarker Threshold", TIP_CO),
    ("Max CoMarker Size", TIP_MAX),
    ("Overlap Percentage", TIP_OVERLAP),
    ("Soma Threshold", TIP_SOMA),
    ("Processed", ""),
];

fn status_kind(r: &Region) -> StatusKind {
    match r.status {
        ProcessStatus::Unprocessed => StatusKind::Idle,
        ProcessStatus::Queued => StatusKind::Waiting,
        ProcessStatus::Processing => StatusKind::Busy,
        ProcessStatus::Processed => StatusKind::Done,
    }
}

fn settings_cells(ui: &mut egui::Ui, i: usize, st: &mut RegionSettings) -> bool {
    match i {
        1 => fill(ui, egui::DragValue::new(&mut st.cell_threshold).speed(0.01)).on_hover_text(TIP_BRANCH).changed(),
        2 => fill(ui, egui::DragValue::new(&mut st.comarker_threshold).speed(0.01)).on_hover_text(TIP_CO).changed(),
        3 => fill(ui, egui::DragValue::new(&mut st.max_comarker_size).speed(10.0)).on_hover_text(TIP_MAX).changed(),
        4 => fill(ui, egui::DragValue::new(&mut st.overlap_percentage).speed(0.1)).on_hover_text(TIP_OVERLAP).changed(),
        5 => fill(ui, egui::DragValue::new(&mut st.soma_threshold).speed(0.01)).on_hover_text(TIP_SOMA).changed(),
        _ => false,
    }
}

pub fn ui(app: &mut App, ui: &mut egui::Ui, s: &mut State) {
    let Some(ws) = app.ws.as_ref() else { return };
    let all = ws.all_regions();
    let rids: Vec<String> = all.iter().map(|&(ii, ri)| ws.images[ii].regions[ri].id(&ws.images[ii].id)).collect();
    s.sel.retain(&rids);

    egui::TopBottomPanel::top("analyse_top")
        .resizable(true)
        .default_height(ui.available_height() * 0.42)
        .min_height(140.0)
        .show_inside(ui, |ui| {
            let widths = column_widths(ui, &WEIGHTS);
            let mut apply = false;
            let mut algo = app.ws.as_ref().unwrap().algorithm;
            aligned_row(ui, &widths, |i, ui| match i {
                0 => apply = ui.button("Apply Setting").on_hover_text("Apply these thresholds to the selected regions").clicked(),
                6 => {
                    egui::ComboBox::from_id_salt("algo").selected_text(algo.label()).width(ui.available_width()).show_ui(ui, |ui| {
                        for a in Algorithm::ALL {
                            ui.selectable_value(&mut algo, a, a.label());
                        }
                    });
                }
                _ => {
                    settings_cells(ui, i, &mut s.apply);
                }
            });
            if algo != app.ws.as_ref().unwrap().algorithm {
                app.ws.as_mut().unwrap().algorithm = algo;
                s.preview = None;
                app.mark_dirty();
            }
            if apply {
                let ws = app.ws.as_mut().unwrap();
                for (&(ii, ri), rid) in all.iter().zip(&rids) {
                    if s.sel.contains(rid) {
                        ws.images[ii].regions[ri].settings = s.apply;
                    }
                }
                app.mark_dirty();
            }
            ui.add_space(4.0);
            region_table(app, ui, s, &all, &rids, &widths);
        });

    ui.add_space(6.0);
    let some = !s.sel.keys.is_empty();
    let can_preview = app.atlas.is_some() && s.sel.focus.is_some() && !s.preview_busy;
    let busy = app.jobs.process.busy();
    match button_row(
        ui,
        &[
            ("Select All", true, ""),
            ("Select Unprocessed", true, ""),
            ("Process Selected", app.atlas.is_some() && some, "Process the selected regions in the background"),
            (if s.preview_busy { "Previewing…" } else { "Process Preview" }, can_preview, "Run the algorithm on the rectangle in the thumbnail, normalised over the whole region"),
            ("Cancel", busy, "Stop processing"),
            ("Export Processed", true, "Save the results of processed regions (selected ones, or all) as CSV"),
        ],
    ) {
        Some(0) => s.sel.select_all(&rids),
        Some(1) => {
            let ws = app.ws.as_ref().unwrap();
            s.sel.set(all.iter().zip(&rids).filter(|(&(ii, ri), _)| ws.images[ii].regions[ri].status != ProcessStatus::Processed).map(|(_, r)| r.clone()));
        }
        Some(2) => {
            let sel = s.sel.ordered(&rids);
            process_selected(app, &sel);
        }
        Some(3) => start_preview(app, s),
        Some(4) => cancel(app),
        Some(5) => export(app, s, &rids),
        _ => {}
    }
    ui.add_space(6.0);
    viewers(app, ui, s);
}

fn region_table(app: &mut App, ui: &mut egui::Ui, s: &mut State, all: &[(usize, usize)], rids: &[String], widths: &[f32]) {
    let mut clicked = None;
    let mut changed = false;
    let ws = app.ws.as_mut().unwrap();
    let mut tb = TableBuilder::new(ui)
        .id_salt("regions")
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
        body.rows(ROW_H, all.len(), |mut row| {
            let i = row.index();
            let (ii, ri) = all[i];
            let r = &mut ws.images[ii].regions[ri];
            row.set_selected(s.sel.contains(&rids[i]));
            row.col(|ui| {
                ui.label(&rids[i]).on_hover_text(&rids[i]);
            });
            for c in 1..=5 {
                row.col(|ui| changed |= settings_cells(ui, c, &mut r.settings));
            }
            row.col(|ui| {
                if r.status == ProcessStatus::Processing {
                    ui.spinner();
                }
                status_label(ui, status_text(r), status_kind(r));
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
        s.sel.click(i, rids, mods);
    }
}

fn start_preview(app: &mut App, s: &mut State) {
    let Some(rid) = s.sel.focus.clone() else { return };
    let ws = app.ws.as_ref().unwrap();
    let Some((ii, ri)) = ws.find_region(&rid) else { return };
    let (img, region) = (ws.images[ii].clone(), ws.images[ii].regions[ri].clone());
    let (paths, algo, atlas) = (ws.paths(), ws.algorithm, app.atlas.clone().unwrap());
    let (thumb, cache) = (s.rect, s.norm.clone());
    s.preview_req = app.jobs.request_id();
    s.preview_busy = true;
    let req = s.preview_req;
    let cancel = crate::core::Cancel::default();
    app.jobs.spawn(move || Msg::Preview {
        req,
        result: pipeline::preview(&paths, &atlas, &img, &region, algo, thumb, cache, &cancel).map_err(|e| crate::gui::jobs::err_string(&e)),
    });
}

fn results_column(ui: &mut egui::Ui, rect: egui::Rect, title: &str, r: Option<&MicrocountResult>) {
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(10.0)).layout(egui::Layout::top_down(egui::Align::Center)));
    c.label(egui::RichText::new(title).strong());
    c.add_space(6.0);
    let v = |f: fn(&MicrocountResult) -> f64| r.map(f).map(fmt).unwrap_or_else(|| "--".into());
    let items: [(&str, String); 10] = [
        ("Cell Area (%)", v(|r| r.percentage_cell_area)),
        ("Cell Density (/mm²)", v(|r| r.cell_density_per_mm2)),
        ("CoMarker (% Area)", v(|r| r.percentage_comarker_area)),
        ("CoMarker (% Number)", v(|r| r.percentage_comarker_num)),
        ("Branch Points", v(|r| r.average_branch_point_count)),
        ("Branch Length (µm)", v(|r| r.average_branch_length_um_per_branch)),
        ("Convexity", v(|r| r.average_convexity)),
        ("Scholl Index", v(|r| r.average_scholl_index)),
        ("Soma Size (µm²)", v(|r| r.average_soma_size_um2)),
        ("Cells", r.map(|r| r.cell_count.to_string()).unwrap_or_else(|| "--".into())),
    ];
    let gap = ((rect.height() - 60.0) / items.len() as f32 - 34.0).clamp(2.0, 18.0);
    for (label, value) in items {
        c.label(egui::RichText::new(label).small().weak());
        c.label(egui::RichText::new(value).size(15.0).strong());
        c.add_space(gap);
    }
}

/// Region thumbnail with the inspection rectangle, the processed view with overlay
/// toggles, and the results column.
fn viewers(app: &mut App, ui: &mut egui::Ui, s: &mut State) {
    let avail = ui.available_size();
    let sp = ui.spacing().item_spacing.x;
    let results_w = 190.0f32.min(avail.x * 0.25);
    let pane_w = (avail.x - results_w - 2.0 * sp) / 2.0;
    let (thumb_r, view_r, res_r) = ui
        .horizontal(|ui| {
            let a = pane(ui, egui::vec2(pane_w, avail.y));
            let b = pane(ui, egui::vec2(pane_w, avail.y));
            let (c, _) = ui.allocate_exact_size(egui::vec2(results_w, avail.y), egui::Sense::hover());
            ui.painter().rect_stroke(c, 4.0, ui.visuals().widgets.noninteractive.bg_stroke, egui::StrokeKind::Inside);
            (a, b, c)
        })
        .inner;

    let Some(rid) = s.sel.focus.clone() else {
        pane_message(ui, thumb_r, "Select a region");
        results_column(ui, res_r, "Results", None);
        return;
    };
    let ws = app.ws.as_ref().unwrap();
    let Some((ii, ri)) = ws.find_region(&rid) else { return };
    let (img, region) = (ws.images[ii].clone(), ws.images[ii].regions[ri].clone());
    let paths = ws.paths();

    if s.thumb_for != rid {
        s.thumb_for = rid.clone();
        s.wanted.clear();
        s.pages = None;
        match TiffFile::open(paths.mask_path(&rid)).and_then(|t| t.read_page::<u16>(0)) {
            Ok(c) => {
                s.rect = centred_rect(c[0].w as f64, c[0].h as f64, 0.2);
                let refs: Vec<&Image<u16>> = c.iter().collect();
                s.thumb.set(ui.ctx(), rid.clone(), crate::gui::widgets::color_image(&refs));
            }
            Err(e) => {
                s.thumb.clear();
                log::warn!("{rid}: thumbnail missing ({e:#})");
            }
        }
    }
    let mut moved = false;
    if let Some((resp, fit)) = image_in(ui, &s.thumb, thumb_r.shrink(8.0), "analyse_thumb") {
        let b = [s.thumb.size[0] as f64, s.thumb.size[1] as f64];
        moved = subview_rect(ui, &resp, &fit, &mut s.rect, b);
        resp.on_hover_text("Drag the rectangle to choose what to view or preview; scroll to resize");
    }

    // Pixels to show: a matching preview, else the processed output image.
    let preview_here = s.preview.as_ref().filter(|p| p.region_id == rid && rect_key(&p.thumb) == rect_key(&s.rect));
    let key = format!("{rid}|{:?}", rect_key(&s.rect));
    let mut source = "";
    let pages: Option<&Vec<[Image<u16>; 3]>> = if let Some(p) = preview_here {
        source = "preview";
        Some(&p.pages)
    } else if region.result.is_some() {
        if let (Some(dn), Some(crop)) = (region.dn_rect, region.crop) {
            if s.wanted != key || moved {
                s.wanted = key.clone();
                let rel = pipeline::thumb_to_crop(&img, dn, crop, s.rect);
                s.view_req = app.jobs.request_id();
                let req = s.view_req;
                let paths = paths.clone();
                let rid2 = rid.clone();
                app.jobs.spawn(move || Msg::Pixels { req, result: pipeline::read_processed(&paths, &rid2, rel).map_err(|e| format!("{e:#}")) });
            }
        }
        source = "processed";
        s.pages.as_ref().filter(|(k, _)| *k == key).map(|(_, p)| p)
    } else {
        None
    };
    let bar = pane_strip(view_r, ROW_H, false);
    let view_area = egui::Rect::from_min_max(view_r.min + egui::vec2(8.0, 8.0), egui::pos2(view_r.max.x - 8.0, bar.min.y - 6.0));
    match pages {
        Some(p) => {
            let vkey = format!("{source}|{key}|{:?}", s.overlay);
            if !s.view.is(&vkey) {
                s.view.set(ui.ctx(), vkey, compose(p, s.overlay));
            }
            image_in(ui, &s.view, view_area, "analyse_view");
        }
        None if s.preview_busy => pane_message(ui, view_area, "running preview…"),
        None if region.result.is_some() => pane_message(ui, view_area, "loading…"),
        None => pane_message(ui, view_area, "Not processed — use Process Preview or Process Selected"),
    }
    let mut c = pane_ui(ui, bar, egui::Layout::left_to_right(egui::Align::Center));
    let sp = c.spacing().item_spacing.x;
    let w = ((bar.width() - 3.0 * sp) / 4.0).floor();
    for (k, name) in OVERLAYS.iter().enumerate() {
        let on = s.overlay[k];
        if c.add(egui::Button::new(*name).selected(on).min_size(egui::vec2(w, ROW_H))).clicked() {
            s.overlay[k] = !on;
        }
    }
    if let Some(p) = preview_here {
        results_column(ui, res_r, "Preview (not saved)", Some(&p.result));
    } else {
        results_column(ui, res_r, "Whole region", region.result.as_ref());
    }
}
