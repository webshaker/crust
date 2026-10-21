#[derive(Default)]
pub struct Moteur;

pub class Servo over Moteur {
    moteur: u8, //~ ERROR CR0002 moteur
    angle: f32,
}

pub class Double over Moteur, Moteur { //~ ERROR CR0003 Moteur {
}

pub class Mauvais {
    type T = u8; //~ ERROR CR0005 type
    fn ok(&self) -> u32 { "pas un u32" } //~ ERROR E0308 "pas un u32"
}
