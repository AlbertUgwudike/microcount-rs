use eframe::egui::Context;
use rfd::FileDialog;
use std::cell::{Ref, RefCell};
use std::future::Future;
use std::io::{self, Error};
use std::path::Path;
use std::pin::{pin, Pin};
use std::rc::Rc;
use std::sync::Arc;
use std::{collections::HashSet, fs};
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

use crate::concurrency::ThreadPool;
use crate::model::{constants, Atlas, ConvertStatus, ImageMetadata, Workspace};
use crate::{ThreadLabel, ThreadResponse};

type MSender = Sender<(
    Pin<Box<dyn Future<Output = ThreadResponse> + Send + 'static>>,
    Option<ThreadLabel>,
)>;

// #[derive(Debug)]
pub struct Model {
    pub workspace: Option<Workspace>,
    pub atlas: Atlas,
    pub sender: MSender,
}

impl Model {
    pub fn new(app_dir: String, sender: MSender) -> Model {
        Model {
            workspace: None,
            atlas: Atlas::new(app_dir).unwrap(),
            sender,
        }
    }

    pub fn get_dir_name(&self) -> String {
        self.workspace
            .as_ref()
            .map_or("".into(), |ws| ws.dir_name.clone())
    }

    pub fn add_images(&mut self) -> Result<(), Error> {
        let file_option = FileDialog::new().set_directory("/").pick_files();
        if let Some(ws) = &mut self.workspace {
            match file_option {
                Some(files) => {
                    files.iter().for_each(|file| {
                        let mut img = ImageMetadata::new(file.to_str().unwrap(), &ws.dir_name);
                        let res = img.set_metadata();
                        if res.is_ok() {
                            ws.images.insert(img.id().to_string(), img);
                        }
                    });

                    self.save_workspace();
                }
                None => (),
            };
        }
        Ok(())
    }

    pub fn convert_and_downsample(&mut self, idx: &HashSet<String>) -> Result<(), Error> {
        if let Some(ws) = &mut self.workspace {
            idx.iter().for_each(|i| {
                ws.images.get_mut(i).map(|img| {
                    Self::convert(img);
                    Self::downsample(img);
                });
            });
        }
        Ok(())
    }

    fn convert(img: &mut ImageMetadata) {
        img.conversion_status = ConvertStatus::Converting;
        std::fs::copy(img.src_fn(), img.conv_fn());
        img.conversion_status = ConvertStatus::Converted;
    }

    fn downsample(img: &mut ImageMetadata) {}

    pub fn get_image(&self, im_id: &String) -> Option<&ImageMetadata> {
        self.workspace
            .as_ref()
            .map(|ws| ws.images.get(im_id))
            .flatten()
    }

    pub fn with_images<R>(&self, f: impl FnOnce(Vec<&ImageMetadata>) -> R) {
        if let Some(ws) = &self.workspace {
            let images: Vec<&ImageMetadata> = ws.images.values().collect();
            f(images);
        }
    }

    pub fn save_workspace(&self) {
        self.workspace.as_ref().map(|ws| {
            let dir_name = self.get_dir_name().clone();
            let folder = Path::new(&dir_name);
            let ws_s = serde_json::to_string(ws).unwrap();
            fs::write(folder.join("ws.json"), ws_s).ok();
        });
    }

    pub fn dispatch<F>(&self, repaint: bool, f: F)
    where
        F: Future<Output = ThreadResponse> + Send + 'static,
    {
        if repaint {
            self.sender.send((Box::pin(f), None));
        } else {
            self.sender.send((Box::pin(f), None));
        }
    }

    pub fn dispatch_exclusive<F>(&self, label: ThreadLabel, repaint: bool, f: F)
    where
        F: Future<Output = ThreadResponse> + Send + 'static,
    {
        if repaint {
            self.sender.try_send((Box::pin(f), Some(label)));
        } else {
            self.sender.try_send((Box::pin(f), Some(label)));
        }
    }

    // fn wrap<F>(&self, f: F) -> Box<dyn Future<Output = Response> + Send + 'static>
    // where
    //     F: Future<Output = Response> + Send + 'static,
    // {
    //     let frame = self.frame.clone();
    //     Box::new(async move {
    //         let t = f.await;
    //         frame.request_repatint();
    //         t
    //     })
    // }
}
