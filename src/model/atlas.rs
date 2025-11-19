use serde::Deserialize;
use std::{collections::HashMap, fs};
use tiff::TiffError;

use crate::utility::imops::{get_slice, matrix_vec_to_volume};
use crate::utility::io::read_tiff_region;
use crate::utility::types::{Matrix, Volume};

const ATLAS_READ_ERR: TiffError =
    TiffError::FormatError(tiff::TiffFormatError::InvalidDimensions(0, 0));

#[derive(Debug)]
pub struct Atlas {
    reference: Volume<u16>,
    annotation: Volume<u16>,
    size: (usize, usize, usize),
    idx_map: HashMap<u64, Vec<u64>>,
    abr_map: HashMap<String, u64>,
}

impl Atlas {
    pub fn new(app_dir: String) -> Result<Atlas, TiffError> {
        let ref_path = format!("{}/assets/reference.tiff", app_dir);
        let ann_path = format!("{}/assets/annotation.tiff", app_dir);
        let str_path = format!("{}/assets/structures.csv", app_dir);

        let reference = read_tiff_region(&ref_path, (0, 0, 160, 228), 1)?;
        let reference = matrix_vec_to_volume(&reference).ok_or(ATLAS_READ_ERR)?;

        let annotation = read_tiff_region(&ann_path, (0, 0, 160, 228), 1)?;
        let annotation = matrix_vec_to_volume(&annotation).ok_or(ATLAS_READ_ERR)?;

        let s_reader = fs::File::open(str_path)?;
        let s_table = csv::Reader::from_reader(s_reader)
            .deserialize::<StructureRow>()
            .filter_map(|a| {
                println!("{:?}", a);
                a.ok()
            })
            .collect();

        let idx_map = Atlas::create_idx_map(&s_table);
        let abr_map = Atlas::create_abr_map(&s_table);

        Ok(Atlas {
            reference: reference,
            annotation: annotation,
            size: (264, 160, 228),
            idx_map: idx_map,
            abr_map: abr_map,
        })
    }

    pub fn get_reference_img(&self, ori: Orientation, idx: isize) -> Matrix<u16> {
        match ori {
            Orientation::Axial => get_slice(&self.reference, idx, 0),
            Orientation::Sagittal => get_slice(&self.reference, idx, 2),
            Orientation::Coronal => get_slice(&self.reference, idx, 1),
        }
    }

    pub fn n_slices(&self, ori: Orientation) -> usize {
        let (a, c, s) = self.reference.dim();
        match ori {
            Orientation::Axial => a,
            Orientation::Sagittal => s,
            Orientation::Coronal => c,
        }
    }

    fn create_idx_map(s_table: &Vec<StructureRow>) -> HashMap<u64, Vec<u64>> {
        s_table
            .iter()
            .map(|a| (a.id, Atlas::child_idxs_from_table(s_table, a.id)))
            .collect()
    }

    fn create_abr_map(s_table: &Vec<StructureRow>) -> HashMap<String, u64> {
        s_table
            .iter()
            .map(|a| (a.acronym.to_string(), a.id))
            .collect()
    }

    fn child_idxs_from_table(s_table: &Vec<StructureRow>, idx: u64) -> Vec<u64> {
        let finder = |j| {
            s_table
                .iter()
                .filter(|r| r.parent_structure_id as u64 == j)
                .map(|r| r.id)
                .collect()
        };

        let mut children: Vec<u64> = finder(idx);
        let mut i = 0;

        while i < children.len() {
            let sub_idx = children[i];
            let subchildren = finder(sub_idx);
            children = [children, subchildren].concat();
            i += 1;
        }

        return children;
    }
}

#[derive(Deserialize, Debug)]
struct StructureRow {
    acronym: String,
    id: u64,
    name: String,
    structure_id_path: String,
    parent_structure_id: f64,
}

#[derive(Copy, Clone)]
pub enum Orientation {
    Axial,
    Sagittal,
    Coronal,
}
