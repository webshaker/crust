//! Mesures de performance (section 23.1). Ignoré par défaut :
//! `cargo test --release -p crust-build --test perf -- --ignored --nocapture`

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn time(mut f: impl FnMut()) -> Duration {
    let t = Instant::now();
    f();
    t.elapsed()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn cargo(dir: &Path, args: &[&str]) {
    let out = Command::new("cargo").args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
#[ignore]
fn projet_de_500_fichiers() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("perf/projet500");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let crust_build = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"projet500\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[build-dependencies]\ncrust-build = {{ path = {:?} }}\n\n[workspace]\n", crust_build.display().to_string()),
    )
    .unwrap();
    std::fs::write(dir.join("build.rs"), "fn main() {\n    crust_build::run();\n}\n").unwrap();
    std::fs::write(
        dir.join("src/main.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/crust/root_main.rs\"));\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.rsc"),
        "fn main() {\n    println!(\"{}\", crate::m0::Classe0::new().get());\n}\n",
    )
    .unwrap();
    for d in 0..25 {
        std::fs::create_dir_all(dir.join(format!("src/m{d}"))).unwrap();
        for f in 0..20 {
            let i = d * 20 + f;
            std::fs::write(
                dir.join(format!("src/m{d}/Classe{i}.rsc")),
                format!(
                    "/// Classe {i}\n#[derive(Debug, Clone, Default)]\npub class Classe{i} {{\n    pub a: u32,\n    b: Vec<u8>,\n\n    pub fn new() -> Self {{\n        Self::default()\n    }}\n\n    pub fn get(&self) -> u32 {{\n        self.a + {i} + self.b.len() as u32\n    }}\n\n    impl PartialEq {{\n        fn eq(&self, o: &Self) -> bool {{\n            self.a == o.a\n        }}\n    }}\n}}\n"
                ),
            )
            .unwrap();
        }
    }

    let cold = time(|| cargo(&dir, &["build", "-q"]));
    let noop = median((0..5).map(|_| time(|| cargo(&dir, &["build", "-q"]))).collect());

    // Build script seul, sur un projet inchangé.
    let out_dir = std::fs::read_dir(dir.join("target/debug/build"))
        .unwrap()
        .flatten()
        .map(|e| e.path().join("out"))
        .find(|p| p.join("crust").is_dir())
        .unwrap();
    let script = std::fs::read_dir(dir.join("target/debug/build"))
        .unwrap()
        .flatten()
        .map(|e| e.path().join("build-script-build"))
        .find(|p| p.is_file())
        .unwrap();
    let run_script = || {
        let out = Command::new(&script)
            .env("CARGO_MANIFEST_DIR", &dir)
            .env("OUT_DIR", &out_dir)
            .env("CARGO_PKG_NAME", "projet500")
            .output()
            .unwrap();
        assert!(out.status.success());
    };
    let script_noop = median((0..5).map(|_| time(run_script)).collect());

    // Modification d'un fichier : build script puis rustc.
    let file = dir.join("src/m3/Classe60.rsc");
    let original = std::fs::read_to_string(&file).unwrap();
    let mut edits = Vec::new();
    let mut scripts = Vec::new();
    for k in 0..5 {
        std::fs::write(&file, original.replace("+ 60 +", &format!("+ {} +", 60 + k + 1))).unwrap();
        scripts.push(time(run_script));
        std::fs::write(&file, original.replace("+ 60 +", &format!("+ {} +", 100 + k))).unwrap();
        edits.push(time(|| cargo(&dir, &["build", "-q"])));
    }
    // Référence : modifier main.rs (pas de .rsc touché).
    let main = dir.join("src/main.rs");
    let main_text = std::fs::read_to_string(&main).unwrap();
    let baseline = median(
        (0..5)
            .map(|k| {
                std::fs::write(&main, format!("{main_text}// {k}\n")).unwrap();
                time(|| cargo(&dir, &["build", "-q"]))
            })
            .collect(),
    );

    // `cargo crust check` contre `cargo check`, projet inchangé.
    cargo(&dir, &["check", "-q"]);
    let check = median((0..5).map(|_| time(|| cargo(&dir, &["check", "-q"]))).collect());
    let crust_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/cargo-crust");
    let crust_check = if crust_bin.is_file() {
        Some(median(
            (0..5)
                .map(|_| {
                    time(|| {
                        let s = Command::new(&crust_bin)
                            .args(["crust", "check", "-q"])
                            .current_dir(&dir)
                            .status()
                            .unwrap();
                        assert!(s.success());
                    })
                })
                .collect(),
        ))
    } else {
        None
    };

    println!("\n=== Projet de 500 fichiers .rsc (25 répertoires) ===");
    println!("premier build (crust-build compris) : {cold:?}");
    println!("build sans changement               : {noop:?}");
    println!("build script seul, sans changement  : {script_noop:?}");
    println!("build script seul, 1 fichier modifié: {:?}", median(scripts));
    println!("build après modification d'un .rsc  : {:?}", median(edits));
    println!("build après modification de main.rs : {baseline:?} (référence)");
    println!("cargo check sans changement         : {check:?}");
    match crust_check {
        Some(c) => println!("cargo crust check sans changement   : {c:?}"),
        None => println!("cargo crust check                   : (compiler cargo-crust en release pour mesurer)"),
    }
}
