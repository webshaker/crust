#[derive(Debug, Clone)]
pub struct Limite {
    pub max: u32,
}

impl Limite {
    pub fn valide(&self) -> bool {
        self.max > 0
    }
}
