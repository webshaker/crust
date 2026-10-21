pub struct Moteur { 
pub vitesse: u32, }

impl Moteur {

    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }
}

