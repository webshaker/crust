//! Résolution des chemins d'`import` (section 13).

use std::path::{Component, Path, PathBuf};

use crate::names;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// Chemin de module Rust, par exemple `crate::robot::moteur`.
    pub module_path: String,
    /// Noms définis par le fichier cible, si on peut les connaître (fichiers `.rsc`).
    pub exports: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    NotFound {
        tried: Vec<String>,
    },
    /// Le chemin ne commence ni par `./`, ni par `../`, ni par `@/`.
    BadPrefix,
    Ambiguous {
        rsc: String,
        rs: String,
    },
    OutsideSrc,
    InvalidName {
        component: String,
    },
    /// Le fichier `.rs` est monté par plusieurs déclarations `mod`.
    MountedTwice {
        modules: Vec<String>,
    },
    /// Aucun résolveur disponible (transpilation d'un fichier isolé).
    Unsupported,
}

pub trait Resolver {
    fn resolve(&self, path: &str) -> Result<Resolved, ResolveError>;
}

/// Résolveur qui refuse tout import.
pub struct NoResolver;

impl Resolver for NoResolver {
    fn resolve(&self, _path: &str) -> Result<Resolved, ResolveError> {
        Err(ResolveError::Unsupported)
    }
}

/// Résolution sur le système de fichiers : le chemin de module est déduit de
/// l'emplacement du fichier sous `src/` (section 12.1).
///
/// Utilisé par `cargo crust expand` sur un fichier isolé : un `.rs` y est supposé
/// monté à l'emplacement déduit de son chemin. `crust-build` utilise l'arbre de
/// modules réel (section 12.2).
pub struct FsResolver {
    pub src_root: PathBuf,
    pub current_file: PathBuf,
}

impl FsResolver {
    /// Cherche le répertoire `src` parmi les ancêtres de `file`.
    pub fn for_file(file: &Path) -> Option<FsResolver> {
        let file = std::path::absolute(file).ok()?;
        let src_root = file
            .ancestors()
            .skip(1)
            .find(|a| a.file_name().is_some_and(|n| n == "src"))?
            .to_path_buf();
        Some(FsResolver {
            src_root,
            current_file: file,
        })
    }
}

/// Normalisation lexicale (`a/./b/../c` → `a/c`), sans toucher au disque.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Fichier visé par un chemin d'import : (chemin canonique, chemin relatif à `src_root`).
///
/// `./` et `../` sont relatifs au fichier courant, `@/` à `src_root` ; sans
/// extension, `.rsc` puis `.rs` (les deux à la fois : ambigu).
pub fn resolve_file(src_root: &Path, current_file: &Path, path: &str) -> Result<(PathBuf, PathBuf), ResolveError> {
    let base = if let Some(rest) = path.strip_prefix("@/") {
        src_root.join(rest)
    } else if path.starts_with("./") || path.starts_with("../") {
        current_file.parent().unwrap_or(Path::new("/")).join(path)
    } else {
        return Err(ResolveError::BadPrefix);
    };
    let base = normalize(&base);
    if !base.starts_with(normalize(src_root)) {
        return Err(ResolveError::OutsideSrc);
    }
    let candidates: Vec<PathBuf> = match base.extension().and_then(|e| e.to_str()) {
        Some("rsc" | "rs") => vec![base.clone()],
        _ => vec![base.with_extension("rsc"), base.with_extension("rs")],
    };
    let found: Vec<&PathBuf> = candidates.iter().filter(|c| c.is_file()).collect();
    let target = match found.as_slice() {
        [] => {
            return Err(ResolveError::NotFound {
                tried: candidates.iter().map(|c| c.display().to_string()).collect(),
            });
        }
        [one] => (*one).clone(),
        [rsc, rs, ..] => {
            return Err(ResolveError::Ambiguous {
                rsc: rsc.display().to_string(),
                rs: rs.display().to_string(),
            });
        }
    };
    let root = src_root.canonicalize().map_err(|_| ResolveError::OutsideSrc)?;
    let canonical = target.canonicalize().map_err(|_| ResolveError::OutsideSrc)?;
    let rel = canonical
        .strip_prefix(&root)
        .map_err(|_| ResolveError::OutsideSrc)?
        .to_path_buf();
    Ok((canonical, rel))
}

/// Noms définis par un fichier `.rsc` (pour CR0007) ; `None` pour un `.rs`, dont
/// les items peuvent venir de macros.
pub fn exports_of(file: &Path) -> Option<Vec<String>> {
    if file.extension().is_some_and(|e| e == "rsc") {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|src| crate::top_level_names(&src))
    } else {
        None
    }
}

impl Resolver for FsResolver {
    fn resolve(&self, path: &str) -> Result<Resolved, ResolveError> {
        let (target, rel) = resolve_file(&self.src_root, &self.current_file, path)?;
        Ok(Resolved {
            module_path: module_path(&rel)?,
            exports: exports_of(&target),
        })
    }
}

/// `robot/RobotArm.rsc` → `crate::robot::robot_arm` ; `main.rsc` → `crate`.
pub fn module_path(rel: &Path) -> Result<String, ResolveError> {
    let mut parts = vec!["crate".to_string()];
    let comps: Vec<&str> = rel.iter().filter_map(|c| c.to_str()).collect();
    for (i, comp) in comps.iter().enumerate() {
        let last = i + 1 == comps.len();
        let name = if last {
            let stem = Path::new(comp).file_stem().and_then(|s| s.to_str()).unwrap_or(comp);
            if comps.len() == 1 && matches!(stem, "main" | "lib") {
                break;
            }
            names::to_snake_case(stem)
        } else {
            comp.to_string()
        };
        if !names::is_valid_ident(&name) {
            return Err(ResolveError::InvalidName {
                component: comp.to_string(),
            });
        }
        parts.push(name);
    }
    Ok(parts.join("::"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_de_module() {
        assert_eq!(
            module_path(Path::new("robot/RobotArm.rsc")).unwrap(),
            "crate::robot::robot_arm"
        );
        assert_eq!(module_path(Path::new("Moteur.rsc")).unwrap(), "crate::moteur");
        assert_eq!(module_path(Path::new("main.rsc")).unwrap(), "crate");
        assert_eq!(module_path(Path::new("capteur.rs")).unwrap(), "crate::capteur");
        assert!(matches!(
            module_path(Path::new("mon-dossier/A.rsc")),
            Err(ResolveError::InvalidName { .. })
        ));
    }
}
