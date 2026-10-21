//! Exemple complet du cahier des charges (section 25).
import Servomoteur from "@/robot/Servomoteur";
import { lire } from "./Capteur.rs";

mod util;

fn main() {
    let mut servo = Servomoteur::new();
    servo.parent.accelerer(1200);
    servo.aller_a(90.0);
    println!("vitesse={} position={} capteur={}", servo.parent.vitesse, servo.position, lire(&servo));
    println!("chemin court : {}", crate::robot::Moteur::default().vitesse);
    println!("util : {}", util::double(21));
    println!("{}", message());
}

fn message() -> &'static str {
    "version 1"
}
