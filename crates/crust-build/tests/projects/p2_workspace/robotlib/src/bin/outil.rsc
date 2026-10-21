import { Moteur, assembler } from "@/lib.rsc";

fn main() {
    let (bras, _) = assembler();
    let m: Moteur = bras.base;
    println!("outil : moteur {}", m.id);
}
