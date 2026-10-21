//@ no-compile
#[derive(Default)]
pub struct Moteur;

pub class Collision over Moteur {
    moteur: u8,
}

pub class Doublon over Moteur, Moteur {
}

pub class NonChemin over &'static Moteur {
}

pub class Interdits {
    x: u8,
    struct Dedans;
    type T = u8;
    use std::fmt;
    impl Clone for Moteur {}
    fn ok(&self) {}
}

pub class SansVirgule {
    a: u8
    b: u8,
}

pub class EnTete<T {
}

fn suite_valide() {}
