pub struct Crlf { 
pub x: u8, }

impl Crlf {

    pub fn get(&self) -> u8 { self.x }
}

impl Clone for Crlf {
        fn clone(&self) -> Self { Self { x: self.x } }
    }

