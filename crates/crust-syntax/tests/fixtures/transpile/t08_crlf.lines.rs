impl Crlf {     
              

  } impl Clone for Crlf {
        fn clone(&self) -> Self { Self { x: self.x } }
    } impl Crlf {

    pub fn get(&self) -> u8 { self.x }
} pub struct Crlf { pub x: u8, }
