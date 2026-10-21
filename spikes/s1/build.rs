// Simule crust-build : copie les fichiers "transpilés" à la main vers OUT_DIR
// et génère les fichiers racine avec des #[path] absolus.
use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("crust");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let fx = manifest.join("gen_fixtures");
    fs::create_dir_all(out.join("src/robot")).unwrap();
    for f in ["src/main.rsc", "src/robot/Moteur.rsc", "src/robot/Servomoteur.rsc"] {
        fs::copy(fx.join(f), out.join(f)).unwrap();
        println!("cargo::rerun-if-changed=gen_fixtures/{f}");
    }
    let o = out.display();
    fs::write(
        out.join("src/robot/__dir.rs"),
        format!(
            "#[path = \"{o}/src/robot/Moteur.rsc\"] pub mod moteur;\n\
             #[path = \"{o}/src/robot/Servomoteur.rsc\"] pub mod servomoteur;\n\
             pub use self::moteur::*;\npub use self::servomoteur::*;\n"
        ),
    )
    .unwrap();
    fs::write(
        out.join("root_main.rs"),
        format!(
            "#[path = \"{o}/src/robot/__dir.rs\"] pub mod robot;\n\
             include!(\"{o}/src/main.rsc\");\n"
        ),
    )
    .unwrap();
}
