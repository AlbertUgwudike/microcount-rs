use eframe::egui::{self, Color32, Ui, Window};
use itertools::izip;

use crate::utility::{
    imops::{array2buff, volume_to_matrix_vec},
    types::{Matrix, Volume},
};

pub fn black_box(ui: &mut Ui, name: &str, add_contents: impl FnOnce(&mut Ui) -> ()) {
    Window::new(name.to_string())
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

pub fn egui_display_gray(ui: &mut Ui, mat: &Matrix<u8>) {
    let im = array2buff(mat.map(|&p| std::cmp::min(p, 255) as u8));
    let (h, w) = mat.dim();
    let pixels = im.as_flat_samples();
    let im = egui::ColorImage::from_gray([w, h], pixels.as_slice());
    let h = ui.ctx().load_texture("name", im, Default::default());
    ui.image(&h);
}

pub fn egui_display_rgb(ui: &mut Ui, mat: &Volume<u8>) {
    let mats = volume_to_matrix_vec(mat, (0, 1, 2));
    let (h, w) = mats[0].dim();
    let pixels: Vec<u8> = izip!(mats[0].clone(), mats[1].clone(), mats[2].clone())
        .map(|(a, b, c)| [a, b, c])
        .flatten()
        .map(|a| std::cmp::min(255, a) as u8)
        .collect();
    let im = egui::ColorImage::from_rgb([w, h], &pixels);
    let h = ui.ctx().load_texture("name", im, Default::default());
    ui.image(&h);
}
