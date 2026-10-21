//! Rendu d'un diagnostic rustc (JSON, après remappage) au format habituel de rustc.

use std::collections::HashMap;
use std::ops::Range;

use annotate_snippets::{Level, Message, Renderer, Snippet};
use serde_json::Value;

fn level(name: &str) -> Level {
    match name {
        "warning" => Level::Warning,
        "note" | "failure-note" => Level::Note,
        "help" => Level::Help,
        _ => Level::Error,
    }
}

struct Ann {
    range: Range<usize>,
    label: String,
    primary: bool,
}

struct Snip {
    origin: String,
    source: String,
    anns: Vec<Ann>,
}

/// Données possédées d'un message (annotate-snippets ne fait qu'emprunter).
struct Owned {
    level: String,
    title: String,
    code: Option<String>,
    snippets: Vec<Snip>,
    footers: Vec<(String, String)>,
}

fn collect(msg: &Value, sources: &mut HashMap<String, Option<String>>) -> (Owned, Vec<Owned>) {
    let mut snippets: Vec<Snip> = Vec::new();
    for span in msg["spans"].as_array().into_iter().flatten() {
        let Some(file) = span["file_name"].as_str() else {
            continue;
        };
        let Some(source) = sources
            .entry(file.to_string())
            .or_insert_with(|| std::fs::read_to_string(file).ok())
            .clone()
        else {
            continue;
        };
        let start = (span["byte_start"].as_u64().unwrap_or(0) as usize).min(source.len());
        let mut end = (span["byte_end"].as_u64().unwrap_or(0) as usize).clamp(start, source.len());
        if end == start {
            // Span vide : on souligne le caractère suivant, comme rustc.
            end = source[start..].chars().next().map_or(start, |c| start + c.len_utf8());
        }
        let ann = Ann {
            range: start..end,
            label: span["label"].as_str().unwrap_or_default().to_string(),
            primary: span["is_primary"].as_bool().unwrap_or(false),
        };
        match snippets.iter_mut().find(|s| s.origin == file) {
            Some(s) => s.anns.push(ann),
            None => snippets.push(Snip {
                origin: file.to_string(),
                source,
                anns: vec![ann],
            }),
        }
    }

    let mut footers = Vec::new();
    let mut subs = Vec::new();
    for child in msg["children"].as_array().into_iter().flatten() {
        let child_level = child["level"].as_str().unwrap_or("note").to_string();
        let mut text = child["message"].as_str().unwrap_or_default().to_string();
        let suggestions: Vec<&str> = child["spans"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["suggested_replacement"].as_str())
            .collect();
        if !suggestions.is_empty() {
            let shown: Vec<String> = suggestions.iter().map(|s| format!("`{}`", s.trim())).collect();
            text = format!("{text} : {}", shown.join(", "));
        }
        let has_spans = child["spans"].as_array().is_some_and(|s| !s.is_empty());
        if has_spans && suggestions.is_empty() {
            let (mut owned, _) = collect(child, sources);
            owned.level = child_level;
            owned.title = text;
            subs.push(owned);
        } else {
            footers.push((child_level, text));
        }
    }

    let owned = Owned {
        level: msg["level"].as_str().unwrap_or("error").to_string(),
        title: msg["message"].as_str().unwrap_or_default().to_string(),
        code: msg["code"]["code"].as_str().map(str::to_string),
        snippets,
        footers,
    };
    (owned, subs)
}

fn message(o: &Owned) -> Message<'_> {
    let lvl = level(&o.level);
    let mut m = lvl.title(&o.title);
    if let Some(code) = &o.code {
        m = m.id(code);
    }
    for s in &o.snippets {
        let mut snippet = Snippet::source(&s.source).line_start(1).origin(&s.origin).fold(true);
        for a in &s.anns {
            // annotate-snippets 0.11 ne souligne en `^` que le niveau `Error` :
            // tout span principal l'utilise, comme rustc (seule la couleur diffère).
            let ann_level = if a.primary { Level::Error } else { Level::Info };
            snippet = snippet.annotation(ann_level.span(a.range.clone()).label(&a.label));
        }
        m = m.snippet(snippet);
    }
    for (l, text) in &o.footers {
        m = m.footer(level(l).title(text));
    }
    m
}

/// Texte complet du diagnostic, prêt à afficher sur stderr (une ligne vide
/// après chaque message, comme rustc).
pub fn render(msg: &Value, color: bool) -> String {
    let mut sources = HashMap::new();
    let (main, subs) = collect(msg, &mut sources);
    let renderer = if color { Renderer::styled() } else { Renderer::plain() };
    let mut out = String::new();
    for m in std::iter::once(&main).chain(&subs) {
        for line in renderer.render(message(m)).to_string().lines() {
            out += &strip_label_prefix(line);
            out.push('\n');
        }
    }
    out.push('\n');
    out
}

/// annotate-snippets 0.11 préfixe parfois une étiquette de son niveau (`info: `) ;
/// rustc ne le fait pas. On retire ce préfixe sur les lignes d'annotation : après
/// la barre `|` de la marge, des espaces et des marques de soulignement.
fn strip_label_prefix(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut seen_bar = false;
    while i < bytes.len() {
        match bytes[i] {
            // Séquence ANSI `ESC [ ... m`.
            0x1b => {
                while i < bytes.len() && bytes[i] != b'm' {
                    i += 1;
                }
                i += 1;
            }
            b'|' => {
                seen_bar = true;
                i += 1;
            }
            b' ' => i += 1,
            b'-' | b'^' | b'_' | b'/' | b'\\' if seen_bar => i += 1,
            _ => break,
        }
    }
    if seen_bar {
        for prefix in ["info: ", "note: ", "help: ", "error: ", "warning: "] {
            if line[i..].starts_with(prefix) {
                return format!("{}{}", &line[..i], &line[i + prefix.len()..]);
            }
        }
    }
    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::strip_label_prefix;

    #[test]
    fn prefixe_retire_seulement_dans_la_marge() {
        assert_eq!(
            strip_label_prefix("   |         info: attendu ici"),
            "   |         attendu ici"
        );
        assert_eq!(
            strip_label_prefix("   |   -----  info: emprunt ici"),
            "   |   -----  emprunt ici"
        );
        assert_eq!(
            strip_label_prefix("error[E0308]: mismatched types"),
            "error[E0308]: mismatched types"
        );
        assert_eq!(
            strip_label_prefix("14 |         *v = \"info: x\";"),
            "14 |         *v = \"info: x\";"
        );
    }
}
