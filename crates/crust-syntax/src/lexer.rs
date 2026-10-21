//! Lexer Rust sans perte : la concaténation des tokens redonne exactement la source.
//!
//! Il ne cherche pas à valider la syntaxe : il doit seulement découper la source
//! sans jamais se désynchroniser (commentaires imbriqués, chaînes brutes,
//! lifetimes / caractères), car le parseur léger s'appuie sur les délimiteurs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocStyle {
    Outer,
    Inner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Whitespace,
    LineComment {
        doc: Option<DocStyle>,
    },
    BlockComment {
        doc: Option<DocStyle>,
        terminated: bool,
    },
    Ident,
    RawIdent,
    Lifetime,
    Literal,
    /// Un seul caractère de ponctuation (`::` donne deux tokens `:` collés).
    Punct(char),
    Shebang,
    Unknown,
}

impl TokenKind {
    /// Espaces et commentaires ordinaires : ignorés par le parseur.
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::Shebang
                | TokenKind::LineComment { doc: None }
                | TokenKind::BlockComment { doc: None, .. }
        )
    }

    pub fn doc_style(self) -> Option<DocStyle> {
        match self {
            TokenKind::LineComment { doc } | TokenKind::BlockComment { doc, .. } => doc,
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

pub fn tokenize(src: &str) -> Vec<Token> {
    let mut lx = Lexer {
        src,
        pos: 0,
        out: Vec::new(),
    };
    lx.run();
    lx.out
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    out: Vec<Token>,
}

pub fn is_whitespace(c: char) -> bool {
    // Pattern_White_Space, comme rustc.
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{85}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}'
    )
}

pub fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

pub fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

impl Lexer<'_> {
    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn peek(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.rest().chars().next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn bump_while(&mut self, f: impl Fn(char) -> bool) {
        while let Some(c) = self.peek(0) {
            if !f(c) {
                break;
            }
            self.bump();
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.out.push(Token {
            kind,
            start,
            end: self.pos,
        });
    }

    fn run(&mut self) {
        self.shebang();
        while self.pos < self.src.len() {
            let start = self.pos;
            let kind = self.token();
            debug_assert!(self.pos > start, "le lexer doit toujours avancer");
            self.push(kind, start);
        }
    }

    /// `#!` en tête de fichier, sauf s'il s'agit d'un attribut interne `#![...]`.
    fn shebang(&mut self) {
        let Some(after) = self.src.strip_prefix("#!") else {
            return;
        };
        let next = after.trim_start_matches(|c: char| is_whitespace(c)).chars().next();
        if next == Some('[') {
            return;
        }
        let end = self.src.find('\n').unwrap_or(self.src.len());
        self.pos = end;
        self.push(TokenKind::Shebang, 0);
    }

    fn token(&mut self) -> TokenKind {
        let c = self.bump().expect("token() appelé en fin de source");
        match c {
            c if is_whitespace(c) => {
                self.bump_while(is_whitespace);
                TokenKind::Whitespace
            }
            '/' if self.peek(0) == Some('/') => self.line_comment(),
            '/' if self.peek(0) == Some('*') => self.block_comment(),
            'r' if self.peek(0) == Some('#') && self.peek(1).is_some_and(is_ident_start) => {
                self.bump();
                self.bump_while(is_ident_continue);
                TokenKind::RawIdent
            }
            'r' if matches!(self.peek(0), Some('"' | '#')) => self.raw_string_or_ident(),
            'b' | 'c' if self.peek(0) == Some('r') && matches!(self.peek(1), Some('"' | '#')) => {
                self.bump();
                self.raw_string_or_ident()
            }
            'b' if self.peek(0) == Some('\'') => {
                self.bump();
                self.quoted('\'');
                self.suffix();
                TokenKind::Literal
            }
            'b' | 'c' if self.peek(0) == Some('"') => {
                self.bump();
                self.quoted('"');
                self.suffix();
                TokenKind::Literal
            }
            c if is_ident_start(c) => {
                self.bump_while(is_ident_continue);
                TokenKind::Ident
            }
            '0'..='9' => self.number(c),
            '"' => {
                self.quoted('"');
                self.suffix();
                TokenKind::Literal
            }
            '\'' => self.char_or_lifetime(),
            c if c.is_ascii_punctuation() => TokenKind::Punct(c),
            _ => TokenKind::Unknown,
        }
    }

    fn line_comment(&mut self) -> TokenKind {
        self.bump(); // second '/'
        let doc = match (self.peek(0), self.peek(1)) {
            (Some('/'), Some('/')) => None,
            (Some('/'), _) => Some(DocStyle::Outer),
            (Some('!'), _) => Some(DocStyle::Inner),
            _ => None,
        };
        self.bump_while(|c| c != '\n');
        TokenKind::LineComment { doc }
    }

    fn block_comment(&mut self) -> TokenKind {
        self.bump(); // '*'
        let doc = match (self.peek(0), self.peek(1)) {
            (Some('*'), Some('*' | '/')) => None,
            (Some('*'), _) => Some(DocStyle::Outer),
            (Some('!'), _) => Some(DocStyle::Inner),
            _ => None,
        };
        let mut depth = 1usize;
        while let Some(c) = self.bump() {
            match c {
                '/' if self.peek(0) == Some('*') => {
                    self.bump();
                    depth += 1;
                }
                '*' if self.peek(0) == Some('/') => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        return TokenKind::BlockComment { doc, terminated: true };
                    }
                }
                _ => {}
            }
        }
        TokenKind::BlockComment { doc, terminated: false }
    }

    /// Après `r`, `br` ou `cr` : chaîne brute `r#"..."#`, ou simple identifiant.
    fn raw_string_or_ident(&mut self) -> TokenKind {
        let save = self.pos;
        let mut hashes = 0usize;
        while self.peek(0) == Some('#') {
            self.bump();
            hashes += 1;
        }
        if self.peek(0) != Some('"') {
            // `r#` non suivi d'un identifiant ni d'une chaîne : on rend `r` seul.
            self.pos = save;
            self.bump_while(is_ident_continue);
            return TokenKind::Ident;
        }
        self.bump();
        'outer: while let Some(c) = self.bump() {
            if c == '"' {
                for i in 0..hashes {
                    if self.peek(i) != Some('#') {
                        continue 'outer;
                    }
                }
                for _ in 0..hashes {
                    self.bump();
                }
                break;
            }
        }
        self.suffix();
        TokenKind::Literal
    }

    fn quoted(&mut self, quote: char) {
        while let Some(c) = self.bump() {
            match c {
                '\\' => {
                    self.bump();
                }
                c if c == quote => return,
                _ => {}
            }
        }
    }

    fn suffix(&mut self) {
        if self.peek(0).is_some_and(is_ident_start) {
            self.bump_while(is_ident_continue);
        }
    }

    fn number(&mut self, first: char) -> TokenKind {
        let radix_prefixed = first == '0' && matches!(self.peek(0), Some('x' | 'o' | 'b'));
        self.digits(radix_prefixed);
        // Partie décimale : `1.5`, mais ni `1..2` ni `1.foo()`.
        if !radix_prefixed && self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            self.digits(false);
        }
        TokenKind::Literal
    }

    /// Chiffres, `_`, suffixe, et signe d'exposant décimal (`1e-3`).
    fn digits(&mut self, radix_prefixed: bool) {
        while let Some(c) = self.peek(0) {
            if is_ident_continue(c) {
                self.bump();
                if !radix_prefixed
                    && matches!(c, 'e' | 'E')
                    && matches!(self.peek(0), Some('+' | '-'))
                    && self.peek(1).is_some_and(|d| d.is_ascii_digit())
                {
                    self.bump();
                }
            } else {
                break;
            }
        }
    }

    /// `'a'` / `'\n'` sont des caractères ; `'a` / `'static` des lifetimes.
    fn char_or_lifetime(&mut self) -> TokenKind {
        match (self.peek(0), self.peek(1)) {
            (Some('\\'), _) => {
                self.quoted('\'');
                self.suffix();
                TokenKind::Literal
            }
            (Some(_), Some('\'')) => {
                self.bump();
                self.bump();
                self.suffix();
                TokenKind::Literal
            }
            (Some(c), _) if is_ident_start(c) || c == 'r' => {
                if c == 'r' && self.peek(1) == Some('#') {
                    self.bump();
                    self.bump();
                }
                self.bump_while(is_ident_continue);
                if self.peek(0) == Some('\'') {
                    // `'abc'` : caractère invalide, on le garde en un seul token.
                    self.bump();
                    TokenKind::Literal
                } else {
                    TokenKind::Lifetime
                }
            }
            _ => TokenKind::Punct('\''),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(TokenKind, &str)> {
        tokenize(src)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .map(|t| (t.kind, &src[t.start..t.end]))
            .collect()
    }

    #[test]
    fn lossless() {
        let src = "fn main() { let s = r#\"a \" b\"#; /* /* imbriqué */ */ x.0.1 }";
        let toks = tokenize(src);
        let joined: String = toks.iter().map(|t| &src[t.start..t.end]).collect();
        assert_eq!(joined, src);
    }

    #[test]
    fn lifetimes_et_caracteres() {
        use TokenKind::*;
        assert_eq!(
            kinds("'a 'b' '\\n' 'static '\\'' b'x' 'r#loop"),
            vec![
                (Lifetime, "'a"),
                (Literal, "'b'"),
                (Literal, "'\\n'"),
                (Lifetime, "'static"),
                (Literal, "'\\''"),
                (Literal, "b'x'"),
                (Lifetime, "'r#loop"),
            ]
        );
    }

    #[test]
    fn chaines() {
        use TokenKind::*;
        assert_eq!(
            kinds(r####"r"a" r##"x"#y"## br"z" b"\"" c"c" "s"suf r#match"####),
            vec![
                (Literal, r#"r"a""#),
                (Literal, r###"r##"x"#y"##"###),
                (Literal, r#"br"z""#),
                (Literal, r#"b"\"""#),
                (Literal, r#"c"c""#),
                (Literal, r#""s"suf"#),
                (RawIdent, "r#match"),
            ]
        );
    }

    #[test]
    fn commentaires() {
        use TokenKind::*;
        assert_eq!(
            kinds("/// doc\n//! inner\n//// pas doc\n/** d */ /*! i */ /**/ /*** non */"),
            vec![
                (
                    LineComment {
                        doc: Some(DocStyle::Outer)
                    },
                    "/// doc"
                ),
                (
                    LineComment {
                        doc: Some(DocStyle::Inner)
                    },
                    "//! inner"
                ),
                (LineComment { doc: None }, "//// pas doc"),
                (
                    BlockComment {
                        doc: Some(DocStyle::Outer),
                        terminated: true
                    },
                    "/** d */"
                ),
                (
                    BlockComment {
                        doc: Some(DocStyle::Inner),
                        terminated: true
                    },
                    "/*! i */"
                ),
                (
                    BlockComment {
                        doc: None,
                        terminated: true
                    },
                    "/**/"
                ),
                (
                    BlockComment {
                        doc: None,
                        terminated: true
                    },
                    "/*** non */"
                ),
            ]
        );
    }

    #[test]
    fn nombres() {
        use TokenKind::*;
        assert_eq!(
            kinds("1..2 1.5e-3 0x1E+5 x.0"),
            vec![
                (Literal, "1"),
                (Punct('.'), "."),
                (Punct('.'), "."),
                (Literal, "2"),
                (Literal, "1.5e-3"),
                (Literal, "0x1E"),
                (Punct('+'), "+"),
                (Literal, "5"),
                (Ident, "x"),
                (Punct('.'), "."),
                (Literal, "0"),
            ]
        );
    }

    #[test]
    fn shebang() {
        assert_eq!(tokenize("#!/usr/bin/env run\nfn")[0].kind, TokenKind::Shebang);
        assert_eq!(tokenize("#![allow(x)]")[0].kind, TokenKind::Punct('#'));
    }
}
