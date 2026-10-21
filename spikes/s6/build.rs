// Simule crust-build : parcourt src/, "transpile" (copie) si le contenu change.
use std::{env, fs, path::{Path, PathBuf}};
fn walk(dir: &Path, out: &Path, root_mods: &mut String) {
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut mods = String::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out, root_mods);
        } else if p.extension().is_some_and(|x| x == "rsc") {
            println!("cargo::rerun-if-changed={}", p.display());
            let rel = p.strip_prefix(env::var("CARGO_MANIFEST_DIR").unwrap()).unwrap();
            let dst = out.join(rel);
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            let src = fs::read(&p).unwrap();
            if fs::read(&dst).ok().as_deref() != Some(&src[..]) { fs::write(&dst, &src).unwrap(); }
            let name = p.file_stem().unwrap().to_str().unwrap().to_lowercase();
            mods += &format!("#[path = {:?}] pub mod {name};\n", dst.display().to_string());
        }
    }
    if !mods.is_empty() {
        let name = dir.file_name().unwrap().to_str().unwrap();
        *root_mods += &format!("pub mod {name} {{\n{mods}}}\n");
    }
}
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("crust");
    let mut root = String::new();
    walk(&Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("src"), &out, &mut root);
    let dst = out.join("root_main.rs");
    if fs::read_to_string(&dst).ok() != Some(root.clone()) { fs::write(dst, root).unwrap(); }
}
