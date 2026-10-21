/// Un moteur à courant continu.
#[derive(Debug, Clone)]
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }

    impl Default {
        fn default() -> Self {
            Self::new()
        }
    }
}
