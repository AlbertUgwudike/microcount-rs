use eframe::egui::Context;
use rfd::FileDialog;
use std::future::Future;
use std::io::{self, Error};
use std::path::Path;
use std::pin::Pin;
use std::{fs, sync::Arc};
use tokio::sync::mpsc::Sender;

use crate::model::image_metadata::{Converted, Raw, Registered, SourceFn, Unregistered};
use crate::{
    model::{Atlas, ImageMetadata, Workspace},
    ThreadLabel, ThreadResponse,
};

type MSender = Sender<(
    Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    Option<ThreadLabel>,
)>;

type TSender = Arc<Sender<ThreadResponse>>;

// #[derive(Debug)]
pub struct Model {
    pub workspace_loaded: bool,
    pub workspace: Workspace,
    pub atlas: Atlas,
    pub sender: MSender,
    pub thread_sender: TSender,
    pub context: Context,
}

impl Model {
    pub fn new(
        app_dir: String,
        sender: MSender,
        thread_sender: TSender,
        context: Context,
    ) -> Model {
        Model {
            workspace_loaded: false,
            workspace: Workspace::default(),
            atlas: Atlas::new(app_dir).unwrap(),
            sender,
            thread_sender,
            context,
        }
    }

    pub fn get_dir_name(&self) -> String {
        self.workspace.dir_name.clone()
    }

    pub fn add_images(&mut self) -> Result<(), Error> {
        let file_option = FileDialog::new().set_directory("/").pick_files();
        match file_option {
            Some(files) => {
                files.iter().for_each(|file| {
                    let img = ImageMetadata::new(file.to_str().unwrap(), &self.workspace.dir_name);
                    self.workspace.raw_images.insert(img.id(), img);
                });

                self.save_workspace();
            }
            None => (),
        };
        Ok(())
    }

    pub fn is_converted(&self, im_id: &String) -> bool {
        self.workspace.converted_images.contains_key(im_id)
    }

    pub fn is_registered(&self, im_id: &String) -> bool {
        self.workspace.registered_images.contains_key(im_id)
    }

    pub fn raw_to_converted(&mut self, im_id: String) -> io::Result<()> {
        if let Some(old_im) = self.workspace.raw_images.remove(&im_id) {
            let new_im = old_im.set_metadata()?;
            self.workspace.converted_images.insert(im_id, new_im);
        }
        Ok(())
    }

    pub fn get_converted_image(
        &self,
        im_id: &String,
    ) -> Option<&ImageMetadata<Converted<Unregistered>>> {
        self.workspace.converted_images.get(im_id)
    }

    pub fn get_registered_image(
        &self,
        im_id: &String,
    ) -> Option<&ImageMetadata<Converted<Registered>>> {
        self.workspace.registered_images.get(im_id)
    }

    pub fn get_raw_image(&self, im_id: &String) -> Option<&ImageMetadata<Raw>> {
        self.workspace.raw_images.get(im_id)
    }

    pub fn save_workspace(&self) {
        let dir_name = self.get_dir_name().clone();
        let folder = Path::new(&dir_name);
        let ws_s = serde_json::to_string(&self.workspace).unwrap();
        fs::write(folder.join("ws.json"), ws_s).ok();
    }

    pub fn dispatch<F>(&self, repaint: bool, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if repaint {
            self.sender.try_send((Box::pin(f), None));
        } else {
            self.sender.try_send((Box::pin(f), None));
        }
    }

    pub fn dispatch_exclusive<F>(&self, label: ThreadLabel, repaint: bool, f: F)
    where
        F: Future<Output = ()> + Send + 'static,
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
