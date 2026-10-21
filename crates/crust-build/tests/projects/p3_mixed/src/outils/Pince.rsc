import { mesure } from "../capteur.rs";
import { bonus } from "./util.rs";

pub class Pince {
    serrage: u32,

    pub fn new() -> Self { Self { serrage: mesure() } }

    pub fn force(&self) -> u32 { self.serrage + bonus() }
}
