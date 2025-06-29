use iced::widget::image::{viewer, Handle, Viewer};
use iced::widget::{button, column, container, Image};
use iced::Alignment::Center;
use iced::Element;
use iced::Length;
use serde::{Serialize, Deserialize};
use rfd::FileDialog;
use std::{fs, os, path};

use crate::message::{Message, HomeMessage};

use Message::Home;
use HomeMessage::{CreateWorkspace, LoadWorkspace};

#[derive(Debug, Default)]
pub struct HomePage {
    value: i64,
}

impl HomePage {
    pub fn new() -> Self {
        HomePage { value: 0 }
    }

    pub fn update(&mut self, message: HomeMessage) {
        match message {
            HomeMessage::CreateWorkspace => {
                self.create_workspace()
            }
            HomeMessage::LoadWorkspace => {
                self.value -= 1;
            }
        }
    }

    pub fn view<'a>(&self) -> Element<'a, Message> {

        let handle = Handle::from_path("/Users/vaness/projects/microcount-rs/src/assets/microcount_logo.png");
        let image = Image::new(handle).height(200).width(200);

        container(
            column![
                image,
                button("Load Workspace").on_press(Home(LoadWorkspace)),
                button("Create Workspace").on_press(Home(CreateWorkspace))
            ]
            .padding(20)
            .align_items(Center),
        )
        .into()
    }

    fn create_workspace(&self) {
        let folder = FileDialog::new()
            .set_directory("/")
            .save_file().unwrap();

        // fs::create_dir(folder);

        // let ws_file = folder.as_ref().join("ws.json");

        print!("{:?}", folder);
    }
}
