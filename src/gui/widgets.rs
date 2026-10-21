//! Reusable UI pieces: image textures and viewers, table selection, the draggable
//! sub-view rectangle and the control-point hexagon.

use crate::core::image::Image;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, TextureHandle, Vec2};
use std::collections::BTreeSet;

/// 16-bit channels (1 = grey, 3 = RGB) to an egui image.
pub fn color_image(chans: &[&Image<u16>]) -> egui::ColorImage {
    let (w, h) = (chans[0].w, chans[0].h);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for i in 0..w * h {
        let px = |c: usize| (chans[c.min(chans.len() - 1)].data[i] >> 8) as u8;
        if chans.len() >= 3 {
            rgba.extend_from_slice(&[px(0), px(1), px(2), 255]);
        } else {
            let v = px(0);
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

/// A texture plus the key of what it currently shows, so it is only re-uploaded when
/// the content changes.
#[derive(Default)]
pub struct Tex {
    pub handle: Option<TextureHandle>,
    pub key: String,
    pub size: [usize; 2],
}

impl Tex {
    pub fn set(&mut self, ctx: &egui::Context, key: String, img: egui::ColorImage) {
        self.size = img.size;
        match &mut self.handle {
            Some(h) => h.set(img, egui::TextureOptions::LINEAR),
            None => self.handle = Some(ctx.load_texture(key.clone(), img, egui::TextureOptions::LINEAR)),
        }
        self.key = key;
    }

    pub fn clear(&mut self) {
        self.handle = None;
        self.key.clear();
        self.size = [0, 0];
    }

    pub fn is(&self, key: &str) -> bool {
        self.handle.is_some() && self.key == key
    }
}

/// Mapping between image pixel coordinates (0-based, continuous, pixel edges at integer
/// values) and screen positions for an image drawn to fit a rectangle.
#[derive(Clone, Copy)]
pub struct Fit {
    pub rect: Rect,
    pub scale: f32,
}

impl Fit {
    pub fn to_screen(&self, x: f64, y: f64) -> Pos2 {
        self.rect.min + Vec2::new(x as f32, y as f32) * self.scale
    }

    pub fn to_image(&self, p: Pos2) -> (f64, f64) {
        let v = (p - self.rect.min) / self.scale;
        (v.x as f64, v.y as f64)
    }

    /// MATLAB world coordinates (pixel centres at 1, 2, ...) to screen.
    pub fn world_to_screen(&self, x: f64, y: f64) -> Pos2 {
        self.to_screen(x - 0.5, y - 0.5)
    }

    pub fn screen_to_world(&self, p: Pos2) -> (f64, f64) {
        let (x, y) = self.to_image(p);
        (x + 0.5, y + 0.5)
    }
}

/// Draws `tex` scaled to fit `avail`, returning the interaction response and mapping.
pub fn image_view(ui: &mut egui::Ui, tex: &Tex, avail: Vec2) -> Option<(Response, Fit)> {
    let handle = tex.handle.as_ref()?;
    let [w, h] = tex.size;
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (avail.x / w as f32).min(avail.y / h as f32).max(0.01);
    let size = Vec2::new(w as f32, h as f32) * scale;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    ui.painter_at(rect).image(
        handle.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    Some((resp, Fit { rect, scale }))
}

pub fn placeholder(ui: &mut egui::Ui, avail: Vec2, text: &str) {
    let (rect, _) = ui.allocate_exact_size(avail.max(Vec2::splat(50.0)), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(14.0), ui.visuals().weak_text_color());
}

/// Multi-row selection by key, supporting click, Cmd/Ctrl-click and Shift-click.
#[derive(Default, Clone)]
pub struct Selection {
    pub keys: BTreeSet<String>,
    anchor: Option<usize>,
    /// Most recently clicked key: the one shown in detail views.
    pub focus: Option<String>,
}

impl Selection {
    pub fn click(&mut self, idx: usize, all: &[String], mods: egui::Modifiers) {
        let key = all[idx].clone();
        if mods.shift {
            if let Some(a) = self.anchor.filter(|&a| a < all.len()) {
                let (lo, hi) = (a.min(idx), a.max(idx));
                if !mods.command {
                    self.keys.clear();
                }
                self.keys.extend(all[lo..=hi].iter().cloned());
                self.focus = Some(key);
                return;
            }
        }
        if mods.command {
            if !self.keys.remove(&key) {
                self.keys.insert(key.clone());
            }
        } else {
            self.keys.clear();
            self.keys.insert(key.clone());
        }
        self.anchor = Some(idx);
        self.focus = Some(key);
    }

    pub fn select_all(&mut self, all: &[String]) {
        self.keys = all.iter().cloned().collect();
        if self.focus.is_none() {
            self.focus = all.first().cloned();
        }
    }

    pub fn set(&mut self, keys: impl IntoIterator<Item = String>) {
        self.keys = keys.into_iter().collect();
        self.focus = self.keys.iter().next().cloned();
    }

    pub fn focus_on(&mut self, k: &str) {
        self.keys = [k.to_string()].into();
        self.focus = Some(k.to_string());
    }

    pub fn contains(&self, k: &str) -> bool {
        self.keys.contains(k)
    }

    /// Drops keys that no longer exist.
    pub fn retain(&mut self, all: &[String]) {
        self.keys.retain(|k| all.contains(k));
        if self.focus.as_ref().is_some_and(|f| !all.contains(f)) {
            self.focus = self.keys.iter().next().cloned();
        }
    }

    /// Selected keys in display order.
    pub fn ordered(&self, all: &[String]) -> Vec<String> {
        all.iter().filter(|k| self.keys.contains(*k)).cloned().collect()
    }
}

/// A rectangle in image pixels `[x, y, w, h]` that can be dragged around an image and
/// resized with the scroll wheel. Returns true when the user finished moving it.
pub fn subview_rect(ui: &egui::Ui, resp: &Response, fit: &Fit, r: &mut [f64; 4], bounds: [f64; 2]) -> bool {
    let mut changed = false;
    if resp.dragged() {
        let d = resp.drag_delta() / fit.scale;
        r[0] += d.x as f64;
        r[1] += d.y as f64;
    }
    if resp.drag_stopped() || resp.clicked() {
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (x, y) = fit.to_image(p);
                r[0] = x - r[2] / 2.0;
                r[1] = y - r[3] / 2.0;
            }
        }
        changed = true;
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            let f = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
            let (cx, cy) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            r[2] = (r[2] * f).clamp(4.0, bounds[0].max(4.0));
            r[3] = (r[3] * f).clamp(4.0, bounds[1].max(4.0));
            r[0] = cx - r[2] / 2.0;
            r[1] = cy - r[3] / 2.0;
            changed = true;
        }
    }
    r[2] = r[2].min(bounds[0]);
    r[3] = r[3].min(bounds[1]);
    r[0] = r[0].clamp(0.0, (bounds[0] - r[2]).max(0.0));
    r[1] = r[1].clamp(0.0, (bounds[1] - r[3]).max(0.0));
    let a = fit.to_screen(r[0], r[1]);
    let b = fit.to_screen(r[0] + r[2], r[1] + r[3]);
    let pts = vec![a, Pos2::new(b.x, a.y), b, Pos2::new(a.x, b.y)];
    ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(2.0f32, Color32::from_rgb(255, 210, 0))));
    changed
}

/// Editable six-point polygon in world coordinates. Drag a vertex to move it or drag
/// elsewhere to move the whole hexagon. Returns true while it is being changed.
pub fn hexagon(ui: &egui::Ui, resp: &Response, fit: &Fit, pts: &mut [[f64; 2]; 6], drag: &mut Option<usize>, colour: Color32) -> bool {
    let screen: Vec<Pos2> = pts.iter().map(|p| fit.world_to_screen(p[0], p[1])).collect();
    if resp.drag_started() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (i, d) = screen
                .iter()
                .enumerate()
                .map(|(i, s)| (i, s.distance(pos)))
                .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a });
            *drag = Some(if d < 12.0 { i } else { 6 });
        }
    }
    let mut changed = false;
    if resp.dragged() {
        if let Some(i) = *drag {
            let d = resp.drag_delta() / fit.scale;
            let (dx, dy) = (d.x as f64, d.y as f64);
            if i < 6 {
                pts[i][0] += dx;
                pts[i][1] += dy;
            } else {
                for p in pts.iter_mut() {
                    p[0] += dx;
                    p[1] += dy;
                }
            }
            changed = true;
        }
    }
    if resp.drag_stopped() {
        *drag = None;
    }
    let screen: Vec<Pos2> = pts.iter().map(|p| fit.world_to_screen(p[0], p[1])).collect();
    let painter = ui.painter_at(fit.rect.expand(40.0));
    painter.add(egui::Shape::closed_line(screen.clone(), Stroke::new(1.5f32, colour)));
    for (i, s) in screen.iter().enumerate() {
        painter.circle_filled(*s, 4.0, colour);
        painter.text(
            *s + Vec2::new(7.0, -7.0),
            egui::Align2::LEFT_BOTTOM,
            format!("V{}", i + 1),
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    }
    changed
}

// ---------------------------------------------------------------------------------------
// Layout helpers shared by the tabs
// ---------------------------------------------------------------------------------------

pub const ROW_H: f32 = 26.0;
/// Background of the image panes.
pub const PANE_BG: Color32 = Color32::from_rgb(14, 14, 16);

/// Splits `total` into columns proportional to `weights`, leaving room for the spacing
/// a table puts between columns, so field rows can line up with table columns.
pub fn column_widths(ui: &egui::Ui, weights: &[f32]) -> Vec<f32> {
    let sp = ui.spacing().item_spacing.x;
    let n = weights.len() as f32;
    let avail = (ui.available_width() - sp * (n - 1.0)).max(n * 40.0);
    let sum: f32 = weights.iter().sum();
    weights.iter().map(|w| (avail * w / sum).floor()).collect()
}

/// One cell per column width, spaced exactly like `TableBuilder` columns.
pub fn aligned_row(ui: &mut egui::Ui, widths: &[f32], mut cell: impl FnMut(usize, &mut egui::Ui)) {
    ui.horizontal(|ui| {
        for (i, &w) in widths.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ROW_H), Sense::hover());
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
            );
            cell(i, &mut child);
        }
    });
}

/// A full-width row of equally sized buttons `(label, enabled, tooltip)`; returns the
/// index of the clicked one.
pub fn button_row(ui: &mut egui::Ui, items: &[(&str, bool, &str)]) -> Option<usize> {
    let sp = ui.spacing().item_spacing.x;
    let n = items.len() as f32;
    let w = ((ui.available_width() - sp * (n - 1.0)) / n).floor();
    let mut clicked = None;
    ui.horizontal(|ui| {
        for (i, (label, enabled, tip)) in items.iter().enumerate() {
            let mut r = ui.add_enabled(*enabled, egui::Button::new(*label).min_size(Vec2::new(w, ROW_H)));
            if !tip.is_empty() {
                r = r.on_hover_text(*tip);
            }
            if r.clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// Allocates a dark image pane of `size` and returns its rectangle.
pub fn pane(ui: &mut egui::Ui, size: Vec2) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size.max(Vec2::splat(40.0)), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, PANE_BG);
    rect
}

/// Centred, dimmed message inside a pane.
pub fn pane_message(ui: &egui::Ui, rect: Rect, text: &str) {
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(14.0), Color32::from_gray(140));
}

/// Draws `tex` fitted and centred in `area`; the image is interactive (click and drag).
pub fn image_in(ui: &mut egui::Ui, tex: &Tex, area: Rect, id: &str) -> Option<(Response, Fit)> {
    let handle = tex.handle.as_ref()?;
    let [w, h] = tex.size;
    if w == 0 || h == 0 || area.width() < 4.0 || area.height() < 4.0 {
        return None;
    }
    let scale = (area.width() / w as f32).min(area.height() / h as f32);
    let rect = Rect::from_center_size(area.center(), Vec2::new(w as f32, h as f32) * scale);
    ui.painter().image(handle.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    let resp = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    Some((resp, Fit { rect, scale }))
}

/// A child UI laid over part of a pane, with dark visuals for docked controls.
pub fn pane_ui(ui: &mut egui::Ui, rect: Rect, layout: egui::Layout) -> egui::Ui {
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    c.style_mut().visuals = egui::Visuals::dark();
    c
}

/// Strip of `h` points at the top or bottom of a pane, inset from its edges.
pub fn pane_strip(rect: Rect, h: f32, top: bool) -> Rect {
    let r = rect.shrink(8.0);
    if top {
        Rect::from_min_size(r.min, Vec2::new(r.width(), h))
    } else {
        Rect::from_min_size(Pos2::new(r.min.x, r.max.y - h), Vec2::new(r.width(), h))
    }
}

/// Status text coloured by state.
pub fn status_label(ui: &mut egui::Ui, text: &str, kind: StatusKind) {
    let c = match kind {
        StatusKind::Done => Color32::from_rgb(40, 150, 70),
        StatusKind::Busy => Color32::from_rgb(210, 130, 20),
        StatusKind::Waiting => Color32::from_rgb(90, 120, 200),
        StatusKind::Idle => ui.visuals().weak_text_color(),
    };
    ui.label(egui::RichText::new(text).color(c));
}

#[derive(Clone, Copy)]
pub enum StatusKind {
    Done,
    Busy,
    Waiting,
    Idle,
}

/// Header cell with an optional explanatory tooltip.
pub fn header(ui: &mut egui::Ui, text: &str, tip: &str) {
    let r = ui.strong(text);
    if !tip.is_empty() {
        r.on_hover_text(tip);
    }
}

/// Adds `w` stretched to the full width of the current cell, so table cells match the
/// aligned field row above them.
pub fn fill(ui: &mut egui::Ui, w: impl egui::Widget) -> Response {
    ui.add_sized(Vec2::new(ui.available_width(), ROW_H - 4.0), w)
}

/// Default inspection rectangle: `frac` of the shorter side, centred.
pub fn centred_rect(w: f64, h: f64, frac: f64) -> [f64; 4] {
    let side = (w.min(h) * frac).max(4.0);
    [((w - side) / 2.0).max(0.0), ((h - side) / 2.0).max(0.0), side, side]
}
