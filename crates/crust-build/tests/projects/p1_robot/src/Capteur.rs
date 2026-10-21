// Rust standard : utilise un type défini dans un .rsc, par le chemin court.
use crate::robot::Servomoteur;

pub fn lire(servo: &Servomoteur) -> f32 {
    servo.position / 2.0
}
