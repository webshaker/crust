/// Un moteur documenté.
///
/// Sur plusieurs lignes.
#[derive(Debug, PartialEq)]
#[cfg(not(feature = "jamais"))]
#[allow(dead_code)]
pub class Moteur {
    /// Vitesse en tr/min.
    #[allow(unused)]
    pub vitesse: u32,
    /** Bloc de doc. */
    pub couple: f32,

    #[inline]
    #[must_use]
    pub fn vitesse(&self) -> u32 {
        self.vitesse
    }

    #[allow(clippy::derivable_impls)]
    impl Default {
        fn default() -> Self { Self { vitesse: 0, couple: 0.0 } }
    }
}
