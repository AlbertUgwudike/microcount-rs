use iced::{Element};
use iced::widget::{container, column, button, text};
use iced::Alignment::{ Center };

#[derive(Debug, Clone)]
pub enum SelectImagesMessage {
    Increment,
    Decrement,
    GoRegister
}

#[derive(Default, Debug)]
pub struct SelectImagesPage {
    value: i64,
}

impl SelectImagesPage {
    pub fn new() -> Self {
        SelectImagesPage { value: 0 }
    }
    
    pub fn update(&mut self, message: SelectImagesMessage){
        match message {
            SelectImagesMessage::Increment => {
                self.value += 1;
            }
            SelectImagesMessage::Decrement => {
                self.value -= 1;
            }
            SelectImagesMessage::GoRegister => {
                println!("Go to register page!")
            }
        }
    }

    pub fn view<'a>(&self) -> Element<'a, SelectImagesMessage> {
        container(
            column![
                button("Increment").on_press(SelectImagesMessage::Increment),
                text(self.value).size(50),
                button("Decrement").on_press(SelectImagesMessage::Decrement),
                button("Go to select images").on_press(SelectImagesMessage::GoRegister)
            ]
            .padding(20)
            .align_items(Center)
        ).into()
    }
}