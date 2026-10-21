pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }
}
