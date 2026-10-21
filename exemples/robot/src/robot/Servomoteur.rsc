import Moteur from "./Moteur.rsc";

/// Un servomoteur : un moteur et une position.
pub class Servomoteur over Moteur(parent) {
    pub position: f32,

    pub fn new() -> Self {
        Self {
            parent: Moteur::new(),
            position: 0.0,
        }
    }

    pub fn aller_a(&mut self, position: f32) {
        self.position = position;
    }
}
