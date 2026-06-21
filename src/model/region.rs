use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Region {
    key: bool,
    laterality: bool,
}
