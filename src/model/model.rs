use eframe::egui::Context;
use rfd::FileDialog;
use std::future::Future;
use std::io::{self, Error};
use std::path::Path;
use std::sync::Arc;
use std::{collections::HashSet, fs};
use tokio::sync::Mutex;

use crate::concurrency::ThreadPool;
use crate::model::{constants, Atlas, ConvertStatus, ImageMetadata, Workspace};
use crate::ThreadLabel;

// #[derive(Debug)]
pub struct Model {
    pub workspace: Option<Arc<Mutex<Workspace>>>,
    pub atlas: Atlas,
    pub counter: Arc<Mutex<i32>>,
    pub threadpool: ThreadPool<ThreadLabel>,
    pub frame: Arc<Mutex<Context>>,
}

impl Model {
    pub fn new(app_dir: String, frame: Context) -> Model {
        Model {
            workspace: None,
            atlas: Atlas::new(app_dir).unwrap(),
            counter: Arc::new(Mutex::new(0)),
            threadpool: ThreadPool::new(5, 5),
            frame: Arc::new(Mutex::new(frame)),
        }
    }

    pub fn create_workspace(&self) -> Result<(), Error> {
        let folder_option = FileDialog::new().set_directory("/").save_file();

        let folder = match folder_option {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::NotADirectory, "")),
        };

        fs::create_dir(folder.to_owned());

        let ws = Workspace::new(folder.to_str().unwrap().into());
        let ws_s = serde_json::to_string(&ws).unwrap();
        fs::write(folder.join("ws.json"), ws_s);

        let join_path = |slug: &str| folder.join(slug);

        fs::create_dir(join_path(constants::DIR_CONVERT));
        fs::create_dir(join_path(constants::DIR_DOWN));
        fs::create_dir(join_path(constants::DIR_PROC));
        fs::create_dir(join_path(constants::DIR_MASK))
    }

    pub fn load_workspace(&mut self) -> Result<(), Error> {
        let folder_option = FileDialog::new().set_directory("/").pick_folder();

        let ws_dir = match folder_option {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::NotADirectory, "")),
        };

        let ws_s = match fs::read(Path::new(&ws_dir).join("ws.json")) {
            Ok(v) => match String::from_utf8(v) {
                Ok(s) => s,
                Err(err) => {
                    return Err(Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
                }
            },
            Err(err) => return Err(err),
        };

        let ws = match serde_json::from_str::<Workspace>(&ws_s) {
            Ok(w) => w,
            Err(err) => return Err(Error::new(std::io::ErrorKind::InvalidData, err.to_string())),
        };

        self.workspace = Some(Arc::new(Mutex::new(ws)));

        Ok(())
    }

    pub fn get_dir_name(&self) -> String {
        if let Some(ws) = &self.workspace {
            ws.try_lock()
                .map(|w| w.dir_name.clone())
                .unwrap_or("".into())
        } else {
            "".into()
        }
    }

    pub fn add_images(&mut self) -> Result<(), Error> {
        let file_option = FileDialog::new().set_directory("/").pick_files();
        if let Some(ws) = &self.workspace {
            let mut ws = ws.try_lock().map_err(|_| Error::other("error"))?;

            match file_option {
                Some(files) => {
                    files.iter().for_each(|file| {
                        let mut img = ImageMetadata::new(file.to_str().unwrap(), &ws.dir_name);
                        img.set_metadata();
                        ws.images.insert(img.src_fn().to_string(), img);
                    });

                    self.save_workspace();
                }
                None => (),
            };
        }
        Ok(())
    }

    pub fn convert_and_downsample(&mut self, idx: &HashSet<String>) -> Result<(), Error> {
        if let Some(ws) = &self.workspace {
            let mut ws = ws.try_lock().map_err(|_| Error::other("error"))?;
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

    pub fn get_all_images(&self) -> io::Result<Vec<ImageMetadata>> {
        if let Some(ws) = &self.workspace {
            let k = ws
                .try_lock()
                .map(|t| t.images.clone().into_values().collect());
            k.map_err(|_| Error::other("could not acquire lock"))
        } else {
            Err(Error::other("No workspace loaded!"))
        }
    }

    pub fn get_image(&self, id: &str) -> Option<ImageMetadata> {
        if let Some(ws) = &self.workspace {
            ws.try_lock()
                .map(|t| t.images.get(id).cloned())
                .ok()
                .flatten()
        } else {
            None
        }
    }

    fn save_workspace(&self) {
        self.workspace.as_ref().map(|ws| {
            let dir_name = self.get_dir_name().clone();
            let folder = Path::new(&dir_name);
            tokio::task::block_in_place(async move || {
                let ws_s = serde_json::to_string(&*ws.lock().await).unwrap();
                fs::write(folder.join("ws.json"), ws_s).ok();
            });
        });
    }

    pub fn dispatch<F>(&self, repaint: bool, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if repaint {
            self.threadpool.dispatch(f);
        } else {
            self.threadpool.dispatch(f);
        }
    }

    pub fn dispatch_exclusive<F>(&mut self, label: ThreadLabel, repaint: bool, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if repaint {
            self.threadpool.dispatch_exclusive(f, label);
        } else {
            self.threadpool.dispatch_exclusive(f, label);
        }
    }

    // fn wrap<F>(&self, f: F) -> Box<dyn Future<Output = ()> + Send + 'static>
    // where
    //     F: Future<Output = ()> + Send + 'static,
    // {
    //     let frame = Arc::clone(&self.frame);
    //     Box::new(move || {
    //         f();
    //         frame.lock().unwrap().request_repaint();
    //     })
    // }
}
