pub struct Capteur<T> {
    valeur: T,
}

// Bornes différentes de la struct : reste hors de la classe.
impl<T: Copy> Capteur<T> {
    pub fn new(valeur: T) -> Self {
        Self { valeur }
    }

    pub fn lire(&self) -> T {
        self.valeur
    }
}

impl<T> Capteur<T> {
    pub fn brut(&self) -> &T {
        &self.valeur
    }
}
