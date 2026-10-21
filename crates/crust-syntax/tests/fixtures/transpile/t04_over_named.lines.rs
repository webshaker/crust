#[derive(Default)]
pub struct Moteur;

impl Axe {                               
                  
} pub struct Axe { pub motorisation: Moteur, position: f32, }

impl Chariot {                                                    
                 
} pub struct Chariot { pub gauche: Moteur, pub(crate) droite: Moteur, vitesse: u32, }

impl Interne {                        
} pub(crate) struct Interne { pub(crate) moteur: Moteur, }

impl Prive {                     
          
} struct Prive { r#type: Moteur, x: u8, }
