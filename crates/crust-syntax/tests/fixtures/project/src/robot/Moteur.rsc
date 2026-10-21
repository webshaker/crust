pub class Moteur { pub vitesse: u32, }
pub enum Etat { Arret, Marche }
pub fn creer() -> Moteur { Moteur { vitesse: 0 } }
