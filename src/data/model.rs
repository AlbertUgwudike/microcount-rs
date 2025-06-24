use crate::data::Workspace;

#[derive(Debug)]
pub struct Model {
    workspace: Workspace,
}

impl Model {
    pub fn new() -> Model {
        Model {
            workspace: Workspace::new(),
        }
    }
}
