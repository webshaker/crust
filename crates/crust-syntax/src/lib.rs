//! Crust : transpilation des fichiers `.rsc` vers du Rust standard.
//!
//! ```
//! use crust_syntax::{transpile, Layout, NoResolver};
//!
//! let out = transpile("pub class A { pub x: u32, }", Layout::Lines, &NoResolver);
//! assert!(out.diagnostics.is_empty());
//! assert!(out.code.contains("pub struct A { pub x: u32, }"));
//! ```

pub mod diag;
mod emit;
pub mod fmt;
pub mod lexer;
pub mod names;
mod parser;
pub mod resolve;
pub mod sourcemap;
mod toks;

pub use diag::{Code, Diagnostic, Label};
pub use parser::ModDecl;
pub use resolve::{FsResolver, NoResolver, ResolveError, Resolved, Resolver};
pub use sourcemap::{LineCol, LineIndex, Mapped, Segment, SegmentKind, SourceMap};

/// Plage d'octets dans un texte, fin exclue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span {
            start,
            end: end.max(start),
        }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Disposition préservant les lignes : pour la compilation et l'éditeur.
    Lines,
    /// `struct` puis `impl`, lisible : pour `cargo crust expand` et `eject`.
    Natural,
}

/// Emplacement réel du fichier `.rsc` : nécessaire pour réécrire ses `mod x;`,
/// puisque le fichier généré vit ailleurs (dans `OUT_DIR`).
#[derive(Clone, Debug)]
pub struct SourceInfo {
    /// Chemin absolu du `.rsc`.
    pub file: std::path::PathBuf,
    /// Racine de crate (`main.rsc`, `lib.rsc`) : ses sous-modules sont dans son
    /// répertoire, comme pour un `mod.rs`, et le fichier généré est inséré par
    /// `include!` (ses attributs internes doivent aller dans le `.rs` racine).
    pub crate_root: bool,
}

impl SourceInfo {
    /// Répertoire des fichiers des sous-modules (`src/robot/Moteur.rsc` → `src/robot/Moteur/`).
    pub fn children_dir(&self) -> std::path::PathBuf {
        let dir = self.file.parent().unwrap_or(std::path::Path::new("/")).to_path_buf();
        if self.crate_root {
            dir
        } else {
            dir.join(self.file.file_stem().unwrap_or_default())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub layout: Layout,
    /// Si présent, les `mod x;` sont réécrits pour pointer vers `src/` (section 17.3).
    pub source: Option<SourceInfo>,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub code: String,
    /// Vide en disposition [`Layout::Natural`].
    pub map: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
}

/// Traduit un fichier `.rsc` en Rust standard.
///
/// Un fichier sans construction Crust est rendu à l'identique (garantie C1).
pub fn transpile(src: &str, layout: Layout, resolver: &dyn Resolver) -> Output {
    transpile_with(src, &Options { layout, source: None }, resolver)
}

/// [`transpile`] avec toutes les options.
pub fn transpile_with(src: &str, options: &Options, resolver: &dyn Resolver) -> Output {
    transpile_impl(src, options, resolver, options.source.is_none())
}

/// [`transpile`] sans le raccourci sur les fichiers sans mot-clé Crust :
/// utilisé par les tests de compatibilité pour exercer le parseur sur tout le corpus.
#[doc(hidden)]
pub fn transpile_without_shortcut(src: &str, layout: Layout, resolver: &dyn Resolver) -> Output {
    transpile_impl(src, &Options { layout, source: None }, resolver, false)
}

fn transpile_impl(src: &str, options: &Options, resolver: &dyn Resolver, shortcut: bool) -> Output {
    let layout = options.layout;
    let identity = || Output {
        code: src.to_string(),
        map: SourceMap {
            segments: vec![Segment {
                gen_start: 0,
                gen_len: src.len(),
                src_start: 0,
                src_len: src.len(),
                kind: SegmentKind::Copied,
                primary: false,
            }],
        },
        diagnostics: Vec::new(),
    };
    // Raccourci : pas de mot-clé Crust, rien à faire.
    if shortcut && !src.contains("class") && !src.contains("import") {
        return identity();
    }
    let t = toks::Toks::new(src);
    let crate_root = options.source.as_ref().is_some_and(|s| s.crate_root);
    let items = parser::parse(&t, crate_root);
    if items.is_empty() {
        return identity();
    }
    let (code, mut map, diagnostics) = emit::emit(&t, &items, options, resolver);
    if layout == Layout::Natural {
        map = SourceMap::default();
    }
    Output { code, map, diagnostics }
}

/// Noms définis au premier niveau d'un fichier `.rsc` ou `.rs`, ou `None` si on
/// ne peut pas le déterminer sans expansion de macros.
pub fn top_level_names(src: &str) -> Option<Vec<String>> {
    emit::top_level_names(&toks::Toks::new(src))
}

/// Déclarations `mod` d'un fichier Rust (pour construire l'arbre de modules).
pub fn mod_decls(src: &str) -> Vec<ModDecl> {
    parser::mod_decls(&toks::Toks::new(src))
}

/// `#![cfg(...)]` en tête d'un fichier, convertis en attributs externes
/// (`#[cfg(...)]`) à reporter sur sa déclaration `mod` et ses réexports.
pub fn leading_cfgs(src: &str) -> Vec<String> {
    let t = toks::Toks::new(src);
    let mut out = Vec::new();
    let mut p = 0;
    while let Some(next) = t.inner_attr_end(p, t.len()) {
        if t.is_punct(p, '#') && matches!(t.text(p + 3), "cfg" | "cfg_attr") {
            let text = emit::oneline_text(&t, toks::TokRange::new(p, next));
            out.push(text.replacen("#!", "#", 1).replacen("# !", "#", 1));
        }
        p = next;
    }
    out
}
