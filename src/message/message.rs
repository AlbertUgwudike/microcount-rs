#[derive(Debug, Clone)]
pub enum Message {
    Navigate(NavigationMessage),
    SelectImages(SelectImagesMessage),
    Register(RegisterMessage),
}

#[derive(Debug, Clone)]
pub enum SelectImagesMessage {
    Increment,
    Decrement,
}

#[derive(Debug, Clone)]
pub enum RegisterMessage {
    Increment,
    Decrement,
}

#[derive(Debug, Clone)]
pub enum NavigationMessage {
    GoToSelectImages,
    GoToRegister,
}
