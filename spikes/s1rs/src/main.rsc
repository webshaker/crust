import Servomoteur from "@/robot/Servomoteur";

fn main() {
    let mut s = Servomoteur::new();
    s.parent.accelerer(1200);
    println!("{} {}", s.parent.vitesse, s.position);
}
