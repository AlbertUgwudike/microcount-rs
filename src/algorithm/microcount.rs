use crate::utility::{
    io::{read_tiff_region, save_as_binary, save_as_luma8, save_as_rgb_bool},
    types::{Pnt, Results, Settings, ROI},
};

use crate::algorithm::{
    binary::{branch_length, conncomps, count_branches, perimeter, skel},
    helpers::{nearest, scholl},
    proc::pacefilt,
    regions::{centroids, floodfill, label2regions, regions2label, rotunditiy},
};

use ndarray::prelude::*;
use scirs2_ndimage::morphology::binary_opening;

pub fn from_fn(file_name: &str, roi: ROI, settings: Settings) -> Results {
    let channels = read_tiff_region(file_name, roi, 1).expect("ReadFailure");
    let iba1 = &(channels[2].map(|&a| a as f64));
    let cd68 = &(channels[1].map(|&a| a as f64));

    // Segment activation
    let cd68_log = (cd68 + 0.000001).log10();
    let cd68_norm = (&cd68_log - cd68_log.mean().unwrap()) / cd68_log.std(0.0);
    let cd68_th = cd68_norm.map(|a| *a > settings.co_marker_threshold);
    let cd68_blobs = conncomps(&cd68_th);
    let cd68_regions = cd68_blobs
        .iter()
        .filter(|&a| a.len() < settings.max_co_marker_size)
        .map(|a| a.to_owned())
        .collect();

    // Segment Somas
    let iba1_log = (iba1 + 0.000001).log10();
    let iba1_norm = (&iba1_log - iba1_log.mean().unwrap()) / iba1_log.std(0.0);
    let soma = iba1_norm.map(|a| *a > settings.soma_threshold);
    let soma_mask =
        binary_opening(&soma, None, Some(5), None, None, None, None).expect("Open Error");

    // Segment microglia
    let iba1_scale = iba1 / iba1.fold(0.0, |acc, a| if acc > *a { acc } else { *a });
    let eig1 = pacefilt(&iba1_scale, 17, 5.0);
    let branches = eig1.map(|a| *a > settings.cell_marker_threshold);

    // Separate microglia
    let regions = conncomps(&soma_mask);
    let markers = regions.iter().map(centroids).collect::<Vec<Pnt>>();

    let mut centroid_mask = Array2::from_elem(soma_mask.dim(), 0);
    markers.iter().for_each(|r| centroid_mask[(r.0, r.1)] = 1);

    let segmented_regions = floodfill(&branches, &markers);
    let cell_count = segmented_regions.len();
    let segmented = regions2label(&segmented_regions, soma_mask.dim());
    let skelly = skel(&branches);
    let poly = perimeter(&segmented);

    // Analyse morphology
    let rotundities = regions.iter().map(rotunditiy);
    let average_rotundity = Array1::from_iter(rotundities).mean().unwrap();
    let detected = count_branches(&skelly);

    let labelled_skelly = skelly.map(|&a| if a { 1 } else { 0 }) * &segmented;
    let skelly_regions = label2regions(&labelled_skelly);

    let skelly_markers = skelly_regions
        .iter()
        .zip(markers)
        .map(|(a, b)| nearest(b, a))
        .filter_map(|a| a.cloned())
        .collect::<Vec<Pnt>>();

    let (length_img, man_hists) = branch_length(&labelled_skelly, &skelly_markers);
    let length_img = length_img.map(|&a| a as u32);
    let av_lengths_total = man_hists.iter().fold(0, |acc, a| {
        let weighted_sum = a.iter().enumerate().fold(0, |acc, (a, &b)| acc + a * b);
        let sum = a.iter().fold(0, |a, &b| a + b);
        acc + (weighted_sum / sum)
    });
    let average_branch_length = av_lengths_total as f64 / cell_count as f64;

    let scholl_idxs = skelly_regions
        .iter()
        .zip(skelly_markers)
        .map(|(a, b)| scholl(b, a.to_vec()));

    let average_scholl = scholl_idxs.fold(0.0, |acc, a| acc + a) / cell_count as f64;

    // Analyse activation
    let mut cd68_mask_label = regions2label(&cd68_regions, cd68.dim());
    let cd68_mask = cd68_mask_label.map(|&a| a > 0);
    let cd68_count = cd68_mask_label
        .map(|&a| if a > 0 { 1.0 } else { 0.0 })
        .sum();
    let percentage_cd68_area = 100.0 * cd68_count / (cd68_mask_label.iter().count() as f64);

    branches
        .indexed_iter()
        .for_each(|(pt, &a)| cd68_mask_label[pt] = if a { cd68_mask_label[pt] } else { 0 });

    let overlap_regions = label2regions(&cd68_mask_label);
    let n_overlap = overlap_regions
        .iter()
        .zip(segmented_regions)
        .map(|(a, b)| 100.0 * a.len() as f64 / b.len() as f64)
        .filter(|&a| a > settings.overlap_percentage_threshold)
        .count() as f64;

    let percentage_cd68_num = 100.0 * n_overlap / cell_count as f64;

    // save images
    save_as_binary(&cd68_mask, "./assets/cd68_mask.tif");
    save_as_binary(&soma_mask, "./assets/somas.tif");
    save_as_binary(&branches, "./assets/branches.tif");
    save_as_luma8(&segmented, "./assets/segmented.tif");
    save_as_binary(&skelly, "./assets/skelly.tif");
    save_as_luma8(&poly, "./assets/poly.tif");
    save_as_rgb_bool(&skelly, &detected, &detected, "./assets/overlay.tif");
    save_as_binary(&detected, "./assets/detected.tif");
    save_as_luma8(&length_img, "./assets/branch_length.tif");

    Results {
        cell_count,
        average_rotundity,
        average_branch_length,
        average_scholl,
        percentage_cd68_area,
        percentage_cd68_num,
    }
}
