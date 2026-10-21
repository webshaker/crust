//! Retours d'usage (RustUI, Simulateur) : dossier privé, cfg de tête,
//! module_inception, include_str relatif, import de dossier.
import { Banc } from "./banc/";

/// `include_str!` relatif au .rsc, alors que le fichier compilé est dans OUT_DIR.
pub fn texte() -> &'static str {
    include_str!("../data/message.txt").trim()
}

pub fn mesure() -> u32 {
    Banc { n: 2 }.double()
}

pub fn brut() -> u8 {
    let x = 7u8;
    crate::interne::lire(&x)
}
