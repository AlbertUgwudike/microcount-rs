use iced::widget::button::{Appearance, StyleSheet};
use iced::widget::image::Handle;
use iced::widget::{button, column, container, row, Button, Image};
use iced::Alignment::Center;
use iced::{Background, Border, Color, Element, Renderer, Theme};

use crate::message::{HomeMessage, Message};

use HomeMessage::{CreateWorkspace, LoadWorkspace};
use Message::Home;

#[derive(Debug, Default)]
pub struct HomePage;

impl HomePage {
    pub fn new() -> Self {
        HomePage {}
    }

    pub fn view<'a>(&self) -> Element<'a, Message> {
        let handle = Handle::from_path("./src/assets/microcount_logo.png");
        let image = Image::new(handle).height(200).width(200);

        container(
            column![
                image,
                row![
                    ui_button("Load Workspace").on_press(Home(LoadWorkspace)),
                    button("Create Workspace").on_press(Home(CreateWorkspace))
                ]
                .padding(10)
            ]
            .padding(20)
            .align_items(Center),
        )
        .into()
    }
}

struct CustomButtonStylesheet {}

impl StyleSheet for CustomButtonStylesheet {
    type Style = iced::Theme;

    fn active(&self, style: &Self::Style) -> Appearance {
        Appearance {
            border: Border::with_radius(10),
            shadow: Default::default(),
            shadow_offset: Default::default(),
            background: Some(Background::Color(Color::new(0.0, 1.0, 1.0, 1.0))),
            text_color: Color::new(1.0, 0.0, 0.0, 1.0),
        }
    }
}

pub fn ui_button<'a, Message>(content: &'static str) -> Button<'a, Message> {
    button(content).style(iced::theme::Button::Custom(Box::new(
        CustomButtonStylesheet {},
    )))
}
