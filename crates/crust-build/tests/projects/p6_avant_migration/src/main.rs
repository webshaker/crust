mod outils;
mod robot;

fn main() {
    let mut s = robot::servo::Servo::new(3);
    s.tourner(2);
    let c = robot::capteur::Capteur::new(5u8);
    println!("{} {} {} {}", s, outils::double(s.angle()), c.lire(), c.brut());
}
