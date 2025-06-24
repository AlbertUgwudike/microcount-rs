use iced::widget::image::{viewer, Handle, Viewer};
use iced::widget::{button, column, container, Image};
use iced::Alignment::Center;
use iced::Element;
use iced::Length;

use imageproc::map::map_pixels;

use crate::message::{Message, SelectImagesMessage};

use Message::SelectImages;
use SelectImagesMessage::{Decrement, Increment};

#[derive(Debug, Default)]
pub struct SelectImagesPage {
    value: i64,
}

impl SelectImagesPage {
    pub fn new() -> Self {
        SelectImagesPage { value: 0 }
    }

    pub fn update(&mut self, message: SelectImagesMessage) {
        match message {
            SelectImagesMessage::Increment => {
                self.value += 1;
            }
            SelectImagesMessage::Decrement => {
                self.value -= 1;
            }
        }
    }

    pub fn view<'a>(&self) -> Element<'a, Message> {
        // let image = Image::new("/Users/albert/Downloads/Recall.tiff")
        //     .width(Length::Fill)
        //     .height(Length::Fill)

        let handle = Handle::from_path("/Users/albert/Downloads/Recall.tiff");
        let viewer = viewer(handle);

        container(
            column![
                button("Increment").on_press(SelectImages(Increment)),
                viewer
            ]
            .padding(20)
            .align_items(Center),
        )
        .into()
    }
}
