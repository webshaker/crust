//! Doc du module.
#![allow(dead_code)]

// Commentaire avant la classe.
pub class Vide {
}

pub class UneLigne { x: u8, fn f(&self) -> u8 { self.x } }

pub class Divers {
    #![allow(unused)]
    // commentaire entre les membres
    a: u8, /* bloc */ b: u16,
    r#type: u8,

    const MAX: u8 = 10;

    pub const fn max() -> u8 { Self::MAX }

    async fn attendre(&self) {}

    unsafe fn brut(&self) -> u8 { self.a }

    extern "C" fn ffi() {}

    macro_genere!();
    impl Clone { fn clone(&self) -> Self { Self { a: self.a, b: self.b, r#type: self.r#type } } }
}

macro_rules! macro_genere {
    () => {
        pub fn genere() -> u8 { 1 }
    };
}
use macro_genere;

mod interne {
    pub class Imbriquee {
        pub v: Vec<Vec<u8>>,

        pub fn taille(&self) -> usize { self.v.len() }
    }
}

fn utilise(c: &Divers) -> u8 {
    c.a + interne::Imbriquee { v: vec![] }.taille() as u8
}
