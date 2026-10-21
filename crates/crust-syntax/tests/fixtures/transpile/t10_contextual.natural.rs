// `class`, `over`, `import` et `from` restent des identifiants ordinaires.
struct S { class: u8, over: u8, import: bool, from: u16 }

fn class(over: u8) -> u8 { over }

fn import() {
    let from = String::from("x");
    let class = S { class: 1, over: 2, import: true, from: 3 };
    let _ = (from, class.class, class::<>);
}

macro_rules! py_class {
    (class $n:ident { $($t:tt)* }) => { pub struct $n; };
}
py_class!(class Mine { def x });
py_class! { class Autre { import y from z } }

mod class {}
use self::class as _class;
