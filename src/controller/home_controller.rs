use crate::model::Model;

pub struct HomeController {
    pub name: String,
    pub age: u32,
}

impl HomeController {
    pub fn new() -> HomeController {
        Self {
            name: "Arthur".to_owned(),
            age: 42,
        }
    }

    pub fn increment_age(&mut self, model: &mut Model) {
        self.age += 1;
    }

    pub fn load_workspace(&mut self, model: &mut Model) -> Result<(), std::io::Error> {
        model.load_workspace()
    }

    pub fn create_workspace(&mut self, model: &mut Model) -> Result<(), std::io::Error> {
        model.create_workspace()
    }
}
