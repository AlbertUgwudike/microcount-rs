use iced::{ Application, Command, Element, executor, Settings, Theme};
use iced::widget::{ container };

pub mod page;
use page::{ register, select_images };

use crate::page::register::RegisterMessage;
use crate::page::select_images::SelectImagesMessage;

pub fn main() -> iced::Result {
    Microcount::run(Settings::default())
}

#[derive(Debug)]
pub enum Page {
    SelectImages,
    Register
}

#[derive(Debug)]
pub enum Message {
    GoSelectImages,
    IncrementSelectImages,
    DecrementSelectImage,

    GoRegister,
    IncrementRegister,
    DecrementRegister,
}

#[derive(Debug)]
struct Microcount {
    select_images_page: select_images::SelectImagesPage,
    register_page: register::RegisterPage,
    selected_page: Page
}

impl Application for Microcount {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let select_images_page = select_images::SelectImagesPage::new();
        let register_page = register::RegisterPage::new();
        (Self { select_images_page, register_page, selected_page: Page::SelectImages } , Command::none())
    }

    fn title(&self) -> String {
        String::from("Microcount")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::GoSelectImages => {
                self.selected_page = Page::SelectImages;
            }

            Message::IncrementSelectImages => {
                self.select_images_page.update(select_images::SelectImagesMessage::Increment);
            }

            Message::DecrementSelectImage => {
                self.select_images_page.update(select_images::SelectImagesMessage::Decrement);
            }

            Message::GoRegister => {
                self.selected_page = Page::Register;
            }

            Message::IncrementRegister => {
                self.register_page.update(register::RegisterMessage::IncrementTen);
            }

            Message::DecrementRegister => {
                self.register_page.update(register::RegisterMessage::DecrementFive);
            }
        }

        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        match &self.selected_page {
            Page::SelectImages => {
                container(self.select_images_page.view().map(|msg|{
                    match msg {
                        SelectImagesMessage::Increment => Message::IncrementSelectImages,
                        SelectImagesMessage::Decrement => Message::DecrementSelectImage,
                        SelectImagesMessage::GoRegister => Message::GoRegister
                    }
                })).into()
            },

            Page::Register => container(self.register_page.view().map(|msg|{
                    match msg {
                        RegisterMessage::IncrementTen => Message::IncrementRegister,
                        RegisterMessage::DecrementFive => Message::DecrementRegister,
                        RegisterMessage::GoSelectImages => Message::GoSelectImages
                    }
            })).into(),
        }
    }
}