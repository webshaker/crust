use std::fmt;

                       
impl Moteur {     
                     

    pub fn new() -> Self { Self { vitesse: 0 } }

  } impl Default for Moteur {
        fn default() -> Self {
            Self::new()
        }
    } impl Moteur {

    pub fn arreter(&mut self) {
        self.vitesse = 0;
    }

  } impl fmt::Display for Moteur {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} tr/min", self.vitesse)
        }
    } impl Moteur {
} #[derive(Debug, Clone)] pub struct Moteur { pub vitesse: u32, }
