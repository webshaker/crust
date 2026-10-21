//! Correspondance positions générées ↔ positions `.rsc` (section 18.1).

use crate::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    /// Octets recopiés à l'identique : correspondance 1:1.
    Copied,
    /// Texte généré : toute la plage renvoie à `src`.
    Synthesized,
    /// Membre effacé (champ, attributs) : renvoie à `src`, jamais utilisé en sens inverse.
    Blank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub gen_start: usize,
    pub gen_len: usize,
    pub src_start: usize,
    pub src_len: usize,
    pub kind: SegmentKind,
    /// Cible préférée quand une même position source a plusieurs copies
    /// (le nom de la classe est recopié dans la `struct` et dans chaque `impl`).
    pub primary: bool,
}

impl Segment {
    fn gen_end(&self) -> usize {
        self.gen_start + self.gen_len
    }

    fn src_end(&self) -> usize {
        self.src_start + self.src_len
    }
}

/// Segments triés par `gen_start`, contigus, couvrant tout le fichier généré.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub segments: Vec<Segment>,
}

/// Résultat d'une conversion générée → source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapped {
    pub span: Span,
    /// `true` si la plage est recopiée à l'identique (correspondance exacte).
    pub exact: bool,
}

impl SourceMap {
    pub(crate) fn push(&mut self, seg: Segment) {
        if seg.gen_len == 0 && seg.src_len == 0 {
            return;
        }
        if let Some(last) = self.segments.last_mut() {
            debug_assert_eq!(last.gen_end(), seg.gen_start, "segments contigus");
            // Fusionne les copies contiguës pour garder la table compacte.
            if last.kind == SegmentKind::Copied
                && seg.kind == SegmentKind::Copied
                && last.primary == seg.primary
                && last.src_end() == seg.src_start
            {
                last.gen_len += seg.gen_len;
                last.src_len += seg.src_len;
                return;
            }
        }
        self.segments.push(seg);
    }

    fn segment_at_gen(&self, offset: usize) -> Option<&Segment> {
        let i = self.segments.partition_point(|s| s.gen_end() <= offset);
        self.segments.get(i).or_else(|| self.segments.last())
    }

    /// Position générée → position `.rsc`.
    pub fn gen_to_src(&self, offset: usize) -> Option<Mapped> {
        let seg = self.segment_at_gen(offset)?;
        Some(match seg.kind {
            SegmentKind::Copied => {
                let at = seg.src_start + (offset.min(seg.gen_end()) - seg.gen_start);
                Mapped {
                    span: Span::new(at, at),
                    exact: true,
                }
            }
            _ => Mapped {
                span: Span::new(seg.src_start, seg.src_end()),
                exact: false,
            },
        })
    }

    /// Plage générée → plage `.rsc`. Exacte si les deux bornes tombent dans des copies.
    pub fn gen_span_to_src(&self, span: Span) -> Option<Mapped> {
        let start = self.gen_to_src(span.start)?;
        let end = if span.end > span.start {
            let m = self.gen_to_src(span.end - 1)?;
            if m.exact {
                Mapped {
                    span: Span::new(m.span.start + 1, m.span.start + 1),
                    exact: true,
                }
            } else {
                m
            }
        } else {
            start
        };
        let exact = start.exact && end.exact;
        let (s, e) = if exact {
            (start.span.start, end.span.end)
        } else {
            (start.span.start.min(end.span.start), start.span.end.max(end.span.end))
        };
        Some(Mapped {
            span: Span::new(s, e.max(s)),
            exact,
        })
    }

    /// Position `.rsc` → position générée (pour les requêtes de l'éditeur).
    pub fn src_to_gen(&self, offset: usize) -> Option<usize> {
        let mut best: Option<&Segment> = None;
        for seg in &self.segments {
            let covers = seg.src_start <= offset && offset < seg.src_end().max(seg.src_start + 1);
            if seg.kind == SegmentKind::Copied && covers {
                if seg.primary {
                    best = Some(seg);
                    break;
                }
                best.get_or_insert(seg);
            }
        }
        best.map(|s| s.gen_start + (offset - s.src_start))
    }
}

/// Conversion octets ↔ (ligne, colonne) ; colonnes en caractères et en unités UTF-16.
pub struct LineIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    /// Ligne, à partir de 0.
    pub line: usize,
    /// Colonne en caractères Unicode, à partir de 0 (convention de rustc).
    pub col_char: usize,
    /// Colonne en unités UTF-16, à partir de 0 (convention LSP).
    pub col_utf16: usize,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text, line_starts }
    }

    pub fn line_col(&self, offset: usize) -> LineCol {
        let offset = offset.min(self.text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let prefix = &self.text[self.line_starts[line]..offset];
        LineCol {
            line,
            col_char: prefix.chars().count(),
            col_utf16: prefix.chars().map(char::len_utf16).sum(),
        }
    }

    /// (ligne, colonne en caractères) → octet.
    pub fn offset(&self, line: usize, col_char: usize) -> Option<usize> {
        let start = *self.line_starts.get(line)?;
        let end = self.line_starts.get(line + 1).copied().unwrap_or(self.text.len());
        let line_text = &self.text[start..end];
        Some(match line_text.char_indices().nth(col_char) {
            Some((i, _)) => start + i,
            None => end,
        })
    }
}
