pub class Capteur<T> {
    pub valeur: T,
    pub unite: Inconnu, //~ ERROR@map E0425 Inconnu

    pub fn lire(&self) -> &T {
        &self.valeu //~ ERROR E0609 valeu
    }
}
