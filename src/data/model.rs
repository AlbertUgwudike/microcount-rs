use iced::futures::io;
use rfd::FileDialog;
use std::io::Error;
use std::path::{Path, PathBuf};
use std::{fs};

use crate::data::Workspace;

#[derive(Debug)]
pub struct Model {
    workspace: Option<Workspace>,
}

impl Model {
    pub fn new<'a>() -> Model {
        Model {
            workspace: None,
        }
    }

    pub fn create_workspace(&self) -> io::Result<()> {
        
        let folder_option = FileDialog::new()
            .set_directory("/")
            .save_file();
        
        let folder = match folder_option {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::NotADirectory, ""))
        };

        fs::create_dir(folder.to_owned());

        let ws = Workspace::new(folder.to_str().unwrap().into());
        let ws_s = serde_json::to_string(&ws).unwrap();
        fs::write(folder.join("ws.json"), ws_s);

        let join_path = |slug: &str| folder.join(slug);

        fs::create_dir(join_path("ws_converted"));
        fs::create_dir(join_path("ws_downsampled"));
        fs::create_dir(join_path("ws_processed"));
        fs::create_dir(join_path("ws_masks"))
    }

    pub fn load_workspace(&mut self) -> io::Result<()> {
        
        let folder_option = FileDialog::new()
            .set_directory("/")
            .pick_folder();
        
        let ws_dir = match folder_option {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::NotADirectory, ""))
        };

        let ws_s = match fs::read(Path::new(&ws_dir).join("ws.json")) {
            Ok(v) => match String::from_utf8(v) {
                Ok(s) => s,
                Err(err) => return Err(Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
            },
            Err(err) => return Err(err)
        };

        let ws = match serde_json::from_str::<Workspace>(&ws_s) {
            Ok(w) => w,
            Err(err) => return Err(Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
        };

        self.workspace = Some(ws);

        Ok(())
    }
}
