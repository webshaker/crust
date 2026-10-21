mod robot;
mod a;
#[path = "a.rs"]
mod b;
include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));
fn main() {}
