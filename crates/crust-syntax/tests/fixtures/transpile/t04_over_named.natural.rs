#[derive(Default)]
pub struct Moteur;

pub struct Axe { pub motorisation: Moteur, 
position: f32, }


pub struct Chariot { pub gauche: Moteur, pub(crate) droite: Moteur, 
vitesse: u32, }


pub(crate) struct Interne { pub(crate) moteur: Moteur, }


struct Prive { r#type: Moteur, 
x: u8, }

