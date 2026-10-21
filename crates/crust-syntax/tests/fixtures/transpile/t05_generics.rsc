pub class Pile<'a, T: Clone, const N: usize = 16>
where
    T: Default,
{
    items: [T; N],
    nom: &'a str,

    pub fn len(&self) -> usize {
        N
    }

    impl<U> From<(U, &'a str)> where U: Into<T> {
        fn from((u, nom): (U, &'a str)) -> Self {
            let v: T = u.into();
            Self { items: std::array::from_fn(|_| v.clone()), nom }
        }
    }

    unsafe impl Send where T: Send {}

    impl<'b> PartialEq<Pile<'b, T, N>> where T: PartialEq {
        fn eq(&self, other: &Pile<'b, T, N>) -> bool {
            self.items == other.items
        }
    }
}

pub class Fonction<F: Fn(u32) -> u32> {
    f: F,

    pub fn appeler(&self, x: u32) -> u32 { (self.f)(x) }
}
