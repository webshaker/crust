import Moteur from "@/Moteur";

pub class Bras over Moteur(base) {
    // Reçoit le Moteur créé par Pince.rsc : même type.
    pub fn avec(base: Moteur) -> Self { Self { base } }
}
