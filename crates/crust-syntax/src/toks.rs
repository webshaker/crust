//! Vue « significative » des tokens (sans espaces ni commentaires ordinaires),
//! avec appariement des délimiteurs.

use crate::Span;
use crate::lexer::{DocStyle, Token, TokenKind, tokenize};

/// Plage d'indices de tokens significatifs, fin exclue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokRange {
    pub start: usize,
    pub end: usize,
}

impl TokRange {
    pub fn new(start: usize, end: usize) -> Self {
        TokRange {
            start,
            end: end.max(start),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

pub struct Toks<'a> {
    pub src: &'a str,
    /// Tous les tokens, trivia comprises.
    pub all: Vec<Token>,
    /// Indices dans `all` des tokens significatifs.
    pub sig: Vec<usize>,
    /// Pour un délimiteur ouvrant (indice significatif) : son fermant.
    close: Vec<Option<usize>>,
}

impl<'a> Toks<'a> {
    pub fn new(src: &'a str) -> Self {
        let all = tokenize(src);
        let sig: Vec<usize> = (0..all.len()).filter(|&i| !all[i].kind.is_trivia()).collect();
        let mut close = vec![None; sig.len()];
        let mut stack: Vec<(usize, char)> = Vec::new();
        for (p, &i) in sig.iter().enumerate() {
            match all[i].kind {
                TokenKind::Punct(c @ ('(' | '[' | '{')) => stack.push((p, c)),
                TokenKind::Punct(c @ (')' | ']' | '}')) => {
                    let open = match c {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    // Fermant orphelin ou mal apparié : on remonte jusqu'à l'ouvrant
                    // correspondant s'il existe, sinon on l'ignore.
                    if let Some(depth) = stack.iter().rposition(|&(_, o)| o == open) {
                        let (op, _) = stack[depth];
                        stack.truncate(depth);
                        close[op] = Some(p);
                    }
                }
                _ => {}
            }
        }
        Toks { src, all, sig, close }
    }

    pub fn len(&self) -> usize {
        self.sig.len()
    }

    pub fn tok(&self, p: usize) -> Token {
        self.all[self.sig[p]]
    }

    pub fn kind(&self, p: usize) -> Option<TokenKind> {
        self.sig.get(p).map(|&i| self.all[i].kind)
    }

    pub fn text(&self, p: usize) -> &'a str {
        match self.sig.get(p) {
            Some(&i) => &self.src[self.all[i].start..self.all[i].end],
            None => "",
        }
    }

    pub fn span(&self, p: usize) -> Span {
        match self.sig.get(p) {
            Some(&i) => Span::new(self.all[i].start, self.all[i].end),
            None => Span::new(self.src.len(), self.src.len()),
        }
    }

    /// Plage source couverte par les tokens `[r.start, r.end)`.
    pub fn range_span(&self, r: TokRange) -> Span {
        if r.is_empty() {
            let at = self.span(r.start).start;
            return Span::new(at, at);
        }
        Span::new(self.span(r.start).start, self.span(r.end - 1).end)
    }

    pub fn is_punct(&self, p: usize, c: char) -> bool {
        self.kind(p) == Some(TokenKind::Punct(c))
    }

    pub fn is_ident(&self, p: usize, s: &str) -> bool {
        self.kind(p) == Some(TokenKind::Ident) && self.text(p) == s
    }

    /// Identifiant utilisable comme nom (identifiant brut ou non réservé).
    pub fn is_name(&self, p: usize) -> bool {
        match self.kind(p) {
            Some(TokenKind::RawIdent) => true,
            Some(TokenKind::Ident) => !crate::names::is_keyword(self.text(p)),
            _ => false,
        }
    }

    pub fn is_open(&self, p: usize) -> bool {
        matches!(self.kind(p), Some(TokenKind::Punct('(' | '[' | '{')))
    }

    /// Fermant du groupe ouvert en `p` ; pour un groupe non fermé, la fin de `limit`.
    pub fn group_end(&self, p: usize, limit: usize) -> usize {
        match self.close.get(p).copied().flatten() {
            Some(c) if c < limit => c,
            _ => limit.saturating_sub(1).max(p),
        }
    }

    pub fn is_closed_group(&self, p: usize, limit: usize) -> bool {
        matches!(self.close.get(p).copied().flatten(), Some(c) if c < limit)
    }

    /// Les tokens `p` et `p + 1` sont collés (`::`, `->`).
    pub fn joint(&self, p: usize) -> bool {
        p + 1 < self.len() && self.tok(p).end == self.tok(p + 1).start
    }

    /// `::` commençant en `p`.
    pub fn is_path_sep(&self, p: usize) -> bool {
        self.is_punct(p, ':') && self.is_punct(p + 1, ':') && self.joint(p)
    }

    /// Un saut de ligne sépare le token `p` du précédent.
    pub fn newline_before(&self, p: usize) -> bool {
        p > 0 && p < self.len() && self.src[self.tok(p - 1).end..self.tok(p).start].contains('\n')
    }

    pub fn doc_style(&self, p: usize) -> Option<DocStyle> {
        self.kind(p).and_then(TokenKind::doc_style)
    }

    /// Attribut externe `#[...]` en `p` : renvoie l'indice qui le suit.
    pub fn outer_attr_end(&self, p: usize, limit: usize) -> Option<usize> {
        if self.doc_style(p) == Some(DocStyle::Outer) {
            return Some(p + 1);
        }
        if self.is_punct(p, '#') && self.is_punct(p + 1, '[') && p + 1 < limit {
            return Some(self.group_end(p + 1, limit) + 1);
        }
        None
    }

    /// Attribut interne `#![...]` ou doc interne en `p` : renvoie l'indice qui le suit.
    pub fn inner_attr_end(&self, p: usize, limit: usize) -> Option<usize> {
        if self.doc_style(p) == Some(DocStyle::Inner) {
            return Some(p + 1);
        }
        if self.is_punct(p, '#') && self.is_punct(p + 1, '!') && self.is_punct(p + 2, '[') && p + 2 < limit {
            return Some(self.group_end(p + 2, limit) + 1);
        }
        None
    }

    /// Visibilité `pub` / `pub(...)` en `p` : renvoie l'indice qui la suit.
    pub fn vis_end(&self, p: usize, limit: usize) -> Option<usize> {
        if !self.is_ident(p, "pub") {
            return None;
        }
        if self.is_punct(p + 1, '(') && p + 1 < limit {
            return Some(self.group_end(p + 1, limit) + 1);
        }
        Some(p + 1)
    }
}
