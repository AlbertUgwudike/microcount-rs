use iced::widget::{button, column, container, text};
use iced::Alignment::Center;
use iced::Element;

use crate::message::{Message, RegisterMessage};

use Message::Register;
use RegisterMessage::{Decrement, Increment};

#[derive(Default, Debug)]
pub struct RegisterPage {
    value: i64,
}

impl RegisterPage {
    pub fn new() -> Self {
        RegisterPage { value: 0 }
    }

    pub fn update(&mut self, message: RegisterMessage) {
        match message {
            RegisterMessage::Increment => {
                self.value += 10;
            }
            RegisterMessage::Decrement => {
                self.value -= 5;
            }
        }
    }

    pub fn view<'a>(&self) -> Element<'a, Message> {
        container(
            column![
                button("Increment 10").on_press(Register(Increment)),
                text(self.value).size(50),
                button("Decrement 5").on_press(Register(Decrement))
            ]
            .padding(20)
            .align_items(Center),
        )
        .into()
    }
}
