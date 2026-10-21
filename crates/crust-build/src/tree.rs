//! Arbre de modules d'un paquet mêlant `.rsc` et `.rs` (section 12 du cahier des charges).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crust_syntax::names::{is_valid_ident, to_snake_case};
use crust_syntax::resolve::{exports_of, resolve_file};
use crust_syntax::{Layout, ModDecl, Options, ResolveError, Resolved, Resolver, SourceInfo, transpile_with};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Lib,
    /// `src/main.rs`.
    Bin,
    /// `src/bin/<nom>.rs` : indice dans [`Project::roots`].
    ExtraBin(u32),
}

/// Racine de crate du paquet : un `.rs` (stub ou écrit à la main) qui inclut
/// `root_file`, et sa racine `.rsc` éventuelle.
#[derive(Clone, Debug)]
pub struct Root {
    pub target: Target,
    /// `.rs` racine (chemin absolu).
    pub rs: PathBuf,
    /// Racine `.rsc` correspondante, relative à `src/` (`main.rsc`, `bin/outil.rsc`).
    pub rsc_rel: PathBuf,
    /// Fichier généré à inclure : `root_main.rs`, `root_lib.rs`, `root_bin_<nom>.rs`.
    pub root_file: String,
}

#[derive(Debug)]
pub struct RscFile {
    /// Chemin relatif à `src/`.
    pub rel: PathBuf,
    pub abs: PathBuf,
    pub canonical: PathBuf,
    /// `Some` pour une racine : `main.rsc`, `lib.rsc`, `bin/<nom>.rsc`.
    pub root: Option<Target>,
    /// Nom du module (fichiers non racines).
    pub name: String,
    /// Chemin de module, par exemple `crate::robot::moteur` (fichiers non racines).
    pub module: String,
}

impl RscFile {
    /// Cible dont ce fichier fait partie.
    pub fn target(&self, owner: Target) -> Target {
        self.root.unwrap_or(owner)
    }
}

/// Module Rust standard trouvé en lisant les déclarations `mod`.
#[derive(Debug)]
pub struct RsModule {
    pub path: String,
    /// `None` pour un module inline.
    pub file: Option<PathBuf>,
    pub target: Target,
    /// Répertoires (relatifs à `src/`) dont le contenu Crust est inclus par ce fichier.
    pub hosts: Vec<String>,
}

/// `.rs` importé depuis un `.rsc` sans être monté par un `mod` : Crust le monte.
#[derive(Debug)]
pub struct AutoRs {
    pub name: String,
    pub file: PathBuf,
    pub module: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DirKind {
    /// `src/` lui-même : contenu dans `root_<cible>.rs`.
    Root,
    /// Module généré par Crust (module inline avec `#[path]`).
    Generated,
    /// Module défini par un `.rs` qui contient la ligne d'inclusion.
    Hosted,
    /// Module défini par un `.rs` sans la ligne d'inclusion (CR0010).
    Unhosted(String),
}

#[derive(Debug)]
pub struct Dir {
    pub module: String,
    pub kind: DirKind,
    pub rsc: Vec<usize>,
    pub subdirs: Vec<PathBuf>,
    pub auto_rs: Vec<usize>,
    pub errors: Vec<String>,
}

pub struct Project {
    pub src: PathBuf,
    pub lib_name: String,
    pub has_lib: bool,
    pub has_bin: bool,
    /// Racines de crate existantes : lib, main, puis `src/bin/*.rs`.
    pub roots: Vec<Root>,
    /// Cible qui contient les modules (la lib s'il y en a une).
    pub owner: Target,
    pub rsc: Vec<RscFile>,
    pub rs_modules: Vec<RsModule>,
    pub auto_rs: Vec<AutoRs>,
    pub dirs: BTreeMap<PathBuf, Dir>,
    /// Erreurs sans fichier `.rsc` à désigner : injectées dans le fichier racine.
    pub root_errors: Vec<String>,
    pub warnings: Vec<String>,
}

fn walk(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let rel = rel.join(e.file_name());
        if path.is_dir() {
            // `src/bin/` : racines de binaires, traitées à part.
            if rel != Path::new("bin") {
                walk(&path, &rel, out);
            }
        } else if path.extension().is_some_and(|x| x == "rsc") {
            out.push(rel);
        }
    }
}

fn module_join(parent: &str, name: &str) -> String {
    format!("{parent}::{name}")
}

fn dir_module(rel_dir: &Path) -> Result<String, String> {
    let mut path = "crate".to_string();
    for seg in rel_dir.iter() {
        let seg = seg.to_string_lossy();
        if !is_valid_ident(&seg) {
            return Err(seg.into_owned());
        }
        path = module_join(&path, &seg);
    }
    Ok(path)
}

/// Répertoires `R` dont le contenu est inclus par ce fichier : `"/crust/dir/<R>.rs"`.
fn find_hosts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("/crust/dir/") {
        rest = &rest[i + "/crust/dir/".len()..];
        if let Some(end) = rest.find(".rs\"") {
            out.push(rest[..end].to_string());
        }
    }
    out
}

impl Project {
    pub fn scan(src: &Path, lib_name: &str) -> Result<Project, String> {
        let src = std::path::absolute(src).map_err(|e| e.to_string())?;
        let has_lib = src.join("lib.rs").is_file();
        let has_bin = src.join("main.rs").is_file();
        if !has_lib && !has_bin {
            return Err(format!("ni `src/lib.rs` ni `src/main.rs` dans {}", src.display()));
        }
        let owner = if has_lib { Target::Lib } else { Target::Bin };
        let mut roots = Vec::new();
        let root = |target, rs: &str, rsc: &str, file: &str| Root {
            target,
            rs: src.join(rs),
            rsc_rel: PathBuf::from(rsc),
            root_file: file.to_string(),
        };
        if has_lib {
            roots.push(root(Target::Lib, "lib.rs", "lib.rsc", "root_lib.rs"));
        }
        if has_bin {
            roots.push(root(Target::Bin, "main.rs", "main.rsc", "root_main.rs"));
        }
        let mut bins: Vec<PathBuf> = std::fs::read_dir(src.join("bin"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "rs" || x == "rsc"))
            .collect();
        bins.sort();
        for b in &bins {
            let stem = b.file_stem().unwrap_or_default().to_string_lossy();
            let is_rs = b.extension().is_some_and(|x| x == "rs");
            if is_rs {
                let target = Target::ExtraBin(roots.len() as u32);
                roots.push(root(
                    target,
                    &format!("bin/{stem}.rs"),
                    &format!("bin/{stem}.rsc"),
                    &format!("root_bin_{stem}.rs"),
                ));
            }
        }
        let mut p = Project {
            src: src.clone(),
            lib_name: lib_name.to_string(),
            has_lib,
            has_bin,
            roots,
            owner,
            rsc: Vec::new(),
            rs_modules: Vec::new(),
            auto_rs: Vec::new(),
            dirs: BTreeMap::new(),
            root_errors: Vec::new(),
            warnings: Vec::new(),
        };

        // 1. Fichiers .rsc et leurs chemins de module.
        let mut rels = Vec::new();
        walk(&src, Path::new(""), &mut rels);
        rels.extend(
            bins.iter()
                .filter(|b| b.extension().is_some_and(|x| x == "rsc"))
                .filter_map(|b| b.strip_prefix(&src).ok().map(Path::to_path_buf)),
        );
        for rel in rels {
            let abs = src.join(&rel);
            let canonical = abs.canonicalize().unwrap_or_else(|_| abs.clone());
            let root = p.roots.iter().find(|r| r.rsc_rel == rel).map(|r| r.target);
            // Racine `.rsc` sans son `.rs` : ignorée (avertissement dans `check_roots`).
            if root.is_none() && (rel == Path::new("lib.rsc") || rel == Path::new("main.rsc") || rel.starts_with("bin"))
            {
                continue;
            }
            let stem = rel.file_stem().unwrap_or_default().to_string_lossy();
            let name = to_snake_case(&stem);
            let module = match dir_module(rel.parent().unwrap_or(Path::new(""))) {
                Ok(m) => module_join(&m, &name),
                Err(_) => String::new(), // signalé avec le répertoire (CR0012)
            };
            p.rsc.push(RscFile {
                rel,
                abs,
                canonical,
                root,
                name,
                module,
            });
        }

        p.check_roots();

        // 2. Modules .rs déclarés par les racines et par les `mod x;` des .rsc.
        let mut visited = HashSet::new();
        for root in p.roots.clone() {
            let dir = root.rs.parent().unwrap_or(&src).to_path_buf();
            p.scan_rs(&root.rs, "crate", &dir, &dir, root.target, &mut visited);
            if let Ok(text) = std::fs::read_to_string(src.join(&root.rsc_rel)) {
                p.scan_decls(
                    &crust_syntax::mod_decls(&text),
                    "crate",
                    &dir,
                    &dir,
                    root.target,
                    &mut visited,
                );
            }
        }
        for i in 0..p.rsc.len() {
            if p.rsc[i].root.is_some() || p.rsc[i].module.is_empty() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p.rsc[i].abs) else {
                continue;
            };
            let info = SourceInfo {
                file: p.rsc[i].abs.clone(),
                crate_root: false,
            };
            let file_dir = p.rsc[i].abs.parent().unwrap_or(&src).to_path_buf();
            let module = p.rsc[i].module.clone();
            p.scan_decls(
                &crust_syntax::mod_decls(&text),
                &module,
                &info.children_dir(),
                &file_dir,
                owner,
                &mut visited,
            );
        }

        // 3. `.rs` importés mais montés nulle part : Crust les monte.
        p.collect_auto_rs();

        // 4. Répertoires.
        p.build_dirs();
        Ok(p)
    }

    /// Racine d'une cible.
    pub fn root(&self, target: Target) -> Option<&Root> {
        self.roots.iter().find(|r| r.target == target)
    }

    fn check_roots(&mut self) {
        let mut orphans: Vec<PathBuf> = ["lib.rsc", "main.rsc"].iter().map(PathBuf::from).collect();
        orphans.extend(
            std::fs::read_dir(self.src.join("bin"))
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "rsc"))
                .map(|e| Path::new("bin").join(e.file_name())),
        );
        for rsc in orphans {
            if self.src.join(&rsc).is_file() && !self.roots.iter().any(|r| r.rsc_rel == rsc) {
                let rs = rsc.with_extension("rs");
                let file = if rsc == Path::new("lib.rsc") {
                    "root_lib.rs".to_string()
                } else if rsc == Path::new("main.rsc") {
                    "root_main.rs".to_string()
                } else {
                    format!("root_bin_{}.rs", rsc.file_stem().unwrap_or_default().to_string_lossy())
                };
                self.warnings.push(format!(
                    "`src/{}` est ignoré : créez `src/{}` contenant `include!(concat!(env!(\"OUT_DIR\"), \"/crust/{file}\"));` (ou lancez `cargo crust init`)",
                    rsc.display(),
                    rs.display()
                ));
            }
        }
        // L'inclusion est nécessaire si la racine a son `.rsc`, ou si elle porte les
        // modules d'un projet qui contient des `.rsc` (un binaire en pur Rust n'en a pas besoin).
        let has_modules = self.rsc.iter().any(|r| r.root.is_none());
        for root in &self.roots {
            let needed = self.src.join(&root.rsc_rel).is_file() || (root.target == self.owner && has_modules);
            if needed
                && let Ok(text) = std::fs::read_to_string(&root.rs)
                && !text.contains(&format!("/crust/{}", root.root_file))
            {
                self.warnings.push(format!(
                    "`{}` ne contient pas la ligne `include!(concat!(env!(\"OUT_DIR\"), \"/crust/{}\"));` : les modules Crust ne sont pas montés",
                    root.rs.display(),
                    root.root_file
                ));
            }
        }
    }

    fn scan_rs(
        &mut self,
        file: &Path,
        module: &str,
        children_dir: &Path,
        path_base: &Path,
        target: Target,
        visited: &mut HashSet<(PathBuf, Target)>,
    ) {
        let Ok(text) = std::fs::read_to_string(file) else {
            return;
        };
        let canonical = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
        let hosts = find_hosts(&text);
        if module != "crate" {
            // Enregistré à chaque montage, même répété : CR0011 en dépend.
            self.rs_modules.push(RsModule {
                path: module.to_string(),
                file: Some(canonical.clone()),
                target,
                hosts,
            });
        } else if !hosts.is_empty() {
            self.warnings.push(format!(
                "{} : la racine inclut `/crust/root_*.rs`, pas `/crust/dir/...`",
                file.display()
            ));
        }
        if !visited.insert((canonical, target)) {
            return;
        }
        self.scan_decls(
            &crust_syntax::mod_decls(&text),
            module,
            children_dir,
            path_base,
            target,
            visited,
        );
    }

    /// Règles de rustc : `mod x;` → `<rép>/x.rs` ou `<rép>/x/mod.rs` ;
    /// `#[path = "p"] mod x;` → relatif au répertoire du fichier.
    fn scan_decls(
        &mut self,
        decls: &[ModDecl],
        module: &str,
        children_dir: &Path,
        path_base: &Path,
        target: Target,
        visited: &mut HashSet<(PathBuf, Target)>,
    ) {
        for d in decls {
            let name = d.name.strip_prefix("r#").unwrap_or(&d.name);
            let child = module_join(module, &d.name);
            if let Some(inner) = &d.inline {
                let dir = match &d.path {
                    Some(p) => path_base.join(p),
                    None => children_dir.join(name),
                };
                self.rs_modules.push(RsModule {
                    path: child.clone(),
                    file: None,
                    target,
                    hosts: Vec::new(),
                });
                self.scan_decls(inner, &child, &dir, &dir, target, visited);
                continue;
            }
            let (file, child_dir) = match &d.path {
                Some(p) => {
                    let f = path_base.join(p);
                    let dir = f.parent().unwrap_or(path_base).to_path_buf();
                    (f, dir)
                }
                None => {
                    let flat = children_dir.join(format!("{name}.rs"));
                    let nested = children_dir.join(name).join("mod.rs");
                    (if flat.is_file() { flat } else { nested }, children_dir.join(name))
                }
            };
            let base = file.parent().unwrap_or(path_base).to_path_buf();
            self.scan_rs(&file, &child, &child_dir, &base, target, visited);
        }
    }

    fn rsc_by_canonical(&self, file: &Path) -> Option<&RscFile> {
        self.rsc.iter().find(|r| r.canonical == file)
    }

    /// Passe de collecte : imports des `.rsc` vers des `.rs` non montés.
    fn collect_auto_rs(&mut self) {
        struct Collect<'p> {
            p: &'p Project,
            current: PathBuf,
            found: std::cell::RefCell<BTreeSet<PathBuf>>,
        }
        impl Resolver for Collect<'_> {
            fn resolve(&self, path: &str) -> Result<Resolved, ResolveError> {
                if let Ok((file, _)) = resolve_file(&self.p.src, &self.current, path)
                    && file.extension().is_some_and(|e| e == "rs")
                    && !self.p.rs_modules.iter().any(|m| m.file.as_deref() == Some(&file))
                {
                    self.found.borrow_mut().insert(file);
                }
                Err(ResolveError::Unsupported)
            }
        }
        let mut found = BTreeSet::new();
        for r in &self.rsc {
            let Ok(text) = std::fs::read_to_string(&r.abs) else {
                continue;
            };
            if !text.contains("import") {
                continue;
            }
            let c = Collect {
                p: self,
                current: r.abs.clone(),
                found: Default::default(),
            };
            let options = Options {
                layout: Layout::Lines,
                source: None,
            };
            let _ = transpile_with(&text, &options, &c);
            found.extend(c.found.into_inner());
        }
        let src_canonical = self.src.canonicalize().unwrap_or_else(|_| self.src.clone());
        for file in found {
            let Ok(rel) = file.strip_prefix(&src_canonical) else {
                continue;
            };
            let stem = rel.file_stem().unwrap_or_default().to_string_lossy();
            if stem == "mod" || stem == "main" || stem == "lib" {
                continue; // résolu en erreur par le résolveur
            }
            let name = to_snake_case(&stem);
            let Ok(dir) = dir_module(rel.parent().unwrap_or(Path::new(""))) else {
                continue;
            };
            let module = module_join(&dir, &name);
            self.auto_rs.push(AutoRs { name, file, module });
        }
    }

    fn build_dirs(&mut self) {
        let src_canonical = self.src.canonicalize().unwrap_or_else(|_| self.src.clone());
        let mut needed: BTreeSet<PathBuf> = BTreeSet::new();
        needed.insert(PathBuf::new());
        let file_dirs = self
            .rsc
            .iter()
            .filter(|r| r.root.is_none())
            .map(|r| r.rel.parent().unwrap_or(Path::new("")).to_path_buf())
            .chain(
                self.auto_rs
                    .iter()
                    .filter_map(|a| a.file.strip_prefix(&src_canonical).ok())
                    .map(|rel| rel.parent().unwrap_or(Path::new("")).to_path_buf()),
            )
            .collect::<Vec<_>>();
        for d in file_dirs {
            for a in d.ancestors() {
                needed.insert(a.to_path_buf());
            }
        }

        for rel in &needed {
            let (module, mut errors) = match dir_module(rel) {
                Ok(m) => (m, Vec::new()),
                Err(seg) => (
                    String::new(),
                    vec![format!(
                        "[CR0012] le répertoire `src/{}` ne donne pas un nom de module Rust valide (`{seg}`) : ses fichiers .rsc ne sont pas montés",
                        rel.display()
                    )],
                ),
            };
            let kind = if rel.as_os_str().is_empty() {
                DirKind::Root
            } else {
                let key = rel.to_string_lossy().replace('\\', "/");
                match self
                    .rs_modules
                    .iter()
                    .find(|m| m.path == module && m.target == self.owner)
                {
                    None => DirKind::Generated,
                    Some(m) if m.hosts.contains(&key) => DirKind::Hosted,
                    Some(m) => DirKind::Unhosted(
                        m.file
                            .as_ref()
                            .map_or("un module inline".to_string(), |f| format!("`{}`", f.display())),
                    ),
                }
            };
            if let DirKind::Unhosted(by) = &kind {
                let key = rel.to_string_lossy().replace('\\', "/");
                errors.push(format!(
                    "[CR0010] `src/{key}` contient des fichiers Crust mais le module `{module}` est défini par {by} : ajoutez-y la ligne `include!(concat!(env!(\"OUT_DIR\"), \"/crust/dir/{key}.rs\"));`"
                ));
            }
            self.dirs.insert(
                rel.clone(),
                Dir {
                    module,
                    kind,
                    rsc: Vec::new(),
                    subdirs: Vec::new(),
                    auto_rs: Vec::new(),
                    errors,
                },
            );
        }
        for rel in &needed {
            if let Some(parent) = rel.parent()
                && !rel.as_os_str().is_empty()
            {
                self.dirs.get_mut(parent).unwrap().subdirs.push(rel.clone());
            }
        }
        for (i, r) in self.rsc.iter().enumerate() {
            if r.root.is_none() {
                let d = self.dirs.get_mut(r.rel.parent().unwrap_or(Path::new(""))).unwrap();
                d.rsc.push(i);
            }
        }
        for (i, a) in self.auto_rs.iter().enumerate() {
            if let Ok(rel) = a.file.strip_prefix(&src_canonical) {
                self.dirs
                    .get_mut(rel.parent().unwrap_or(Path::new("")))
                    .unwrap()
                    .auto_rs
                    .push(i);
            }
        }

        // CR0009 : deux fichiers pour un même module.
        let rels: Vec<PathBuf> = self.dirs.keys().cloned().collect();
        for rel in rels {
            // Les sous-répertoires d'abord : en cas de conflit, c'est le fichier qui
            // n'est pas monté (son module est vidé), pour éviter les erreurs en cascade.
            #[derive(Clone, Copy)]
            enum Entry {
                Subdir,
                Rsc(usize),
                Auto(usize),
            }
            let d = &self.dirs[&rel];
            let entries: Vec<(String, String, Entry)> = d
                .subdirs
                .iter()
                .map(|s| {
                    let name = s.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    (name, format!("src/{}/", s.display()), Entry::Subdir)
                })
                .chain(d.rsc.iter().map(|&i| {
                    (
                        self.rsc[i].name.clone(),
                        format!("src/{}", self.rsc[i].rel.display()),
                        Entry::Rsc(i),
                    )
                }))
                .chain(d.auto_rs.iter().map(|&i| {
                    let a = &self.auto_rs[i];
                    (a.name.clone(), a.file.display().to_string(), Entry::Auto(i))
                }))
                .collect();
            let dir_module = d.module.clone();
            let mut seen: BTreeMap<String, String> = BTreeMap::new();
            let mut errors = Vec::new();
            for (name, what, entry) in entries {
                let mut conflict = None;
                if !is_valid_ident(&name) {
                    conflict = Some(format!(
                        "[CR0012] `{what}` ne donne pas un nom de module Rust valide (`{name}`)"
                    ));
                } else if let Some(prev) = seen.get(&name) {
                    conflict = Some(format!(
                        "[CR0009] `{prev}` et `{what}` donnent tous deux le module `{name}`"
                    ));
                } else if !matches!(entry, Entry::Subdir) {
                    let module = module_join(&dir_module, &name);
                    if let Some(m) = self
                        .rs_modules
                        .iter()
                        .find(|m| m.path == module && m.target == self.owner)
                    {
                        let by = m
                            .file
                            .as_ref()
                            .map_or("un module inline".into(), |f| f.display().to_string());
                        conflict = Some(format!(
                            "[CR0009] `{what}` et `{by}` donnent tous deux le module `{module}`"
                        ));
                    }
                }
                match (conflict, entry) {
                    (None, _) => {
                        seen.insert(name, what);
                    }
                    (Some(e), Entry::Rsc(i)) => {
                        errors.push(e);
                        self.rsc[i].module.clear();
                    }
                    (Some(e), Entry::Auto(i)) => {
                        errors.push(e);
                        self.auto_rs[i].module.clear();
                    }
                    (Some(e), Entry::Subdir) => errors.push(e),
                }
            }
            self.dirs.get_mut(&rel).unwrap().errors.extend(errors);
        }
    }

    /// Chemin de module d'un fichier vu depuis la cible `from`.
    pub fn module_of(&self, file: &Path, from: Target) -> Result<String, ResolveError> {
        let fix = |module: &str, target: Target| -> Result<String, ResolveError> {
            match (target, from) {
                (a, b) if a == b => Ok(module.to_string()),
                // Un binaire voit les modules de la lib par le nom de la crate.
                (Target::Lib, _) => Ok(module.replacen("crate", &self.lib_name, 1)),
                // Modules du binaire principal (sans lib) : invisibles des autres cibles.
                _ => Err(ResolveError::Unsupported),
            }
        };
        if let Some(r) = self.rsc_by_canonical(file) {
            if r.module.is_empty() && r.root.is_none() {
                return Err(ResolveError::InvalidName {
                    component: r.rel.display().to_string(),
                });
            }
            let module = if r.root.is_some() { "crate" } else { &r.module };
            return fix(module, r.target(self.owner));
        }
        let mounted: Vec<&RsModule> = self
            .rs_modules
            .iter()
            .filter(|m| m.file.as_deref() == Some(file))
            .collect();
        let same_target: Vec<&&RsModule> = mounted.iter().filter(|m| m.target == from).collect();
        match (same_target.as_slice(), mounted.as_slice()) {
            ([one], _) => return Ok(one.path.clone()),
            ([], [one]) => return fix(&one.path, one.target),
            ([], []) => {}
            (several, _) if several.len() > 1 => {
                return Err(ResolveError::MountedTwice {
                    modules: several.iter().map(|m| m.path.clone()).collect(),
                });
            }
            (_, several) => {
                return Err(ResolveError::MountedTwice {
                    modules: several.iter().map(|m| m.path.clone()).collect(),
                });
            }
        }
        match self.auto_rs.iter().find(|a| a.file == file && !a.module.is_empty()) {
            Some(a) => fix(&a.module, self.owner),
            None => Err(ResolveError::Unsupported),
        }
    }
}

/// Résolveur des imports pendant le build : utilise l'arbre de modules réel.
pub struct BuildResolver<'p> {
    pub project: &'p Project,
    pub current: PathBuf,
    pub target: Target,
}

impl Resolver for BuildResolver<'_> {
    fn resolve(&self, path: &str) -> Result<Resolved, ResolveError> {
        let (file, _) = resolve_file(&self.project.src, &self.current, path)?;
        Ok(Resolved {
            module_path: self.project.module_of(&file, self.target)?,
            exports: exports_of(&file),
        })
    }
}
