use std::fmt;

#[derive(Debug, Clone)]
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self { Self { vitesse: 0 } }

    impl Default {
        fn default() -> Self {
            Self::new()
        }
    }

    pub fn arreter(&mut self) {
        self.vitesse = 0;
    }

    impl fmt::Display {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} tr/min", self.vitesse)
        }
    }
}
