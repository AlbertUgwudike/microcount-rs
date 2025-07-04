use iced::widget::{column, container};
use iced::{executor, Application, Command, Element, Settings, Theme};

pub mod component;
use component::{navigator, register, select_images, home};

pub mod data;
use data::model;

pub mod message;
use message::{Message, NavigationMessage};

use Message::{Navigate, Register, SelectImages, Home};
use NavigationMessage::{GoToRegister, GoToSelectImages, GoToHome};

use crate::component::HomePage;
use crate::message::SelectImagesMessage;

pub fn main() -> iced::Result {
    Microcount::run(Settings::default())
}

#[derive(Debug)]
pub enum Page {
    Home,
    SelectImages,
    Register,
}

#[derive(Debug)]
struct Microcount {
    model: model::Model,
    navigator_bar: navigator::NavigatorBar,
    home_page: home::HomePage,
    select_images_page: select_images::SelectImagesPage,
    register_page: register::RegisterPage,
    selected_page: Page,
}

impl Application for Microcount {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let home_page = home::HomePage::new();
        let select_images_page = select_images::SelectImagesPage::new();
        let register_page = register::RegisterPage::new();
        let navigator_bar = navigator::NavigatorBar::new();
        let model = model::Model::new();
        (
            Self {
                model,
                home_page,
                select_images_page,
                register_page,
                navigator_bar,
                selected_page: Page::Home,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Microcount")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Navigate(GoToSelectImages) => {
                self.selected_page = Page::SelectImages;
            }

            Navigate(GoToRegister) => {
                self.selected_page = Page::Register;
            }

            Navigate(GoToHome) => {
                self.selected_page = Page::Home;
            }

            Home(message::HomeMessage::LoadWorkspace) => {
                self.model.load_workspace();
            }

            Home(message::HomeMessage::CreateWorkspace) => {
                self.model.create_workspace();
            }
            SelectImages(msg) => {
                self.select_images_page.update(msg);
            }

            Register(msg) => {
                self.register_page.update(msg);
            }
        }

        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let navigator_bar = self.navigator_bar.view();
        let page = match &self.selected_page {
            Page::Home => container(self.home_page.view()),
            Page::SelectImages => container(self.select_images_page.view()),
            Page::Register => container(self.register_page.view()),
        };

        container(column![navigator_bar, page]).into()
    }
}
