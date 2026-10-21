// Projet Rust existant, auquel on ajoute des fichiers Crust.
mod capteur;
mod outils;

include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));

fn main() {
    let p = outils::Pince::new();
    println!("pince {} / moteur {}", p.force(), robot::Moteur { tours: 3 }.tours);
}
