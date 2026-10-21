mod io {
    #[derive(Default)]
    pub struct Uart2;
}

#[derive(Default)]
pub struct Moteur;
#[derive(Default)]
pub struct HTTPServer;

impl Servomoteur {                 
                  
} pub struct Servomoteur { pub moteur: Moteur, position: f32, }

impl Robot {                                               
                

    pub fn new() -> Self {
        Self { moteur: Moteur, uart2: io::Uart2, http_server: HTTPServer, nom: String::new() }
    }
} pub struct Robot { pub moteur: Moteur, pub uart2: crate::io::Uart2, pub http_server: HTTPServer, nom: String, }
