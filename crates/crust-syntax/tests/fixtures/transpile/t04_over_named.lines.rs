#[derive(Default)]
pub struct Moteur;

pub struct Axe { pub motorisation: Moteur, position: f32, } impl Axe {
                  
}

pub struct Chariot { pub gauche: Moteur, pub(crate) droite: Moteur, vitesse: u32, } impl Chariot {
                 
}

pub(crate) struct Interne { pub(crate) moteur: Moteur, } impl Interne {
}

struct Prive { r#type: Moteur, x: u8, } impl Prive {
          
}
