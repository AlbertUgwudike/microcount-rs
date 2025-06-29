#[derive(Debug, Clone)]
pub enum Message {
    Navigate(NavigationMessage),
    Home(HomeMessage),
    SelectImages(SelectImagesMessage),
    Register(RegisterMessage),
}

#[derive(Debug, Clone)]
pub enum HomeMessage {
    CreateWorkspace,
    LoadWorkspace,
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
    GoToHome,
    GoToSelectImages,
    GoToRegister,
}
