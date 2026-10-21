//! `cargo crust migrate` : `struct` + `impl` → `class`, dans les cas sans ambiguïté.
//!
//! Une `struct` à champs nommés, au premier niveau du fichier, devient une classe
//! avec :
//! - ses `impl` inhérents dont les paramètres génériques et la clause `where` sont
//!   exactement ceux de la `struct` (et sans attribut) ;
//! - ses `impl Trait for` dont les paramètres et prédicats commencent par ceux de
//!   la `struct` (le reste devient `impl<…> Trait where … { }` dans la classe).
//!
//! Tout le reste (impl partiel `impl Pile<i32>`, `impl<T: Copy> Pile<T>`, impl avec
//! attributs…) reste tel quel hors de la classe : c'est du Rust valide dans un `.rsc`.

use crate::Span;
use crate::emit::oneline_text;
use crate::lexer::TokenKind;
use crate::parser::{Generics, ParamKind, generics_at, skip_item_at};
use crate::toks::{TokRange, Toks};

struct StructItem {
    span: Span,
    /// Attributs et doc (de leur début au mot `struct` / à la visibilité).
    head: Span,
    vis: Option<TokRange>,
    name: String,
    generics: Option<Generics>,
    where_preds: Option<TokRange>,
    open: usize,
    close: usize,
}

struct ImplItem {
    span: Span,
    has_attrs: bool,
    attrs: Span,
    is_unsafe: bool,
    generics: Option<Generics>,
    negative: bool,
    trait_path: Option<TokRange>,
    self_ty: TokRange,
    where_preds: Option<TokRange>,
    open: usize,
    close: usize,
}

/// Texte sans espaces, pour comparer des fragments de code.
fn compact(t: &Toks, r: TokRange) -> String {
    (r.start..r.end).map(|p| t.text(p)).collect()
}

/// Découpe une liste séparée par des virgules au premier niveau (`<>`, `()`…).
fn split_top(t: &Toks, r: TokRange) -> Vec<TokRange> {
    let mut out = Vec::new();
    let mut start = r.start;
    let mut depth = 0usize;
    let mut p = r.start;
    while p < r.end {
        if t.is_open(p) {
            p = t.group_end(p, r.end) + 1;
            continue;
        }
        match t.kind(p) {
            Some(TokenKind::Punct('<')) => depth += 1,
            Some(TokenKind::Punct('>')) if !(t.is_punct(p - 1, '-') && t.joint(p - 1)) => {
                depth = depth.saturating_sub(1)
            }
            Some(TokenKind::Punct(',')) if depth == 0 => {
                if p > start {
                    out.push(TokRange::new(start, p));
                }
                start = p + 1;
            }
            _ => {}
        }
        p += 1;
    }
    if r.end > start {
        out.push(TokRange::new(start, r.end));
    }
    out
}

/// Jusqu'au `{` du corps (ou `where`), au premier niveau.
fn until_body(t: &Toks, mut p: usize, end: usize, stop_at_where: bool, stop_at_for: bool) -> usize {
    let mut depth = 0usize;
    while p < end {
        if t.is_open(p) {
            if t.is_punct(p, '{') && depth == 0 {
                return p;
            }
            p = t.group_end(p, end) + 1;
            continue;
        }
        match t.kind(p) {
            Some(TokenKind::Punct('<')) => depth += 1,
            Some(TokenKind::Punct('>')) if !(t.is_punct(p - 1, '-') && t.joint(p - 1)) => {
                depth = depth.saturating_sub(1)
            }
            Some(TokenKind::Ident) if depth == 0 && stop_at_where && t.text(p) == "where" => return p,
            Some(TokenKind::Ident) if depth == 0 && stop_at_for && t.text(p) == "for" => return p,
            Some(TokenKind::Punct(';')) if depth == 0 => return p,
            _ => {}
        }
        p += 1;
    }
    end
}

fn where_clause(t: &Toks, p: usize, end: usize) -> (Option<TokRange>, usize) {
    if !t.is_ident(p, "where") {
        return (None, p);
    }
    let body = until_body(t, p + 1, end, false, false);
    (Some(TokRange::new(p + 1, body)), body)
}

fn scan(t: &Toks) -> (Vec<StructItem>, Vec<ImplItem>) {
    let end = t.len();
    let mut structs = Vec::new();
    let mut impls = Vec::new();
    let mut p = 0;
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
        let attrs_end = q;
        let vis = t.vis_end(q, end).map(|v| TokRange::new(q, v));
        let k = vis.map_or(q, |v| v.end);
        let item_end = skip_item_at(t, k.max(start), end).max(start + 1);
        let span = t.range_span(TokRange::new(start, item_end));
        let head = Span::new(t.span(start).start, t.span(attrs_end).start.max(t.span(start).start));

        if t.is_ident(k, "struct") && t.is_name(k + 1) {
            let mut r = k + 2;
            let generics = if t.is_punct(r, '<') {
                generics_at(t, r, end).map(|(g, next)| {
                    r = next;
                    g
                })
            } else {
                None
            };
            let (where_preds, body) = where_clause(t, r, end);
            let r = if where_preds.is_some() { body } else { r };
            if t.is_punct(r, '{') && t.is_closed_group(r, end) {
                structs.push(StructItem {
                    span,
                    head,
                    vis,
                    name: t.text(k + 1).to_string(),
                    generics,
                    where_preds,
                    open: r,
                    close: t.group_end(r, end),
                });
            }
        } else if vis.is_none() && (t.is_ident(k, "impl") || (t.is_ident(k, "unsafe") && t.is_ident(k + 1, "impl"))) {
            let is_unsafe = t.is_ident(k, "unsafe");
            let mut r = if is_unsafe { k + 2 } else { k + 1 };
            let generics = if t.is_punct(r, '<') {
                generics_at(t, r, end).map(|(g, next)| {
                    r = next;
                    g
                })
            } else {
                None
            };
            let negative = t.is_punct(r, '!');
            if negative {
                r += 1;
            }
            let first_end = until_body(t, r, end, true, true);
            let (trait_path, self_start) = if t.is_ident(first_end, "for") {
                (Some(TokRange::new(r, first_end)), first_end + 1)
            } else {
                (None, r)
            };
            let self_end = until_body(t, self_start, end, true, false);
            let (where_preds, body) = where_clause(t, self_end, end);
            let body = if where_preds.is_some() { body } else { self_end };
            if t.is_punct(body, '{') && t.is_closed_group(body, end) {
                impls.push(ImplItem {
                    span,
                    has_attrs: attrs_end > start,
                    attrs: Span::new(t.span(start).start, t.span(attrs_end).start.max(t.span(start).start)),
                    is_unsafe,
                    generics,
                    negative,
                    trait_path,
                    self_ty: TokRange::new(self_start, self_end),
                    where_preds,
                    open: body,
                    close: t.group_end(body, end),
                });
            }
        }
        p = item_end;
    }
    (structs, impls)
}

/// Paramètres sans valeur par défaut, en texte compact, séparés lifetimes / autres.
fn params(t: &Toks, g: Option<&Generics>) -> (Vec<String>, Vec<String>) {
    let mut lifetimes = Vec::new();
    let mut others = Vec::new();
    for p in g.iter().flat_map(|g| g.params.iter()) {
        let s = compact(t, p.without_default);
        if p.kind == ParamKind::Lifetime {
            lifetimes.push(s)
        } else {
            others.push(s)
        }
    }
    (lifetimes, others)
}

fn preds(t: &Toks, w: Option<TokRange>) -> Vec<TokRange> {
    w.map(|r| split_top(t, r)).unwrap_or_default()
}

/// Type `Self` attendu : `Nom` ou `Nom<'a,T,N>` (compact).
fn self_type(t: &Toks, s: &StructItem) -> String {
    let names: Vec<&str> = s
        .generics
        .iter()
        .flat_map(|g| g.params.iter())
        .map(|p| t.text(p.name))
        .collect();
    if names.is_empty() {
        s.name.clone()
    } else {
        format!("{}<{}>", s.name, names.join(","))
    }
}

/// Paramètres et prédicats en plus de ceux de la classe, si `impl` commence par eux.
fn extra(t: &Toks, s: &StructItem, i: &ImplItem) -> Option<(Vec<TokRange>, Vec<TokRange>)> {
    let (sl, so) = params(t, s.generics.as_ref());
    let all: Vec<_> = i.generics.iter().flat_map(|g| g.params.iter()).collect();
    let il: Vec<_> = all.iter().filter(|p| p.kind == ParamKind::Lifetime).collect();
    let io: Vec<_> = all.iter().filter(|p| p.kind != ParamKind::Lifetime).collect();
    if il.len() < sl.len() || io.len() < so.len() {
        return None;
    }
    if il.iter().zip(&sl).any(|(p, s)| compact(t, p.without_default) != *s)
        || io.iter().zip(&so).any(|(p, s)| compact(t, p.without_default) != *s)
    {
        return None;
    }
    let extra_params: Vec<TokRange> = il[sl.len()..]
        .iter()
        .chain(io[so.len()..].iter())
        .map(|p| p.without_default)
        .collect();
    let sp: Vec<String> = preds(t, s.where_preds).iter().map(|r| compact(t, *r)).collect();
    let ip = preds(t, i.where_preds);
    if ip.len() < sp.len() || ip.iter().zip(&sp).any(|(r, s)| compact(t, *r) != *s) {
        return None;
    }
    Some((extra_params, ip[sp.len()..].to_vec()))
}

/// Résultat d'une conversion.
pub struct Migrated {
    pub text: String,
    /// Noms des classes créées.
    pub classes: Vec<String>,
}

/// Convertit les `struct` + `impl` du fichier en classes. Le texte rendu n'est pas
/// formaté (passer ensuite par [`crate::fmt::format`]).
pub fn structs_to_classes(src: &str) -> Migrated {
    let t = Toks::new(src);
    let (structs, impls) = scan(&t);
    let mut used = vec![false; impls.len()];
    let mut replacements: Vec<(Span, String)> = Vec::new();
    let mut classes = Vec::new();

    for s in &structs {
        let self_ty = self_type(&t, s);
        let mut inherent = Vec::new();
        let mut traits = Vec::new();
        for (k, i) in impls.iter().enumerate() {
            if used[k] || compact(&t, i.self_ty) != self_ty {
                continue;
            }
            match &i.trait_path {
                None => {
                    let same = params(&t, i.generics.as_ref()) == params(&t, s.generics.as_ref())
                        && preds(&t, i.where_preds)
                            .iter()
                            .map(|r| compact(&t, *r))
                            .collect::<Vec<_>>()
                            == preds(&t, s.where_preds)
                                .iter()
                                .map(|r| compact(&t, *r))
                                .collect::<Vec<_>>();
                    if same && !i.has_attrs && !i.is_unsafe && !i.negative {
                        used[k] = true;
                        inherent.push(k);
                    }
                }
                Some(_) => {
                    if let Some(x) = extra(&t, s, i) {
                        used[k] = true;
                        traits.push((k, x));
                    }
                }
            }
        }
        if inherent.is_empty() && traits.is_empty() {
            continue;
        }

        let body = |i: &ImplItem| src[t.span(i.open).end..t.span(i.close).start].trim().to_string();
        let mut out = String::new();
        out.push_str(src[s.head.start..s.head.end].trim_end());
        if !out.is_empty() {
            out.push('\n');
        }
        if let Some(v) = s.vis {
            out.push_str(&oneline_text(&t, v));
            out.push(' ');
        }
        out.push_str("class ");
        out.push_str(&s.name);
        if let Some(g) = &s.generics {
            out.push_str(&oneline_text(&t, g.range));
        }
        if let Some(w) = s.where_preds {
            out.push_str(" where ");
            out.push_str(&oneline_text(&t, w));
        }
        out.push_str(" {\n");
        let fields = src[t.span(s.open).end..t.span(s.close).start].trim();
        if !fields.is_empty() {
            out.push_str(fields);
            if !fields.ends_with(',') {
                out.push(',');
            }
            out.push_str("\n\n");
        }
        for &k in &inherent {
            let b = body(&impls[k]);
            if !b.is_empty() {
                out.push_str(&b);
                out.push_str("\n\n");
            }
        }
        for (k, (extra_params, extra_preds)) in &traits {
            let i = &impls[*k];
            let attrs = src[i.attrs.start..i.attrs.end].trim();
            if !attrs.is_empty() {
                out.push_str(attrs);
                out.push('\n');
            }
            if i.is_unsafe {
                out.push_str("unsafe ");
            }
            out.push_str("impl");
            if !extra_params.is_empty() {
                let p: Vec<String> = extra_params.iter().map(|r| oneline_text(&t, *r)).collect();
                out.push_str(&format!("<{}>", p.join(", ")));
            }
            out.push(' ');
            if i.negative {
                out.push('!');
            }
            out.push_str(&oneline_text(&t, i.trait_path.unwrap()));
            if !extra_preds.is_empty() {
                let p: Vec<String> = extra_preds.iter().map(|r| oneline_text(&t, *r)).collect();
                out.push_str(&format!(" where {}", p.join(", ")));
            }
            out.push_str(" {\n");
            out.push_str(&body(i));
            out.push_str("\n}\n\n");
        }
        out = out.trim_end().to_string();
        out.push_str("\n}");
        replacements.push((s.span, out));
        for &k in inherent.iter().chain(traits.iter().map(|(k, _)| k)) {
            // L'impl, ses attributs compris, quitte sa place.
            replacements.push((impls[k].span, String::new()));
        }
        classes.push(s.name.clone());
    }

    replacements.sort_by_key(|(s, _)| s.start);
    let mut text = String::with_capacity(src.len());
    let mut pos = 0;
    for (span, rep) in replacements {
        text.push_str(&src[pos..span.start]);
        text.push_str(&rep);
        pos = span.end;
    }
    text.push_str(&src[pos..]);
    Migrated { text, classes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_et_impls() {
        let src = "/// Doc\n#[derive(Debug)]\npub struct Moteur {\n    pub vitesse: u32,\n}\n\nimpl Moteur {\n    pub fn new() -> Self { Self { vitesse: 0 } }\n}\n\nimpl Default for Moteur {\n    fn default() -> Self { Self::new() }\n}\n\nfn libre() {}\n";
        let m = structs_to_classes(src);
        assert_eq!(m.classes, vec!["Moteur"]);
        assert!(
            m.text
                .contains("#[derive(Debug)]\npub class Moteur {\npub vitesse: u32,"),
            "{}",
            m.text
        );
        assert!(m.text.contains("impl Default {"), "{}", m.text);
        assert!(!m.text.contains("impl Moteur"), "{}", m.text);
        assert!(m.text.contains("fn libre() {}"));
    }

    #[test]
    fn generiques() {
        let src = "pub struct Pile<'a, T: Clone> where T: Default { items: Vec<&'a T> }\n\
                   impl<'a, T: Clone> Pile<'a, T> where T: Default { fn len(&self) -> usize { self.items.len() } }\n\
                   impl<'a, 'b, T: Clone, U> From<(&'b U, T)> for Pile<'a, T> where T: Default, U: Into<T> { fn from(_: (&'b U, T)) -> Self { todo!() } }\n\
                   impl<'a> Pile<'a, i32> { fn special(&self) {} }\n\
                   #[cfg(test)] impl<'a, T: Clone> Pile<'a, T> where T: Default { fn test(&self) {} }\n";
        let m = structs_to_classes(src);
        assert!(
            m.text.contains("impl<'b, U> From<(&'b U, T)> where U: Into<T> {"),
            "{}",
            m.text
        );
        // Impl partiel et impl avec attributs : restent dehors.
        assert!(m.text.contains("impl<'a> Pile<'a, i32>"), "{}", m.text);
        assert!(
            m.text.contains("#[cfg(test)] impl<'a, T: Clone> Pile<'a, T>"),
            "{}",
            m.text
        );
    }

    #[test]
    fn rien_a_convertir() {
        let src = "struct Seule { x: u8 }\nstruct Tuple(u8);\nimpl Tuple { fn f(&self) {} }\n";
        let m = structs_to_classes(src);
        assert!(m.classes.is_empty());
        assert_eq!(m.text, src);
    }
}
