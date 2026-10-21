import Moteur from "./Moteur.rsc";

pub class Servomoteur over Moteur(parent) {
    pub position: f32,

    pub fn new() -> Self {
        Self { parent: Moteur::default(), position: 0.0 }
    }
}
