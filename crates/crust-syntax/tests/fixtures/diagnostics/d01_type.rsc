pub class Moteur {
    vitesse: u32,

    pub fn test(&mut self) {
        self.vitesse = "abc"; //~ ERROR E0308 "abc"
    }

    impl Default {
        fn default() -> Self { Self { vitesse: 0u64 } } //~ ERROR E0308 0u64
    }
}
