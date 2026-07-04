use eframe::egui::{self, Color32, Context, TextureHandle, Ui, Window};
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

pub fn egui_display_gray(ctx: &Context, mat: &Matrix<u8>) -> TextureHandle {
    let im = array2buff(mat.map(|&p| std::cmp::min(p, 255) as u8));
    let (h, w) = mat.dim();
    let pixels = im.as_flat_samples();
    let im = egui::ColorImage::from_gray([w, h], pixels.as_slice());
    ctx.load_texture("name", im, Default::default())
}

pub fn egui_display_rgb(ctx: &Context, mat: &Volume<u8>) -> TextureHandle {
    let mats = volume_to_matrix_vec(mat, (0, 1, 2));
    let (h, w) = mats[0].dim();
    let pixels: Vec<u8> = izip!(mats[0].clone(), mats[1].clone(), mats[2].clone())
        .map(|(a, b, c)| [a, b, c])
        .flatten()
        .map(|a| std::cmp::min(255, a) as u8)
        .collect();
    let im = egui::ColorImage::from_rgb([w, h], &pixels);
    ctx.load_texture("name", im, Default::default())
}
