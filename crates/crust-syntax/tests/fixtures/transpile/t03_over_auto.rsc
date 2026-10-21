mod io {
    #[derive(Default)]
    pub struct Uart2;
}

#[derive(Default)]
pub struct Moteur;
#[derive(Default)]
pub struct HTTPServer;

pub class Servomoteur over Moteur {
    position: f32,
}

pub class Robot over Moteur, crate::io::Uart2, HTTPServer {
    nom: String,

    pub fn new() -> Self {
        Self { moteur: Moteur, uart2: io::Uart2, http_server: HTTPServer, nom: String::new() }
    }
}
