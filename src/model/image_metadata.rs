use std::{io, ops::Div, path::Path};

use ome_bioformats_rs::format_in::{tiff_reader::TiffReader, FormatReader};
use serde::{Deserialize, Serialize};

use crate::model::{
    constants::DIR_DOWN, transformation::Direction, Region, Transformation, DIR_CONVERT,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Raw {
    pub conversion_status: ConvertStatus,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Converted<S> {
    size: (usize, usize),
    down_size: (usize, usize),
    pub direction: Direction,

    pub channel_count: usize,
    pub registration_channel: usize,
    pub cell_channel: usize,
    pub comarker_channel: usize,

    pub registration_status: S,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Unregistered {}

#[derive(Serialize, Deserialize, Debug)]
pub enum Registered {
    Affine {
        transformation: Transformation,
        regions: Vec<Region>,
    },
    WholeImage {
        region: Region,
    },
}

impl Registered {
    pub fn to_str(&self) -> String {
        match self {
            Self::Affine { .. } => "Affine".into(),
            Self::WholeImage { .. } => "Whole Image".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ImageMetadata<S> {
    source_fn: String,
    img_id: String,
    img_ws_dir: String,

    pub state: S,
}

impl<T> ImageMetadata<T> {
    pub fn src_fn(&self) -> String {
        self.source_fn.clone()
    }
}

impl<T> SourceFn for ImageMetadata<T> {
    fn id(&self) -> String {
        self.img_id.clone()
    }

    fn ws_dir(&self) -> String {
        self.img_ws_dir.clone()
    }
}

impl ImageMetadata<Raw> {
    pub fn new(source_fn: &str, ws_dir: &str) -> Self {
        Self {
            source_fn: source_fn.to_owned(),
            img_id: Path::new(source_fn)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned(),
            img_ws_dir: ws_dir.to_owned(),
            state: Raw {
                conversion_status: ConvertStatus::Unconverted,
            },
        }
    }

    pub fn set_metadata(self) -> io::Result<ImageMetadata<Converted<Unregistered>>> {
        let conv_fn = self.conv_fn();
        TiffReader::new(conv_fn.into()).map(|im| {
            let md = im.metadata();
            let dim_zero = md.dimensions(0).unwrap();
            let size = (dim_zero.h as usize, dim_zero.w as usize);
            let down_size = (size.0.div(25), size.1.div_ceil(25));
            let channel_count = if md.series_count() > 0 {
                md.series_count()
            } else {
                dim_zero.c as usize
            };
            let registration_channel = 0;
            let cell_channel = 1 % (1 + channel_count);
            let comarker_channel = 2 % (1 + channel_count);

            ImageMetadata {
                source_fn: self.source_fn,
                img_id: self.img_id,
                img_ws_dir: self.img_ws_dir,
                state: Converted {
                    size,
                    down_size,
                    direction: Direction::North,
                    channel_count,
                    registration_channel,
                    cell_channel,
                    comarker_channel,
                    registration_status: Unregistered {},
                },
            }
        })
    }
}

impl<T> ImageMetadata<Converted<T>> {
    pub fn update_cell_channel(&mut self, v: String) {
        let _ = str::parse::<usize>(&v).map(|v| {
            if v < self.state.channel_count {
                self.state.cell_channel = v
            }
        });
    }

    pub fn update_comarker_channel(&mut self, v: String) {
        let _ = str::parse::<usize>(&v).map(|v| {
            if v < self.state.channel_count {
                self.state.comarker_channel = v
            }
        });
    }

    pub fn update_reg_channel(&mut self, v: String) {
        let _ = str::parse::<usize>(&v).map(|v| {
            if v < self.state.channel_count {
                self.state.registration_channel = v
            }
        });
    }

    pub fn rotate(&mut self) {
        self.state.direction = self.state.direction.rotate()
    }

    pub fn size(&self) -> (usize, usize) {
        match self.state.direction {
            Direction::North | Direction::South => self.state.size,
            Direction::East | Direction::West => (self.state.size.1, self.state.size.0),
        }
    }

    pub fn raw_size(&self) -> (usize, usize) {
        self.state.size
    }

    pub fn down_size(&self) -> (usize, usize) {
        match self.state.direction {
            Direction::North | Direction::South => self.state.down_size,
            Direction::East | Direction::West => (self.state.down_size.1, self.state.down_size.0),
        }
    }
}

impl ImageMetadata<Converted<Registered>> {
    pub fn registration_status(&self) -> &Registered {
        &self.state.registration_status
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ConvertStatus {
    Unconverted,
    Converting(f64),
    Converted,
}

impl ConvertStatus {
    pub fn to_str(&self) -> String {
        match self {
            Self::Unconverted => "Unconverted".into(),
            Self::Converting(p) => format!("Converting: {}", p.round().to_string()),
            Self::Converted => "Converted, Downsampling ....".into(),
        }
    }
}

pub trait SourceFn {
    fn id(&self) -> String;
    fn ws_dir(&self) -> String;
}

pub trait ConvFn: SourceFn {
    fn conv_fn(&self) -> String {
        format!("{}/{}/{}_conv.tiff", self.ws_dir(), DIR_CONVERT, self.id())
    }
}

impl ConvFn for ImageMetadata<Raw> {}
impl<T> ConvFn for ImageMetadata<Converted<T>> {}

pub trait DownFn: SourceFn {
    fn down_fn(&self) -> String {
        format!("{}/{}/{}_down.tiff", self.ws_dir(), DIR_DOWN, self.id())
    }
}

impl DownFn for ImageMetadata<Raw> {}
impl<T> DownFn for ImageMetadata<Converted<T>> {}
