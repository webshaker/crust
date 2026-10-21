/// Un moteur documenté.
///
/// Sur plusieurs lignes.
#[derive(Debug, PartialEq)]
#[cfg(not(feature = "jamais"))]
#[allow(dead_code)]
pub struct Moteur { #[doc = " Vitesse en tr/min."] #[allow(unused)] pub vitesse: u32, #[doc = " Bloc de doc. "] pub couple: f32, } #[cfg(not(feature = "jamais"))] #[allow(dead_code)] impl Moteur {
                          
                    
                     
                       
                    

    #[inline]
    #[must_use]
    pub fn vitesse(&self) -> u32 {
        self.vitesse
    }

  } #[cfg(not(feature = "jamais"))] #[allow(dead_code)] #[allow(clippy::derivable_impls)]
    impl Default for Moteur {
        fn default() -> Self { Self { vitesse: 0, couple: 0.0 } }
    } #[cfg(not(feature = "jamais"))] #[allow(dead_code)] impl Moteur {
}
