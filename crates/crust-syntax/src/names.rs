//! Noms : mots-clés Rust, conversion snake_case, validité des identifiants.

/// Mots-clés stricts et réservés (toutes éditions confondues).
const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
    "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box",
    "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// Identifiant Rust valide et non réservé (les identifiants bruts `r#x` sont acceptés).
pub fn is_valid_ident(s: &str) -> bool {
    let bare = s.strip_prefix("r#").unwrap_or(s);
    let mut chars = bare.chars();
    let Some(first) = chars.next() else { return false };
    crate::lexer::is_ident_start(first)
        && chars.all(crate::lexer::is_ident_continue)
        && bare != "_"
        && (s.starts_with("r#") || !is_keyword(bare))
}

/// `MoteurBrushless` → `moteur_brushless`, `HTTPServer` → `http_server`, `Uart2` → `uart2`.
///
/// Le résultat est toujours accepté par le lint `non_snake_case` de rustc.
pub fn to_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            if !out.ends_with('_') || out.is_empty() {
                out.push('_');
            }
            continue;
        }
        if c.is_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let boundary = prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_lower);
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Nom sûr à émettre comme identifiant : ajoute `r#` si c'est un mot-clé.
pub fn ident_for(name: &str) -> String {
    if is_keyword(name) && !matches!(name, "self" | "Self" | "super" | "crate") {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case() {
        for (input, expected) in [
            ("Moteur", "moteur"),
            ("MoteurBrushless", "moteur_brushless"),
            ("RobotArm", "robot_arm"),
            ("HTTPServer", "http_server"),
            ("Uart2", "uart2"),
            ("Vec2D", "vec2_d"),
            ("already_snake", "already_snake"),
            ("_Private", "_private"),
            ("r#Type", "type"),
            ("ÉtatMoteur", "état_moteur"),
        ] {
            assert_eq!(to_snake_case(input), expected, "{input}");
        }
    }

    #[test]
    fn idents() {
        assert!(is_valid_ident("robot"));
        assert!(is_valid_ident("r#type"));
        assert!(!is_valid_ident("type"));
        assert!(!is_valid_ident("mon-dossier"));
        assert!(!is_valid_ident("2d"));
        assert_eq!(ident_for("type"), "r#type");
        assert_eq!(ident_for("moteur"), "moteur");
    }
}
