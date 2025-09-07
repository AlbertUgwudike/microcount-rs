use ndarray::prelude::*;
use ndarray::OwnedRepr;

pub type Pnt = (usize, usize);
pub type ROI = (usize, usize, usize, usize);
pub type Matrix<T> = ArrayBase<OwnedRepr<T>, Dim<[usize; 2]>>;
pub type Volume<T> = ArrayBase<OwnedRepr<T>, Dim<[usize; 3]>>;

pub struct Settings {
    pub cell_marker_threshold: f64,
    pub co_marker_threshold: f64,
    pub overlap_percentage_threshold: f64,
    pub soma_threshold: f64,
    pub max_co_marker_size: usize,
}

pub struct Results {
    pub cell_count: usize,
    pub average_rotundity: f64,
    pub average_branch_length: f64,
    pub average_scholl: f64,
    pub percentage_cd68_area: f64,
    pub percentage_cd68_num: f64,
}

impl std::fmt::Debug for Results {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        println!("Cell Count:\t{:?}", self.cell_count);
        println!("Rotundity:\t{:?}", self.average_rotundity);
        println!("Branch Length:\t{:?}px", self.average_branch_length);
        println!("Scholl Index:\t{:?}", self.average_scholl);
        println!("CoM Area (%):\t{:?}", self.percentage_cd68_area);
        println!("CoM Num (%):\t{:?}", self.percentage_cd68_num);
        Ok(())
    }
}
