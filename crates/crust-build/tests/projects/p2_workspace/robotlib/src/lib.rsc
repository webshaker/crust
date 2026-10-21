//! Bibliothèque écrite en Crust.
pub import Pince from "./a/Pince.rsc";
pub import Bras from "./b/Bras.rsc";

/// Assemble un bras et une pince qui partagent le même type `Moteur`.
pub fn assembler() -> (Bras, Pince) {
    let pince = Pince::new();
    let bras = Bras::avec(pince.moteur.clone());
    (bras, pince)
}
