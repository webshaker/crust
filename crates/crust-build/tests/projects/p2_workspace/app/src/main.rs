// Crate Rust ordinaire, sans Crust, qui utilise les types d'une crate Crust.
use robot_lib::{Bras, Moteur, assembler};

fn main() {
    let (bras, _): (Bras, _) = assembler();
    let m: &Moteur = &bras.base;
    println!("app : moteur {}", m.id);
}
