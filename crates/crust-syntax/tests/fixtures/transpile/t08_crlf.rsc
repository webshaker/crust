pub class Crlf {
    pub x: u8,

    impl Clone {
        fn clone(&self) -> Self { Self { x: self.x } }
    }

    pub fn get(&self) -> u8 { self.x }
}
