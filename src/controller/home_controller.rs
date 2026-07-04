use std::{
    fs,
    io::{self, Error},
    path::Path,
};

use eframe::egui::Ui;
use rfd::FileDialog;

use crate::{
    model::{constants, Model, Workspace},
    view::home_view,
};

pub struct HomeState {
    pub dir_name: String,
}

impl HomeState {
    pub fn new(dir_name: String) -> Self {
        Self { dir_name }
    }
}

pub struct HomeController<'a> {
    model: &'a mut Model,
    pub state: &'a mut HomeState,
}

impl<'a> HomeController<'a> {
    pub fn render(model: &'a mut Model, state: &'a mut HomeState, ui: &mut Ui) {
        let mut con = Self { model, state };
        home_view::ui_tab_home(&mut con, ui);
    }

    pub fn load_workspace(&mut self) -> io::Result<()> {
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

        self.model.workspace = ws;
        self.model.workspace_loaded = true;

        Ok(())
    }

    pub fn create_workspace(&self) -> io::Result<()> {
        let folder_option = FileDialog::new().set_directory("/").save_file();

        let folder = match folder_option {
            Some(f) => f,
            None => return Err(Error::new(std::io::ErrorKind::NotADirectory, "")),
        };

        fs::create_dir(folder.to_owned())?;

        let ws = Workspace::new(folder.to_str().unwrap().into());
        let ws_s = serde_json::to_string(&ws).unwrap();
        fs::write(folder.join("ws.json"), ws_s)?;

        let join_path = |slug: &str| folder.join(slug);

        fs::create_dir(join_path(constants::DIR_CONVERT))?;
        fs::create_dir(join_path(constants::DIR_DOWN))?;
        fs::create_dir(join_path(constants::DIR_PROC))?;
        fs::create_dir(join_path(constants::DIR_MASK))
    }
}
