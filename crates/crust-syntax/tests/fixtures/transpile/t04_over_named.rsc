#[derive(Default)]
pub struct Moteur;

pub class Axe over Moteur(motorisation) {
    position: f32,
}

pub class Chariot over Moteur(gauche), pub(crate) Moteur(droite) {
    vitesse: u32,
}

pub(crate) class Interne over Moteur {
}

class Prive over Moteur(r#type) {
    x: u8,
}
