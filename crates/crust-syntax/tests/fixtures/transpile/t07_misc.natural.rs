//! Doc du module.
#![allow(dead_code)]

// Commentaire avant la classe.
pub struct Vide { }


pub struct UneLigne { 
x: u8, }

impl UneLigne { fn f(&self) -> u8 { self.x }
}


pub struct Divers { 
a: u8, 
b: u16, 
r#type: u8, }

impl Divers {
    #![allow(unused)]

    const MAX: u8 = 10;

    pub const fn max() -> u8 { Self::MAX }

    async fn attendre(&self) {}

    unsafe fn brut(&self) -> u8 { self.a }

    extern "C" fn ffi() {}

    macro_genere!();
}

impl Clone for Divers { fn clone(&self) -> Self { Self { a: self.a, b: self.b, r#type: self.r#type } } }


macro_rules! macro_genere {
    () => {
        pub fn genere() -> u8 { 1 }
    };
}
use macro_genere;

mod interne {
    pub struct Imbriquee { 
pub v: Vec<Vec<u8>>, }

impl Imbriquee {

        pub fn taille(&self) -> usize { self.v.len() }
}

}

fn utilise(c: &Divers) -> u8 {
    c.a + interne::Imbriquee { v: vec![] }.taille() as u8
}
