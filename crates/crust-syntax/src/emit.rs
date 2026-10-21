//! Génération du Rust standard.
//!
//! - [`Layout::Lines`] : disposition préservant les lignes (section 15). Tout code
//!   utilisateur recopié garde sa ligne et, sauf sur les lignes d'en-tête, sa colonne.
//! - [`Layout::Natural`] : `struct` puis `impl`, pour la lecture (`cargo crust expand`).

use crate::diag::{Code, Diagnostic};
use crate::lexer::{DocStyle, TokenKind};
use crate::parser::{AttrKind, Class, FileMod, Generics, Import, ImportList, Item, Member, ParamKind, TraitImpl};
use crate::resolve::{ResolveError, Resolver};
use crate::sourcemap::{Segment, SegmentKind, SourceMap};
use crate::toks::{TokRange, Toks};
use crate::{Layout, Options, SourceInfo, Span};

/// Morceau de texte synthétisé : texte libre ou token recopié de la source.
enum Piece {
    Text(String, Span),
    Src(Span, bool),
}

#[derive(Default)]
struct Synth {
    pieces: Vec<Piece>,
}

impl Synth {
    fn text(&mut self, s: impl Into<String>, origin: Span) -> &mut Self {
        self.pieces.push(Piece::Text(s.into(), origin));
        self
    }

    fn src(&mut self, span: Span, primary: bool) -> &mut Self {
        self.pieces.push(Piece::Src(span, primary));
        self
    }

    /// Les tokens de `r` sur une seule ligne : trivia réduites à une espace,
    /// commentaires de doc convertis en `#[doc = "..."]`.
    fn oneline(&mut self, t: &Toks, r: TokRange) -> &mut Self {
        for p in r.start..r.end {
            let span = t.span(p);
            if p > r.start && t.tok(p - 1).end != span.start {
                self.text(" ", span);
            }
            match t.doc_style(p) {
                Some(style) => {
                    self.text(doc_attr(t.text(p), style), span);
                }
                None => {
                    self.src(span, false);
                }
            }
        }
        self
    }
}

/// `/// texte` → `#[doc = " texte"]`.
fn doc_attr(comment: &str, style: DocStyle) -> String {
    let body = if let Some(rest) = comment.strip_prefix("///").or_else(|| comment.strip_prefix("//!")) {
        rest.trim_end_matches('\r').to_string()
    } else {
        let inner = comment.get(3..comment.len().saturating_sub(2)).unwrap_or("");
        inner.to_string()
    };
    let bang = if style == DocStyle::Inner { "!" } else { "" };
    format!("#{bang}[doc = {body:?}]")
}

/// Texte d'une plage de tokens sur une ligne (pour les messages).
pub(crate) fn oneline_text(t: &Toks, r: TokRange) -> String {
    let mut s = String::new();
    for p in r.start..r.end {
        if p > r.start && t.tok(p - 1).end != t.tok(p).start {
            s.push(' ');
        }
        s.push_str(t.text(p));
    }
    s
}

struct Emitter<'a> {
    src: &'a str,
    out: String,
    map: SourceMap,
}

impl Emitter<'_> {
    fn seg(&mut self, gen_start: usize, src: Span, kind: SegmentKind, primary: bool) {
        let gen_len = self.out.len() - gen_start;
        self.map.push(Segment {
            gen_start,
            gen_len,
            src_start: src.start,
            src_len: src.len(),
            kind,
            primary,
        });
    }

    fn copy(&mut self, span: Span) {
        if span.is_empty() {
            return;
        }
        let start = self.out.len();
        self.out.push_str(&self.src[span.start..span.end]);
        self.seg(start, span, SegmentKind::Copied, false);
    }

    /// Remplace par des espaces en gardant sauts de ligne et tabulations.
    fn blank(&mut self, span: Span) {
        if span.is_empty() {
            return;
        }
        let start = self.out.len();
        for c in self.src[span.start..span.end].chars() {
            self.out.push(if matches!(c, '\n' | '\r' | '\t') { c } else { ' ' });
        }
        self.seg(start, span, SegmentKind::Blank, false);
    }

    fn synth(&mut self, s: &Synth) {
        for piece in &s.pieces {
            match piece {
                Piece::Text(text, origin) => {
                    let start = self.out.len();
                    self.out.push_str(text);
                    self.seg(start, *origin, SegmentKind::Synthesized, false);
                }
                Piece::Src(span, primary) => {
                    let start = self.out.len();
                    self.out.push_str(&self.src[span.start..span.end]);
                    self.seg(start, *span, SegmentKind::Copied, *primary);
                }
            }
        }
    }

    fn current_col(&self) -> usize {
        let line_start = self.out.rfind('\n').map_or(0, |i| i + 1);
        self.out[line_start..].chars().count()
    }

    /// Remplace `span` par `s` sur le même nombre de lignes, et aligne la colonne
    /// de ce qui suit quand c'est possible.
    fn replace(&mut self, span: Span, s: &Synth) {
        let col_before = self.current_col();
        self.synth(s);
        let original = &self.src[span.start..span.end];
        let start = self.out.len();
        let mut prev = '\0';
        for c in original.chars() {
            if c == '\n' {
                self.out.push_str(if prev == '\r' { "\r\n" } else { "\n" });
            }
            prev = c;
        }
        let target_col = match original.rfind('\n') {
            Some(i) => original[i + 1..].chars().count(),
            None => col_before + original.chars().count(),
        };
        let col = self.current_col();
        if target_col > col {
            self.out.extend(std::iter::repeat_n(' ', target_col - col));
        }
        self.seg(start, span, SegmentKind::Synthesized, false);
    }
}

pub(crate) fn emit(
    t: &Toks,
    items: &[Item],
    options: &Options,
    resolver: &dyn Resolver,
) -> (String, SourceMap, Vec<Diagnostic>) {
    let layout = options.layout;
    let mut cx = Cx {
        t,
        e: Emitter {
            src: t.src,
            out: String::with_capacity(t.src.len() + t.src.len() / 4),
            map: SourceMap::default(),
        },
        diags: Vec::new(),
        resolver,
        layout,
        source: options.source.as_ref(),
    };
    let mut pos = 0;
    for item in items {
        let span = t.range_span(item.range());
        cx.e.copy(Span::new(pos, span.start));
        match item {
            Item::Class(c) => match layout {
                Layout::Lines => cx.class_lines(c),
                Layout::Natural => cx.class_natural(c),
            },
            Item::Import(i) => cx.import(i),
            Item::FileMod(m) => cx.file_mod(m, span),
            Item::InnerAttr(r) => cx.inner_attr(*r, span),
            Item::Broken(_, diag) => {
                let mut s = Synth::default();
                s.text(diag.to_compile_error(), diag.span);
                cx.place(span, &s);
                cx.diags.push(diag.clone());
            }
        }
        pos = span.end;
    }
    cx.e.copy(Span::new(pos, t.src.len()));
    (cx.e.out, cx.e.map, cx.diags)
}

struct Cx<'t, 'a> {
    t: &'t Toks<'a>,
    e: Emitter<'a>,
    diags: Vec<Diagnostic>,
    resolver: &'t dyn Resolver,
    layout: Layout,
    source: Option<&'t SourceInfo>,
}

impl Cx<'_, '_> {
    /// `replace` en disposition préservant les lignes, simple écriture sinon.
    fn place(&mut self, span: Span, s: &Synth) {
        match self.layout {
            Layout::Lines => self.e.replace(span, s),
            Layout::Natural => self.e.synth(s),
        }
    }

    fn inherited_attrs(&self, c: &Class, s: &mut Synth) {
        for attr in c.attrs.iter().filter(|a| a.kind.inherited()) {
            s.oneline(self.t, attr.range);
            s.text(" ", self.t.range_span(attr.range));
        }
    }

    /// `<'a, 'b, T, U>` : lifetimes d'abord (classe puis impl), puis le reste.
    fn impl_generics(&self, s: &mut Synth, class: Option<&Generics>, extra: Option<&Generics>, origin: Span) {
        let all: Vec<_> = class.iter().chain(extra.iter()).flat_map(|g| g.params.iter()).collect();
        let ordered: Vec<_> = all
            .iter()
            .filter(|p| p.kind == ParamKind::Lifetime)
            .chain(all.iter().filter(|p| p.kind != ParamKind::Lifetime))
            .collect();
        if ordered.is_empty() {
            return;
        }
        s.text("<", origin);
        for (i, p) in ordered.iter().enumerate() {
            if i > 0 {
                s.text(", ", origin);
            }
            s.oneline(self.t, p.without_default);
        }
        s.text(">", origin);
    }

    /// `Nom<'a, T, N>`.
    fn self_ty(&self, c: &Class, s: &mut Synth, primary: bool) {
        let t = self.t;
        s.src(t.span(c.name), primary);
        if let Some(g) = &c.generics
            && !g.params.is_empty()
        {
            let origin = t.range_span(g.range);
            s.text("<", origin);
            for (i, p) in g.params.iter().enumerate() {
                if i > 0 {
                    s.text(", ", origin);
                }
                s.src(t.span(p.name), false);
            }
            s.text(">", origin);
        }
    }

    fn where_clause(&self, s: &mut Synth, preds: &[Option<TokRange>], origin: Span) {
        let t = self.t;
        let preds: Vec<TokRange> = preds
            .iter()
            .flatten()
            .map(|r| {
                if r.end > r.start && t.is_punct(r.end - 1, ',') {
                    TokRange::new(r.start, r.end - 1)
                } else {
                    *r
                }
            })
            .filter(|r| !r.is_empty())
            .collect();
        if preds.is_empty() {
            return;
        }
        s.text(" where ", origin);
        for (i, r) in preds.iter().enumerate() {
            if i > 0 {
                s.text(", ", origin);
            }
            s.oneline(t, *r);
        }
    }

    /// `impl<...> Nom<...> where ... {` (avec les attributs hérités de la classe).
    fn inherent_header(&self, c: &Class, s: &mut Synth) {
        let origin = self.t.span(c.name);
        self.inherited_attrs(c, s);
        s.text("impl", origin);
        self.impl_generics(s, c.generics.as_ref(), None, origin);
        s.text(" ", origin);
        self.self_ty(c, s, false);
        self.where_clause(s, &[c.where_preds], origin);
        s.text(" {", origin);
    }

    /// `[unsafe] impl<...> [!]Trait for Nom<...> where ... {`.
    fn trait_header(&self, c: &Class, ti: &TraitImpl, s: &mut Synth) {
        let t = self.t;
        let origin = t.range_span(TokRange::new(ti.header, ti.open));
        if ti.is_unsafe {
            s.src(t.span(ti.header), false).text(" ", origin);
        }
        s.text("impl", origin);
        self.impl_generics(s, c.generics.as_ref(), ti.generics.as_ref(), origin);
        s.text(" ", origin);
        if ti.negative {
            s.text("!", origin);
        }
        s.oneline(t, ti.trait_path);
        s.text(" for ", origin);
        self.self_ty(c, s, false);
        self.where_clause(s, &[c.where_preds, ti.where_preds], origin);
        s.text(" {", origin);
    }

    /// La `struct` générée : sur une seule ligne, ou lisible (`natural`), avec
    /// attributs et champs recopiés tels quels, commentaires de doc compris.
    fn struct_def(&self, c: &Class, s: &mut Synth, natural: bool) {
        let t = self.t;
        let origin = t.span(c.name);
        for attr in &c.attrs {
            if natural {
                s.src(t.range_span(attr.range), false);
                s.text("\n", origin);
            } else {
                s.oneline(t, attr.range);
                s.text(" ", origin);
            }
        }
        if let Some(v) = c.vis {
            s.oneline(t, v);
            s.text(" ", origin);
        }
        s.text("struct ", origin);
        s.src(t.span(c.name), true);
        if let Some(g) = &c.generics {
            s.oneline(t, g.range);
        }
        self.where_clause(s, &[c.where_preds], origin);
        s.text(" { ", origin);
        for o in &c.overs {
            let Some(name) = &o.field_name else { continue };
            let ty_span = t.range_span(o.ty);
            match (o.vis, c.vis) {
                (Some(v), _) | (None, Some(v)) => {
                    s.oneline(t, v);
                    s.text(" ", ty_span);
                }
                (None, None) => {}
            }
            match o.explicit_name {
                Some(n) => s.src(t.span(n), true),
                None => s.text(name.clone(), ty_span),
            };
            s.text(": ", ty_span);
            s.oneline(t, o.ty);
            s.text(", ", ty_span);
        }
        for m in &c.members {
            if let Member::Field(f) = m {
                if natural {
                    s.text("\n", t.span(f.name));
                    s.src(t.range_span(f.range), false);
                } else {
                    s.oneline(t, f.range);
                }
                s.text(", ", t.span(f.name));
            }
        }
        s.text("}", origin);
    }

    fn error_synth(&mut self, diag: &Diagnostic) -> Synth {
        self.diags.push(diag.clone());
        let mut s = Synth::default();
        s.text(diag.to_compile_error(), diag.span);
        s
    }

    // ------------------------------------------------------ disposition Lines

    fn class_lines(&mut self, c: &Class) {
        let t = self.t;
        let cspan = t.range_span(c.range);
        let header_start = t.span(c.header).start;
        self.e.blank(Span::new(cspan.start, header_start));

        let mut h = Synth::default();
        self.inherent_header(c, &mut h);
        for diag in &c.header_errors {
            h.text(" ", diag.span);
            let err = self.error_synth(diag);
            h.pieces.extend(err.pieces);
        }
        self.e.replace(Span::new(header_start, t.span(c.open).end), &h);

        let mut pos = t.span(c.open).end;
        for m in &c.members {
            let ms = t.range_span(m.range());
            match m {
                Member::Field(_) => {
                    self.e.copy(Span::new(pos, ms.start));
                    self.e.blank(ms);
                }
                Member::Item(_) => self.e.copy(Span::new(pos, ms.end)),
                Member::Error(_, diag) => {
                    self.e.copy(Span::new(pos, ms.start));
                    let s = self.error_synth(diag);
                    self.e.replace(ms, &s);
                }
                Member::TraitImpl(ti) => self.trait_impl_lines(c, ti, Span::new(pos, ms.start)),
            }
            pos = ms.end;
        }
        let close = t.span(c.close);
        self.e.copy(Span::new(pos, close.end));

        let mut s = Synth::default();
        s.text(" ", t.span(c.name));
        self.struct_def(c, &mut s, false);
        self.e.synth(&s);
    }

    fn trait_impl_lines(&mut self, c: &Class, ti: &TraitImpl, gap: Span) {
        let t = self.t;
        let origin = t.range_span(TokRange::new(ti.header, ti.open));
        // `} ` ferme l'`impl` inhérent ; on le loge dans l'indentation si possible
        // pour ne pas décaler la colonne de l'`impl` du trait.
        let gap_text = &t.src[gap.start..gap.end];
        let tail = gap_text.rsplit('\n').next().unwrap_or("");
        let fits = tail.len() >= 2 && tail.bytes().all(|b| b == b' ' || b == b'\t');
        let mut close = Synth::default();
        close.text("} ", origin);
        self.inherited_attrs(c, &mut close);
        if fits {
            self.e.copy(Span::new(gap.start, gap.end - 2));
        } else {
            self.e.copy(gap);
        }
        self.e.synth(&close);

        let header_start = t.span(ti.header).start;
        self.e.copy(Span::new(t.range_span(ti.range).start, header_start));
        let mut h = Synth::default();
        self.trait_header(c, ti, &mut h);
        self.e.replace(Span::new(header_start, t.span(ti.open).end), &h);
        self.e.copy(Span::new(t.span(ti.open).end, t.span(ti.close).end));

        let mut reopen = Synth::default();
        reopen.text(" ", origin);
        self.inherent_header(c, &mut reopen);
        self.e.synth(&reopen);
    }

    // ---------------------------------------------------- disposition Natural

    fn class_natural(&mut self, c: &Class) {
        let t = self.t;
        let mut s = Synth::default();
        self.struct_def(c, &mut s, true);
        s.text("\n", t.span(c.name));
        self.e.synth(&s);

        let has_inherent = !c.header_errors.is_empty()
            || c.members
                .iter()
                .any(|m| matches!(m, Member::Item(_) | Member::Error(..)));
        let mut prev_end = t.span(c.open).end;
        if has_inherent {
            let mut h = Synth::default();
            h.text("\n", t.span(c.name));
            self.inherent_header(c, &mut h);
            for diag in &c.header_errors {
                let err = self.error_synth(diag);
                h.text("\n", diag.span);
                h.pieces.extend(err.pieces);
            }
            self.e.synth(&h);
            for m in &c.members {
                let ms = t.range_span(m.range());
                match m {
                    Member::Item(_) => self.e.copy(Span::new(prev_end, ms.end)),
                    Member::Error(_, diag) => {
                        let mut s = self.error_synth(diag);
                        s.pieces.insert(0, Piece::Text("\n".into(), diag.span));
                        self.e.synth(&s);
                    }
                    _ => {}
                }
                prev_end = ms.end;
            }
            let mut end = Synth::default();
            end.text("\n}\n", t.span(c.close));
            self.e.synth(&end);
        }
        for m in &c.members {
            if let Member::TraitImpl(ti) = m {
                let mut h = Synth::default();
                h.text("\n", t.span(ti.open));
                self.inherited_attrs(c, &mut h);
                self.e.synth(&h);
                let header_start = t.span(ti.header).start;
                let attrs = Span::new(t.range_span(ti.range).start, header_start);
                self.e.copy(attrs);
                let mut h = Synth::default();
                self.trait_header(c, ti, &mut h);
                self.e.synth(&h);
                self.e.copy(Span::new(t.span(ti.open).end, t.span(ti.close).end));
                let mut nl = Synth::default();
                nl.text("\n", t.span(ti.close));
                self.e.synth(&nl);
            }
        }
    }

    /// Racine `.rsc` insérée par `include!`, où les attributs internes sont refusés :
    /// la doc `//!` devient un commentaire ordinaire, un `#![...]` est une erreur.
    fn inner_attr(&mut self, r: TokRange, span: Span) {
        let t = self.t;
        let text = &t.src[span.start..span.end];
        if t.doc_style(r.start).is_some() {
            // `//!` → `// ` et `/*!` → `/* ` : même longueur, mêmes colonnes.
            let mut s = Synth::default();
            s.text(format!("{} {}", &text[..2], &text[3..]), span);
            // Même texte à un caractère près, retours à la ligne compris : pas de `place`.
            self.e.synth(&s);
            return;
        }
        let diag = Diagnostic::new(
            Code::CR0015,
            span,
            format!("attribut interne `{}` interdit dans une racine `.rsc`", oneline_text(t, r)),
        )
        .help("placez les attributs de crate (`#![...]`) dans `src/main.rs` ou `src/lib.rs`, au-dessus de la ligne `include!`");
        let s = self.error_synth(&diag);
        self.place(span, &s);
    }

    // ------------------------------------------------------------- mod x;

    /// `mod x;` → module enveloppe dont le `#[path]` désigne le répertoire source,
    /// pour que `x` et ses sous-modules soient cherchés comme en Rust standard :
    ///
    /// `#[path = "<rép>"] mod __crust_mod_x { pub mod x; } use self::__crust_mod_x::x;`
    fn file_mod(&mut self, m: &FileMod, span: Span) {
        let t = self.t;
        let Some(source) = self.source else {
            self.e.copy(span);
            return;
        };
        let has_path = m.attrs.iter().any(|a| a.kind == AttrKind::Path);
        let mut dir = if has_path && m.inline.is_empty() {
            source.file.parent().unwrap_or(std::path::Path::new("/")).to_path_buf()
        } else {
            source.children_dir()
        };
        for seg in &m.inline {
            dir.push(seg.strip_prefix("r#").unwrap_or(seg));
        }
        let name = t.text(m.name);
        let wrapper = format!("__crust_mod_{}", name.strip_prefix("r#").unwrap_or(name));
        let mut s = Synth::default();
        for a in m
            .attrs
            .iter()
            .filter(|a| matches!(a.kind, AttrKind::Cfg | AttrKind::MacroUse))
        {
            s.oneline(t, a.range).text(" ", span);
        }
        s.text(
            format!("#[path = {:?}] mod {wrapper} {{ ", dir.display().to_string()),
            span,
        );
        for a in &m.attrs {
            s.oneline(t, a.range).text(" ", span);
        }
        s.text("pub mod ", span).src(t.span(m.name), true).text("; } ", span);
        for a in m.attrs.iter().filter(|a| a.kind == AttrKind::Cfg) {
            s.oneline(t, a.range).text(" ", span);
        }
        if let Some(v) = m.vis {
            s.oneline(t, v).text(" ", span);
        }
        s.text(format!("use self::{wrapper}::"), span)
            .src(t.span(m.name), false)
            .text(";", span);
        self.place(span, &s);
    }

    // ---------------------------------------------------------------- import

    fn import(&mut self, imp: &Import) {
        let t = self.t;
        let span = t.range_span(imp.range);
        let path_span = t.span(imp.path);
        let resolved = match self.resolver.resolve(&imp.path_value) {
            Ok(r) => r,
            Err(err) => {
                let diag = resolve_diag(err, &imp.path_value, path_span);
                let s = self.error_synth(&diag);
                self.place(span, &s);
                return;
            }
        };
        if let Some(exports) = &resolved.exports {
            let names: Vec<usize> = match &imp.list {
                ImportList::Single(n) => vec![*n],
                ImportList::Many(list) => list.iter().map(|(n, _)| *n).collect(),
                ImportList::Glob(_) => vec![],
            };
            for n in names {
                let name = t.text(n);
                if !exports.iter().any(|e| e == name) {
                    let mut diag = Diagnostic::new(
                        Code::CR0007,
                        t.span(n),
                        format!("`{name}` n'est pas défini dans `{}`", imp.path_value),
                    )
                    .label(path_span, "fichier importé");
                    if let Some(close) = closest(name, exports) {
                        diag = diag.help(format!("vouliez-vous dire `{close}` ?"));
                    }
                    let s = self.error_synth(&diag);
                    self.place(span, &s);
                    return;
                }
            }
        }

        let mut s = Synth::default();
        if !imp.attrs.is_empty() {
            s.oneline(t, imp.attrs);
            s.text(" ", span);
        }
        if let Some(v) = imp.vis {
            s.oneline(t, v);
            s.text(" ", span);
        }
        s.text("use ", t.span(imp.range.start));
        s.text(resolved.module_path, path_span);
        s.text("::", path_span);
        match &imp.list {
            ImportList::Single(n) => {
                s.src(t.span(*n), false);
            }
            ImportList::Glob(g) => {
                s.src(t.span(*g), false);
            }
            ImportList::Many(list) => {
                s.text("{", span);
                for (i, (n, alias)) in list.iter().enumerate() {
                    if i > 0 {
                        s.text(", ", span);
                    }
                    s.src(t.span(*n), false);
                    if let Some(a) = alias {
                        s.text(" as ", span);
                        s.src(t.span(*a), false);
                    }
                }
                s.text("}", span);
            }
        }
        s.text(";", span);
        self.place(span, &s);
    }
}

fn resolve_diag(err: ResolveError, path: &str, span: Span) -> Diagnostic {
    match err {
        ResolveError::NotFound { tried } => {
            Diagnostic::new(Code::CR0006, span, format!("fichier introuvable : `{path}`"))
                .help(format!("chemins essayés : {}", tried.join(", ")))
        }
        ResolveError::BadPrefix => Diagnostic::new(Code::CR0006, span, format!("chemin d'import invalide : `{path}`"))
            .help("le chemin doit commencer par `./`, `../` ou `@/` (relatif à `src/`)"),
        ResolveError::Ambiguous { rsc, rs } => Diagnostic::new(
            Code::CR0008,
            span,
            format!("import ambigu : `{rsc}` et `{rs}` existent tous les deux"),
        )
        .help("précisez l'extension `.rsc` ou `.rs`"),
        ResolveError::OutsideSrc => Diagnostic::new(
            Code::CR0013,
            span,
            format!("`{path}` est hors du répertoire `src/` de la crate"),
        ),
        ResolveError::InvalidName { component } => Diagnostic::new(
            Code::CR0012,
            span,
            format!("`{component}` ne donne pas un nom de module Rust valide"),
        )
        .help("les noms de répertoires et de fichiers doivent être des identifiants Rust (lettres, chiffres, `_`)"),
        ResolveError::MountedTwice { modules } => Diagnostic::new(
            Code::CR0011,
            span,
            format!("`{path}` est monté plusieurs fois ({})", modules.join(", ")),
        )
        .help("un fichier ne doit être déclaré que par un seul `mod` ; importez le module avec `use` ailleurs"),
        ResolveError::Unsupported => Diagnostic::new(
            Code::CR0006,
            span,
            format!("impossible de résoudre `{path}` dans ce contexte"),
        ),
    }
}

/// Nom le plus proche (distance d'édition ≤ 2), pour les suggestions.
fn closest<'e>(name: &str, candidates: &'e [String]) -> Option<&'e str> {
    fn dist(a: &str, b: &str) -> usize {
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, &cb) in b.iter().enumerate() {
                let cur = row[j + 1];
                row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
                prev = cur;
            }
        }
        row[b.len()]
    }
    candidates
        .iter()
        .map(|c| (dist(&name.to_lowercase(), &c.to_lowercase()), c))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c.as_str())
}

/// Noms définis au premier niveau d'un fichier, ou `None` si on ne peut pas
/// le savoir (macro au niveau module, `use ...::*`).
pub(crate) fn top_level_names(t: &Toks) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let end = t.len();
    let mut p = 0;
    while p < end {
        if let Some(next) = t.inner_attr_end(p, end) {
            p = next;
            continue;
        }
        while let Some(next) = t.outer_attr_end(p, end) {
            p = next;
        }
        if let Some(v) = t.vis_end(p, end) {
            p = v;
        }
        let start = p;
        // Qualificatifs : `unsafe`, `async`, `const fn`, `extern "C"`, `default`, `safe`.
        while matches!(t.text(p), "unsafe" | "async" | "default" | "safe" | "extern")
            || (t.is_ident(p, "const") && matches!(t.text(p + 1), "fn" | "unsafe" | "async" | "extern"))
            || (p > start && t.is_ident(p - 1, "extern") && t.kind(p) == Some(TokenKind::Literal))
        {
            p += 1;
        }
        let kw = t.text(p);
        match kw {
            "fn" | "struct" | "enum" | "trait" | "type" | "union" | "mod" | "const" | "class" if t.is_name(p + 1) => {
                names.push(t.text(p + 1).to_string());
            }
            "static" => {
                let n = if t.is_ident(p + 1, "mut") { p + 2 } else { p + 1 };
                names.push(t.text(n).to_string());
            }
            "macro_rules" if t.is_punct(p + 1, '!') => names.push(t.text(p + 2).to_string()),
            "use" => {
                let semi = (p..end).find(|&q| t.is_punct(q, ';')).unwrap_or(end);
                if (p..semi).any(|q| t.is_punct(q, '*') || t.is_punct(q, '{')) {
                    return None;
                }
                // `use a::B;` ou `use a::B as C;` : le nom importé est le dernier token.
                names.push(t.text(semi.saturating_sub(1)).to_string());
            }
            "import" => return None,
            _ if t.is_name(p) && t.is_punct(p + 1, '!') => return None,
            _ => {}
        }
        // Saute l'item (même logique que le parseur).
        let mut q = p;
        loop {
            if q >= end {
                p = end;
                break;
            }
            if t.is_open(q) {
                let close = t.group_end(q, end);
                if t.is_punct(q, '{') {
                    p = close + 1;
                    if t.is_punct(p, ';') {
                        p += 1;
                    }
                    break;
                }
                q = close + 1;
            } else if t.is_punct(q, ';') {
                p = q + 1;
                break;
            } else {
                q += 1;
            }
        }
    }
    Some(
        names
            .into_iter()
            .map(|n| n.strip_prefix("r#").map(str::to_string).unwrap_or(n))
            .collect(),
    )
}
