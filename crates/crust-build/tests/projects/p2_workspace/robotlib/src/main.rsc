import Moteur from "./Moteur.rsc";

fn main() {
    let (bras, pince) = robot_lib::assembler();
    let m: Moteur = bras.base;
    println!("bin : meme moteur = {}", m == pince.moteur);
}
