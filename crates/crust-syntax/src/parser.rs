//! Parseur léger : ne reconnaît que les items de module, les classes et les imports.
//!
//! Les corps de fonctions, de macros et d'items Rust ordinaires ne sont jamais
//! analysés : ce sont des groupes de tokens équilibrés, recopiés tels quels.

use crate::diag::{Code, Diagnostic};
use crate::lexer::TokenKind;
use crate::names;
use crate::toks::{TokRange, Toks};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrKind {
    /// Commentaire de doc ou `#[doc]` : sur la `struct` seulement.
    Doc,
    /// `#[cfg]`, `#[cfg_attr]` : recopié sur la `struct` et chaque `impl`.
    Cfg,
    /// `#[allow]`, `#[warn]`, `#[deny]`, `#[forbid]`, `#[expect]` : idem.
    Lint,
    /// Tout le reste (`derive`, `repr`, macros attributs…) : `struct` seulement.
    Other,
}

impl AttrKind {
    /// Attributs à recopier sur les `impl` générés (section 4.5).
    pub fn inherited(self) -> bool {
        matches!(self, AttrKind::Cfg | AttrKind::Lint)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Attr {
    pub range: TokRange,
    pub kind: AttrKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Lifetime,
    Type,
    Const,
}

#[derive(Clone, Copy, Debug)]
pub struct GenericParam {
    /// Le paramètre sans sa valeur par défaut (forme utilisée dans un `impl`).
    pub without_default: TokRange,
    pub kind: ParamKind,
    /// Token du nom (`'a`, `T`, `N`).
    pub name: usize,
}

#[derive(Clone, Debug)]
pub struct Generics {
    /// De `<` à `>` inclus.
    pub range: TokRange,
    pub params: Vec<GenericParam>,
}

#[derive(Clone, Debug)]
pub struct OverItem {
    pub range: TokRange,
    pub vis: Option<TokRange>,
    pub ty: TokRange,
    /// Nom explicite `(nom)` : indice du token.
    pub explicit_name: Option<usize>,
    /// Nom du champ (explicite, ou déduit pour un type chemin).
    pub field_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Field {
    /// Attributs, visibilité, nom et type (sans la virgule).
    pub range: TokRange,
    pub name: usize,
    /// Fin du membre dans la source, virgule comprise.
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct TraitImpl {
    /// Du premier attribut au `}` final.
    pub range: TokRange,
    /// Premier token de l'en-tête (`unsafe` ou `impl`).
    pub header: usize,
    pub is_unsafe: bool,
    pub generics: Option<Generics>,
    pub negative: bool,
    pub trait_path: TokRange,
    /// Prédicats `where`, sans le mot-clé.
    pub where_preds: Option<TokRange>,
    pub open: usize,
    pub close: usize,
}

#[derive(Clone, Debug)]
pub enum Member {
    Field(Field),
    /// Méthode, constante, macro, attribut interne : recopié dans l'`impl` inhérent.
    Item(TokRange),
    TraitImpl(TraitImpl),
    /// Élément refusé ou invalide : remplacé par un `compile_error!`.
    Error(TokRange, Diagnostic),
}

impl Member {
    pub fn range(&self) -> TokRange {
        match self {
            Member::Field(f) => TokRange::new(f.range.start, f.end),
            Member::Item(r) | Member::Error(r, _) => *r,
            Member::TraitImpl(t) => t.range,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Class {
    /// Du premier attribut au `}` final.
    pub range: TokRange,
    pub attrs: Vec<Attr>,
    pub vis: Option<TokRange>,
    /// Premier token de l'en-tête (visibilité ou `class`).
    pub header: usize,
    pub name: usize,
    pub generics: Option<Generics>,
    pub overs: Vec<OverItem>,
    pub where_preds: Option<TokRange>,
    pub open: usize,
    pub close: usize,
    pub members: Vec<Member>,
    /// Erreurs de l'en-tête (CR0003, CR0004) : injectées après l'ouverture de l'`impl`.
    pub header_errors: Vec<Diagnostic>,
}

#[derive(Clone, Debug)]
pub enum ImportList {
    Single(usize),
    /// `(nom, alias)` pour chaque entrée de `{ ... }`.
    Many(Vec<(usize, Option<usize>)>),
    Glob(usize),
}

#[derive(Clone, Debug)]
pub struct Import {
    /// Des attributs au `;` inclus.
    pub range: TokRange,
    pub attrs: TokRange,
    pub vis: Option<TokRange>,
    pub list: ImportList,
    /// Token de la chaîne du chemin.
    pub path: usize,
    pub path_value: String,
}

#[derive(Clone, Debug)]
pub enum Item {
    Class(Class),
    Import(Import),
    /// Construction Crust mal formée : remplacée par un `compile_error!`.
    Broken(TokRange, Diagnostic),
}

impl Item {
    pub fn range(&self) -> TokRange {
        match self {
            Item::Class(c) => c.range,
            Item::Import(i) => i.range,
            Item::Broken(r, _) => *r,
        }
    }
}

pub fn parse(t: &Toks) -> Vec<Item> {
    let mut items = Vec::new();
    Parser { t }.module_items(0, t.len(), &mut items);
    items
}

struct Parser<'t, 'a> {
    t: &'t Toks<'a>,
}

impl Parser<'_, '_> {
    fn module_items(&self, mut p: usize, end: usize, items: &mut Vec<Item>) {
        let t = self.t;
        while p < end {
            if let Some(next) = t.inner_attr_end(p, end) {
                p = next;
                continue;
            }
            let start = p;
            let mut q = p;
            while let Some(next) = t.outer_attr_end(q, end) {
                q = next;
            }
            let vis = t.vis_end(q, end).map(|v| TokRange::new(q, v));
            let k = vis.map_or(q, |v| v.end);

            if t.is_ident(k, "class") && t.is_name(k + 1) && k + 1 < end {
                let (item, next) = self.class(start, q, vis, k, end);
                items.push(item);
                p = next;
            } else if t.is_ident(k, "import")
                && k + 1 < end
                && (t.is_name(k + 1) || t.is_punct(k + 1, '{') || t.is_punct(k + 1, '*'))
            {
                let (item, next) = self.import(start, q, vis, k, end);
                items.push(item);
                p = next;
            } else if t.is_ident(k, "mod") && t.is_name(k + 1) && t.is_punct(k + 2, '{') && k + 2 < end {
                let close = t.group_end(k + 2, end);
                self.module_items(k + 3, close, items);
                p = close + 1;
            } else {
                p = self.skip_item(k.max(start), end).max(start + 1);
            }
        }
    }

    /// Saute un item Rust ordinaire : jusqu'au `;` ou au groupe `{}` qui le termine.
    fn skip_item(&self, mut p: usize, end: usize) -> usize {
        let t = self.t;
        while p < end {
            if t.is_open(p) {
                let close = t.group_end(p, end);
                if t.is_punct(p, '{') {
                    let mut next = close + 1;
                    if next < end && t.is_punct(next, ';') {
                        next += 1;
                    }
                    return next;
                }
                p = close + 1;
            } else if t.is_punct(p, ';') {
                return p + 1;
            } else {
                p += 1;
            }
        }
        end
    }

    fn attrs(&self, start: usize, end: usize) -> Vec<Attr> {
        let t = self.t;
        let mut out = Vec::new();
        let mut p = start;
        while p < end {
            let Some(next) = t.outer_attr_end(p, end) else { break };
            let kind = if t.doc_style(p).is_some() {
                AttrKind::Doc
            } else {
                match t.text(p + 2) {
                    "doc" => AttrKind::Doc,
                    "cfg" | "cfg_attr" => AttrKind::Cfg,
                    "allow" | "warn" | "deny" | "forbid" | "expect" => AttrKind::Lint,
                    _ => AttrKind::Other,
                }
            };
            out.push(Attr {
                range: TokRange::new(p, next),
                kind,
            });
            p = next;
        }
        out
    }

    fn error(&self, p: usize, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Code::CR0001, self.t.span(p), message)
    }

    // ---------------------------------------------------------------- class

    fn class(&self, start: usize, attrs_end: usize, vis: Option<TokRange>, kw: usize, end: usize) -> (Item, usize) {
        let t = self.t;
        let broken = |diag: Diagnostic| {
            let next = self.skip_item(kw, end).max(kw + 1);
            (Item::Broken(TokRange::new(start, next), diag), next)
        };
        let name = kw + 1;
        let mut p = kw + 2;

        let generics = if t.is_punct(p, '<') {
            match self.generics(p, end) {
                Ok((g, next)) => {
                    p = next;
                    Some(g)
                }
                Err(d) => return broken(d),
            }
        } else {
            None
        };

        let mut header_errors = Vec::new();
        let mut overs = Vec::new();
        if t.is_ident(p, "over") {
            match self.overs(p + 1, end) {
                Ok((o, next)) => {
                    overs = o;
                    p = next;
                }
                Err(d) => return broken(d),
            }
        }

        let where_preds = if t.is_ident(p, "where") {
            let (preds, next) = self.where_clause(p, end);
            p = next;
            Some(preds)
        } else {
            None
        };

        if !t.is_punct(p, '{') || p >= end {
            let what = if p < end {
                format!("`{}`", t.text(p))
            } else {
                "la fin du fichier".into()
            };
            return broken(
                self.error(
                    p.min(end.saturating_sub(1)),
                    format!("`{{` attendu après l'en-tête de la classe, trouvé {what}"),
                )
                .help("syntaxe : `class Nom<T> over Type(nom) where ... { ... }`"),
            );
        }
        let open = p;
        if !t.is_closed_group(open, end) {
            return broken(self.error(open, "accolade de la classe jamais fermée"));
        }
        let close = t.group_end(open, end);
        let members = self.members(open + 1, close);

        check_overs(t, &mut overs, &mut header_errors);
        let mut class = Class {
            range: TokRange::new(start, close + 1),
            attrs: self.attrs(start, attrs_end),
            vis,
            header: vis.map_or(kw, |v| v.start),
            name,
            generics,
            overs,
            where_preds,
            open,
            close,
            members,
            header_errors,
        };
        check_field_collisions(t, &mut class);
        (Item::Class(class), close + 1)
    }

    /// `<...>` en `p`. Les `>` de `->` ne ferment pas.
    fn generics(&self, p: usize, end: usize) -> Result<(Generics, usize), Diagnostic> {
        let t = self.t;
        let mut params = Vec::new();
        let mut depth = 0usize;
        let mut q = p + 1;
        let mut param_start = q;
        let mut eq: Option<usize> = None;
        let push = |a: usize, b: usize, eq: Option<usize>, params: &mut Vec<GenericParam>| {
            if a >= b {
                return;
            }
            let mut n = a;
            while let Some(next) = t.outer_attr_end(n, b) {
                n = next;
            }
            let (kind, name) = if t.kind(n) == Some(TokenKind::Lifetime) {
                (ParamKind::Lifetime, n)
            } else if t.is_ident(n, "const") {
                (ParamKind::Const, n + 1)
            } else {
                (ParamKind::Type, n)
            };
            params.push(GenericParam {
                without_default: TokRange::new(a, eq.unwrap_or(b)),
                kind,
                name,
            });
        };
        while q < end {
            if t.is_open(q) {
                q = t.group_end(q, end) + 1;
                continue;
            }
            match t.kind(q) {
                Some(TokenKind::Punct('<')) => depth += 1,
                Some(TokenKind::Punct('>')) if !(q > 0 && t.is_punct(q - 1, '-') && t.joint(q - 1)) => {
                    if depth == 0 {
                        push(param_start, q, eq, &mut params);
                        return Ok((
                            Generics {
                                range: TokRange::new(p, q + 1),
                                params,
                            },
                            q + 1,
                        ));
                    }
                    depth -= 1;
                }
                Some(TokenKind::Punct(',')) if depth == 0 => {
                    push(param_start, q, eq.take(), &mut params);
                    param_start = q + 1;
                }
                Some(TokenKind::Punct('=')) if depth == 0 && eq.is_none() => eq = Some(q),
                Some(TokenKind::Punct('{' | ';')) => break,
                _ => {}
            }
            q += 1;
        }
        Err(self.error(p, "paramètres génériques non fermés : `>` attendu"))
    }

    /// Prédicats `where` jusqu'au `{` du corps.
    fn where_clause(&self, p: usize, end: usize) -> (TokRange, usize) {
        let t = self.t;
        let mut q = p + 1;
        while q < end && !t.is_punct(q, '{') {
            q = if t.is_open(q) { t.group_end(q, end) + 1 } else { q + 1 };
        }
        (TokRange::new(p + 1, q), q)
    }

    /// Liste `over A, B(nom), pub(crate) C` jusqu'à `where` ou `{`.
    fn overs(&self, mut p: usize, end: usize) -> Result<(Vec<OverItem>, usize), Diagnostic> {
        let t = self.t;
        let mut out = Vec::new();
        loop {
            let start = p;
            let vis = t.vis_end(p, end).map(|v| TokRange::new(p, v));
            let ty_start = vis.map_or(p, |v| v.end);
            let mut q = ty_start;
            let mut depth = 0usize;
            while q < end {
                if t.is_open(q) {
                    if t.is_punct(q, '{') && depth == 0 {
                        break;
                    }
                    q = t.group_end(q, end) + 1;
                    continue;
                }
                match t.kind(q) {
                    Some(TokenKind::Punct('<')) => depth += 1,
                    Some(TokenKind::Punct('>')) if !(t.is_punct(q - 1, '-') && t.joint(q - 1)) => {
                        depth = depth.saturating_sub(1)
                    }
                    Some(TokenKind::Punct(',')) if depth == 0 => break,
                    Some(TokenKind::Ident) if depth == 0 && t.text(q) == "where" => break,
                    _ => {}
                }
                q += 1;
            }
            if q == ty_start {
                return Err(self.error(q.min(end - 1), "type attendu après `over`"));
            }
            // `Type(nom)` : le dernier groupe `( ident )` donne le nom du champ.
            let mut ty = TokRange::new(ty_start, q);
            let mut explicit_name = None;
            if t.is_punct(q - 1, ')') {
                let open = (ty_start..q - 1)
                    .rev()
                    .find(|&o| t.is_punct(o, '(') && t.group_end(o, end) == q - 1);
                if let Some(open) = open
                    && open > ty_start
                    && q - 1 == open + 2
                    && matches!(t.kind(open + 1), Some(TokenKind::Ident | TokenKind::RawIdent))
                {
                    explicit_name = Some(open + 1);
                    ty = TokRange::new(ty_start, open);
                }
            }
            out.push(OverItem {
                range: TokRange::new(start, q),
                vis,
                ty,
                explicit_name,
                field_name: None,
            });
            if t.is_punct(q, ',') {
                p = q + 1;
            } else {
                return Ok((out, q));
            }
        }
    }

    // -------------------------------------------------------------- members

    fn members(&self, mut p: usize, close: usize) -> Vec<Member> {
        let t = self.t;
        let mut out = Vec::new();
        while p < close {
            if let Some(next) = t.inner_attr_end(p, close) {
                out.push(Member::Item(TokRange::new(p, next)));
                p = next;
                continue;
            }
            let start = p;
            let mut q = p;
            while let Some(next) = t.outer_attr_end(q, close) {
                q = next;
            }
            let k = t.vis_end(q, close).unwrap_or(q);
            let (member, next) = self.member(start, k, close);
            out.push(member);
            p = next.max(start + 1);
        }
        out
    }

    fn member(&self, start: usize, k: usize, close: usize) -> (Member, usize) {
        let t = self.t;
        let forbidden = |what: &str, help: &str| {
            let next = self.skip_item(k, close).max(k + 1);
            let diag = Diagnostic::new(
                Code::CR0005,
                t.span(k),
                format!("{what} interdit dans le corps d'une classe"),
            )
            .help(help.to_string());
            (Member::Error(TokRange::new(start, next), diag), next)
        };

        if k >= close {
            let diag = self.error(start, "attribut ou visibilité sans élément");
            return (Member::Error(TokRange::new(start, close), diag), close);
        }
        if t.is_ident(k, "impl") || (t.is_ident(k, "unsafe") && t.is_ident(k + 1, "impl")) {
            return self.trait_impl(start, k, close);
        }
        if t.is_name(k) && t.is_punct(k + 1, ':') && !t.is_path_sep(k + 1) {
            return self.field(start, k, close);
        }
        let text = t.text(k);
        match text {
            "type" => {
                return forbidden(
                    "un type associé inhérent",
                    "les types associés ne sont permis que dans un `impl Trait { }`",
                );
            }
            "struct" | "enum" | "trait" | "mod" | "static" => {
                return forbidden(
                    &format!("`{text}`"),
                    "déclarez-le au niveau du module, à côté de la classe",
                );
            }
            "use" => return forbidden("`use`", "placez les `use` au niveau du fichier"),
            "class" if t.is_name(k + 1) => return forbidden("une classe imbriquée", "déclarez-la au niveau du module"),
            "import" if t.is_name(k + 1) || t.is_punct(k + 1, '{') => {
                return forbidden("`import`", "placez les `import` au niveau du fichier");
            }
            "union" if t.is_name(k + 1) => return forbidden("`union`", "déclarez-la au niveau du module"),
            "extern" if t.is_ident(k + 1, "crate") => {
                return forbidden("`extern crate`", "placez-le au niveau de la crate");
            }
            _ => {}
        }
        if matches!(t.kind(k), Some(TokenKind::Ident) | Some(TokenKind::RawIdent)) || t.is_path_sep(k) {
            let next = self.skip_item(k, close);
            return (Member::Item(TokRange::new(start, next)), next);
        }
        let diag = self
            .error(k, format!("élément inattendu dans la classe : `{}`", t.text(k)))
            .help("une classe contient des champs `nom: Type,`, des fonctions et des `impl Trait { }`");
        let next = if t.is_open(k) { t.group_end(k, close) + 1 } else { k + 1 };
        (Member::Error(TokRange::new(start, next), diag), next)
    }

    fn field(&self, start: usize, name: usize, close: usize) -> (Member, usize) {
        let t = self.t;
        let mut q = name + 2;
        let mut depth = 0usize;
        let mut missing_comma = false;
        while q < close {
            if t.is_punct(q, ',') && depth == 0 {
                break;
            }
            // Virgule oubliée : un nouveau membre commence sur une nouvelle ligne.
            if depth == 0
                && q > name + 2
                && t.newline_before(q)
                && (t.outer_attr_end(q, close).is_some()
                    || matches!(
                        t.text(q),
                        "pub" | "fn" | "const" | "async" | "unsafe" | "impl" | "extern"
                    )
                    || (t.is_name(q) && t.is_punct(q + 1, ':') && !t.is_path_sep(q + 1)))
            {
                missing_comma = true;
                break;
            }
            if t.is_open(q) {
                if t.is_punct(q, '{') && depth == 0 {
                    missing_comma = true;
                    break;
                }
                q = t.group_end(q, close) + 1;
                continue;
            }
            match t.kind(q) {
                Some(TokenKind::Punct('<')) => depth += 1,
                Some(TokenKind::Punct('>')) if !(t.is_punct(q - 1, '-') && t.joint(q - 1)) => {
                    depth = depth.saturating_sub(1)
                }
                Some(TokenKind::Punct(';')) if depth == 0 => {
                    missing_comma = true;
                    break;
                }
                _ => {}
            }
            q += 1;
        }
        let range = TokRange::new(start, q);
        if q == name + 2 {
            let diag = self.error(name + 1, "type attendu après `:`");
            let next = if t.is_punct(q, ',') { q + 1 } else { q };
            return (Member::Error(TokRange::new(start, next), diag), next);
        }
        if missing_comma {
            let diag = self
                .error(q - 1, "`,` attendue après le champ")
                .help("séparez les champs par des virgules");
            return (Member::Error(range, diag), q);
        }
        let end = if t.is_punct(q, ',') { q + 1 } else { q };
        (Member::Field(Field { range, name, end }), end)
    }

    fn trait_impl(&self, start: usize, k: usize, close: usize) -> (Member, usize) {
        let t = self.t;
        let is_unsafe = t.is_ident(k, "unsafe");
        let kw = if is_unsafe { k + 1 } else { k };
        let mut p = kw + 1;
        let bail = |diag: Diagnostic| {
            let next = self.skip_item(kw, close).max(kw + 1);
            (Member::Error(TokRange::new(start, next), diag), next)
        };
        let generics = if t.is_punct(p, '<') {
            match self.generics(p, close) {
                Ok((g, next)) => {
                    p = next;
                    Some(g)
                }
                Err(d) => return bail(d),
            }
        } else {
            None
        };
        let negative = t.is_punct(p, '!');
        if negative {
            p += 1;
        }
        let path_start = p;
        let mut depth = 0usize;
        while p < close {
            if t.is_open(p) {
                if t.is_punct(p, '{') && depth == 0 {
                    break;
                }
                p = t.group_end(p, close) + 1;
                continue;
            }
            match t.kind(p) {
                Some(TokenKind::Punct('<')) => depth += 1,
                Some(TokenKind::Punct('>')) if !(t.is_punct(p - 1, '-') && t.joint(p - 1)) => {
                    depth = depth.saturating_sub(1)
                }
                Some(TokenKind::Ident) if depth == 0 && t.text(p) == "where" => break,
                Some(TokenKind::Ident) if depth == 0 && t.text(p) == "for" => {
                    let next = self.skip_item(kw, close).max(kw + 1);
                    let diag = Diagnostic::new(Code::CR0005, t.span(p), "`impl Trait for AutreType` interdit dans une classe")
                        .help("dans une classe, `impl Trait { }` s'applique à la classe elle-même ; écrivez cet `impl` hors de la classe");
                    return (Member::Error(TokRange::new(start, next), diag), next);
                }
                _ => {}
            }
            p += 1;
        }
        if p == path_start {
            return bail(self.error(p.min(close - 1), "nom de trait attendu après `impl`"));
        }
        let trait_path = TokRange::new(path_start, p);
        let where_preds = if t.is_ident(p, "where") {
            let (preds, next) = self.where_clause(p, close);
            p = next;
            Some(preds)
        } else {
            None
        };
        if !t.is_punct(p, '{') || p >= close {
            return bail(self.error(p.min(close - 1), "`{` attendu après `impl Trait`"));
        }
        let open = p;
        let end = t.group_end(open, close);
        let ti = TraitImpl {
            range: TokRange::new(start, end + 1),
            header: k,
            is_unsafe,
            generics,
            negative,
            trait_path,
            where_preds,
            open,
            close: end,
        };
        (Member::TraitImpl(ti), end + 1)
    }

    // --------------------------------------------------------------- import

    fn import(&self, start: usize, attrs_end: usize, vis: Option<TokRange>, kw: usize, end: usize) -> (Item, usize) {
        let t = self.t;
        let broken = |diag: Diagnostic| {
            let next = self.skip_item(kw, end).max(kw + 1);
            (Item::Broken(TokRange::new(start, next), diag), next)
        };
        let help = "syntaxe : `import Nom from \"./Fichier.rsc\";` ou `import { A, B as C } from \"...\";`";
        let mut p = kw + 1;
        let list = if t.is_punct(p, '*') {
            p += 1;
            ImportList::Glob(p - 1)
        } else if t.is_punct(p, '{') {
            let close = t.group_end(p, end);
            let mut names = Vec::new();
            let mut q = p + 1;
            while q < close {
                if !t.is_name(q) {
                    return broken(
                        self.error(q, format!("nom attendu dans l'import, trouvé `{}`", t.text(q)))
                            .help(help),
                    );
                }
                let alias = if t.is_ident(q + 1, "as") {
                    if !t.is_name(q + 2) || q + 2 >= close {
                        return broken(self.error(q + 1, "nom attendu après `as`").help(help));
                    }
                    Some(q + 2)
                } else {
                    None
                };
                names.push((q, alias));
                q = alias.map_or(q + 1, |a| a + 1);
                if t.is_punct(q, ',') {
                    q += 1;
                } else if q < close {
                    return broken(
                        self.error(q, format!("`,` ou `}}` attendu, trouvé `{}`", t.text(q)))
                            .help(help),
                    );
                }
            }
            if names.is_empty() {
                return broken(self.error(p, "liste d'import vide").help(help));
            }
            p = close + 1;
            ImportList::Many(names)
        } else {
            p += 1;
            ImportList::Single(p - 1)
        };
        if !t.is_ident(p, "from") {
            return broken(
                self.error(p.min(end.saturating_sub(1)), "`from \"chemin\"` attendu")
                    .help(help),
            );
        }
        p += 1;
        let path = p;
        let text = t.text(path);
        if t.kind(path) != Some(TokenKind::Literal) || !text.starts_with('"') || !text.ends_with('"') || text.len() < 2
        {
            return broken(
                self.error(
                    path.min(end.saturating_sub(1)),
                    "chemin entre guillemets attendu après `from`",
                )
                .help(help),
            );
        }
        let path_value = text[1..text.len() - 1].replace("\\\\", "\\");
        p += 1;
        if !t.is_punct(p, ';') {
            return broken(self.error(p.min(end.saturating_sub(1)), "`;` attendu à la fin de l'import"));
        }
        let item = Import {
            range: TokRange::new(start, p + 1),
            attrs: TokRange::new(start, attrs_end),
            vis,
            list,
            path,
            path_value,
        };
        (Item::Import(item), p + 1)
    }
}

/// Noms des champs `over` (déduits ou explicites), CR0003 et CR0004.
fn check_overs(t: &Toks, overs: &mut [OverItem], errors: &mut Vec<Diagnostic>) {
    for o in overs.iter_mut() {
        o.field_name = match o.explicit_name {
            Some(n) => Some(t.text(n).to_string()),
            None => derived_name(t, o.ty).map(|n| names::ident_for(&names::to_snake_case(&n))),
        };
        if o.field_name.is_none() {
            errors.push(
                Diagnostic::new(
                    Code::CR0004,
                    t.range_span(o.ty),
                    "ce type n'est pas un chemin : le nom du champ ne peut pas être déduit",
                )
                .help(format!(
                    "nommez le champ explicitement : `over {}(nom)`",
                    crate::emit::oneline_text(t, o.ty)
                )),
            );
        }
    }
    for i in 0..overs.len() {
        let Some(name) = &overs[i].field_name else { continue };
        if let Some(first) = overs[..i].iter().find(|o| o.field_name.as_ref() == Some(name)) {
            errors.push(
                Diagnostic::new(
                    Code::CR0003,
                    t.range_span(overs[i].range),
                    format!("deux composants `over` génèrent le champ `{name}`"),
                )
                .label(t.range_span(first.range), "premier composant ici")
                .help("donnez un nom explicite à l'un d'eux : `over Type(autre_nom)`"),
            );
            overs[i].field_name = None;
        }
    }
}

/// Dernier segment d'un type chemin (`crate::io::Uart2<T>` → `Uart2`), sinon `None`.
fn derived_name(t: &Toks, ty: TokRange) -> Option<String> {
    let mut p = ty.start;
    if t.is_path_sep(p) {
        p += 2;
    }
    let mut last;
    loop {
        match t.kind(p) {
            Some(TokenKind::Ident)
                if !names::is_keyword(t.text(p)) || matches!(t.text(p), "crate" | "self" | "super" | "Self") => {}
            Some(TokenKind::RawIdent) => {}
            _ => return None,
        }
        last = p;
        p += 1;
        if t.is_path_sep(p) && p + 2 < ty.end && !t.is_punct(p + 2, '<') {
            p += 2;
            continue;
        }
        break;
    }
    // Arguments génériques éventuels (`<...>` ou `::<...>`) jusqu'à la fin du type.
    if p < ty.end {
        if t.is_path_sep(p) {
            p += 2;
        }
        if !t.is_punct(p, '<') || !t.is_punct(ty.end - 1, '>') {
            return None;
        }
    }
    let name = t.text(last);
    if matches!(name, "crate" | "self" | "super" | "Self") {
        return None;
    }
    Some(name.strip_prefix("r#").unwrap_or(name).to_string())
}

/// CR0002 : un champ déclaré porte le nom d'un champ `over`.
fn check_field_collisions(t: &Toks, class: &mut Class) {
    for m in class.members.iter_mut() {
        let Member::Field(f) = m else { continue };
        let fname = t.text(f.name);
        let fname = fname.strip_prefix("r#").unwrap_or(fname);
        let Some(o) = class
            .overs
            .iter()
            .find(|o| o.field_name.as_deref().map(|n| n.strip_prefix("r#").unwrap_or(n)) == Some(fname))
        else {
            continue;
        };
        let over_text = crate::emit::oneline_text(t, o.range);
        let diag = Diagnostic::new(
            Code::CR0002,
            t.span(f.name),
            format!("le champ `{fname}` est déjà généré par `over {over_text}`"),
        )
        .label(t.range_span(o.range), format!("génère le champ `{fname}`"))
        .help(match o.explicit_name {
            Some(_) => format!("renommez ce champ ou le composant `over {over_text}`"),
            None => format!("nommez explicitement le composant : `over {over_text}(autre_nom)`"),
        });
        *m = Member::Error(TokRange::new(f.range.start, f.end), diag);
    }
}
