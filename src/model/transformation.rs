use serde::{Deserialize, Serialize};

use crate::model::{atlas::Orientation, image_metadata::Converted, Atlas, ImageMetadata};

#[derive(Copy, Clone, Serialize, Deserialize, Debug)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn rotate(&self) -> Self {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
        }
    }

    pub fn rotate_n(&self, n: usize) -> Self {
        let mut out = self.clone();
        for _ in 0..n {
            out = out.rotate()
        }
        out
    }

    pub fn n_rotations(&self) -> usize {
        match self {
            Direction::North => 0,
            Direction::East => 1,
            Direction::South => 2,
            Direction::West => 3,
        }
    }

    pub fn reciprocal(&self) -> Self {
        match self {
            Direction::North => Direction::North,
            Direction::East => Direction::West,
            Direction::South => Direction::South,
            Direction::West => Direction::East,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MaskGenerator {
    Atlas {
        direction: Direction,
        registration_data: RegistrationData,
        region_key: RegionKey,
        laterality: Laterality,
    },
    Whole,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegistrationData {
    pub affine_matrix: [f32; 6],
    pub orientation: Orientation,
    pub slice_idx: usize,
    pub hist_hex: [(f32, f32); 6],
    pub atlas_hex: [(f32, f32); 6],
}

impl RegistrationData {
    pub fn new(
        affine_matrix: [f32; 6],
        orientation: Orientation,
        slice_idx: usize,
        hist_hex: [(f32, f32); 6],
        atlas_hex: [(f32, f32); 6],
    ) -> Self {
        Self {
            affine_matrix,
            orientation,
            slice_idx,
            hist_hex,
            atlas_hex,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RegionKey {
    AUD,
    HIP,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Laterality {
    Left,
    Right,
}
