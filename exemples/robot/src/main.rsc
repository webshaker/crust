import Servomoteur from "@/robot/Servomoteur";

fn main() {
    let mut servo = Servomoteur::new();
    servo.parent.accelerer(1200);
    servo.aller_a(90.0);
    println!("Bonjour depuis Crust : vitesse {} tr/min, position {}°", servo.parent.vitesse, servo.position);
}
