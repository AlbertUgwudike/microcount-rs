use iced::widget::{button, container, row};
use iced::Element;

use crate::message::{Message, NavigationMessage};
use Message::Navigate;
use NavigationMessage::{GoToRegister, GoToSelectImages};

#[derive(Default, Debug)]
pub struct NavigatorBar;

impl NavigatorBar {
    pub fn new() -> Self {
        NavigatorBar {}
    }

    pub fn view<'a>(&self) -> Element<'a, Message> {
        container(
            row![
                button("Select Images")
                    .on_press(Navigate(GoToSelectImages))
                    .padding(5),
                button("Register")
                    .on_press(Navigate(GoToRegister))
                    .padding(5)
            ]
            .padding(20),
        )
        .into()
    }
}
