#[derive(Debug)]
pub class Moteur {
    pub vitesse: u32,

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }

    #[cfg(feature = "err")]
    pub fn casse(&mut self) {
        self.vitesse = "abc";
    }

    impl Default {
        fn default() -> Self { Self { vitesse: 0 } }
    }
}
