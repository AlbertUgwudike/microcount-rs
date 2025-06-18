use iced::{Element};
use iced::widget::{container, column, button, text};
use iced::Alignment::{ Center };

#[derive(Debug, Clone)]
pub enum RegisterMessage {
    IncrementTen,
    DecrementFive,
    GoSelectImages
}

#[derive(Default, Debug)]
pub struct RegisterPage {
    value: i64,
}

impl RegisterPage {
    pub fn new() -> Self {
        RegisterPage { value: 0 }
    }
    
    pub fn update(&mut self, message: RegisterMessage){
        match message {
            RegisterMessage::IncrementTen => {
                self.value += 10;
            }
            RegisterMessage::DecrementFive => {
                self.value -= 5;
            }
            RegisterMessage::GoSelectImages => {
                println!("Go to select images page!")
            }
        }
    }

    pub fn view<'a>(&self) -> Element<'a, RegisterMessage> {
        container(
            column![
                button("Increment 10").on_press(RegisterMessage::IncrementTen),
                text(self.value).size(50),
                button("Decrement 5").on_press(RegisterMessage::DecrementFive),
                button("Go to select images").on_press(RegisterMessage::GoSelectImages)
            ]
            .padding(20)
            .align_items(Center)
        ).into()
    }
}