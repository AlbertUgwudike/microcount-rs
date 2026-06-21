use serde::{Deserialize, Serialize};

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
pub struct Transformation {
    affine_matrix: [f64; 9],
    direction: Direction,
}
