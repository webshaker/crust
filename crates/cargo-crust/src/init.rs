//! `cargo crust init` et `cargo crust new` (section 17.4).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Dépendance à `crust-build` : chemin local tant que la crate n'est pas publiée.
fn crust_build_dep() -> String {
    let local = std::env::var_os("CRUST_BUILD_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../crust-build"));
    match local.canonicalize() {
        Ok(p) => format!("crust-build = {{ path = {:?} }}", p.display().to_string()),
        Err(_) => format!("crust-build = \"{}\"", env!("CARGO_PKG_VERSION")),
    }
}

const ROOT_LINE: &str = "include!(concat!(env!(\"OUT_DIR\"), \"/crust/{root}\"));";

/// Installe Crust dans le paquet `dir` ; renvoie la liste des modifications.
pub fn init_package(dir: &Path) -> Result<Vec<String>, String> {
    let manifest = dir.join("Cargo.toml");
    let toml = std::fs::read_to_string(&manifest).map_err(|e| format!("{} : {e}", manifest.display()))?;
    let mut done = Vec::new();

    if !toml.contains("crust-build") {
        let dep = crust_build_dep();
        let new = match toml.find("[build-dependencies]") {
            Some(i) => {
                let at = i + "[build-dependencies]".len();
                format!("{}\n{dep}{}", &toml[..at], &toml[at..])
            }
            None => format!("{}\n[build-dependencies]\n{dep}\n", toml.trim_end()),
        };
        std::fs::write(&manifest, new).map_err(|e| e.to_string())?;
        done.push(format!("Cargo.toml : ajout de `{dep}`"));
    }
    if !toml.contains("edition") || toml.contains("edition = \"2015\"") {
        done.push("attention : Crust demande l'édition 2018 ou plus (`edition = \"2021\"`)".into());
    }

    let build = dir.join("build.rs");
    match std::fs::read_to_string(&build) {
        Err(_) => {
            std::fs::write(&build, "fn main() {\n    crust_build::run();\n}\n").map_err(|e| e.to_string())?;
            done.push("build.rs : créé".into());
        }
        Ok(text) if !text.contains("crust_build::run") => {
            let body = text
                .find("fn main()")
                .and_then(|i| text[i..].find('{').map(|j| i + j + 1));
            match body {
                Some(at) => {
                    let new = format!("{}\n    crust_build::run();{}", &text[..at], &text[at..]);
                    std::fs::write(&build, new).map_err(|e| e.to_string())?;
                    done.push("build.rs : ajout de `crust_build::run();`".into());
                }
                None => done.push("build.rs : ajoutez `crust_build::run();` dans `main` (non trouvée)".into()),
            }
        }
        Ok(_) => {}
    }

    for (file, root) in [("src/main.rs", "root_main.rs"), ("src/lib.rs", "root_lib.rs")] {
        let path = dir.join(file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains(&format!("/crust/{root}")) {
            let line = ROOT_LINE.replace("{root}", root);
            let sep = if text.ends_with('\n') || text.is_empty() {
                ""
            } else {
                "\n"
            };
            std::fs::write(&path, format!("{text}{sep}\n{line}\n")).map_err(|e| e.to_string())?;
            done.push(format!("{file} : ajout de la ligne d'inclusion des modules Crust"));
        }
    }
    Ok(done)
}

pub fn init(args: &[String]) -> ExitCode {
    let dir = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    match init_package(&dir) {
        Ok(done) if done.is_empty() => println!("Crust est déjà installé dans ce paquet."),
        Ok(done) => {
            for d in done {
                println!("  {d}");
            }
            println!("Crust est installé : les fichiers .rsc de src/ sont maintenant des modules.");
        }
        Err(e) => {
            eprintln!("erreur : {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// `cargo new` puis installation, avec une racine `main.rsc`.
pub fn new(args: &[String]) -> ExitCode {
    // Le chemin est le seul argument qui n'est ni une option ni la valeur d'une option.
    let takes_value = [
        "--vcs",
        "--edition",
        "--name",
        "--registry",
        "--color",
        "--config",
        "-Z",
    ];
    let name = args
        .iter()
        .enumerate()
        .find(|&(i, a)| !a.starts_with('-') && (i == 0 || !takes_value.contains(&args[i - 1].as_str())))
        .map(|(_, a)| a);
    let Some(name) = name else {
        eprintln!("usage : cargo crust new <chemin> [options de cargo new]");
        return ExitCode::from(2);
    };
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let ok = Command::new(cargo)
        .arg("new")
        .args(args)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return ExitCode::FAILURE;
    }
    let dir = Path::new(name);
    let is_lib = args.iter().any(|a| a == "--lib");
    if !is_lib {
        let main = "fn main() {\n    println!(\"Bonjour depuis Crust !\");\n}\n";
        if std::fs::write(dir.join("src/main.rsc"), main).is_err()
            || std::fs::write(dir.join("src/main.rs"), "").is_err()
        {
            eprintln!("erreur : impossible d'écrire src/main.rsc");
            return ExitCode::FAILURE;
        }
    }
    init(std::slice::from_ref(name))
}
