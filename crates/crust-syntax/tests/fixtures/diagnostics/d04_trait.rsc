pub class Pt {
    x: i32,

    impl Clone { //~ ERROR@map E0046 impl
    }

    impl std::fmt::Display {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.y) //~ ERROR E0609 y
        }
    }
}
