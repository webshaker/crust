//! Formatage des `.rsc` au style rustfmt (section 21).
//!
//! rustfmt ne connaît pas `class` : on lui donne un seul fichier composé
//! - du fichier lui-même, où chaque classe et chaque `import` est remplacé par
//!   un marqueur (`const __CRUST_FMT_<i>: () = ();`) ;
//! - de morceaux séparés par des commentaires `// __CRUST_CHUNK_<k>` : attributs,
//!   champs, méthodes et corps d'`impl Trait`, chacun dans un `struct` ou un
//!   `impl` temporaire, et les en-têtes sous forme de `struct` / `impl` vides.
//!
//! On recompose ensuite les classes à partir de ces morceaux formatés.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::Span;
use crate::emit::oneline_text;
use crate::parser::{self, Class, Import, ImportList, Item, Member, TraitImpl};
use crate::toks::Toks;

const ATTRS_END: &str = "struct __CrustAttrs;";

#[derive(Default)]
struct Batch {
    chunks: Vec<String>,
}

impl Batch {
    fn add(&mut self, chunk: String) -> usize {
        self.chunks.push(chunk);
        self.chunks.len() - 1
    }
}

/// Lignes formatées d'un morceau, entre son ouverture et sa fermeture.
struct Chunks(Vec<Vec<String>>);

impl Chunks {
    /// Attributs : lignes avant `struct __CrustAttrs;`.
    fn attrs(&self, k: usize) -> Vec<String> {
        self.0[k]
            .iter()
            .take_while(|l| l.trim() != ATTRS_END)
            .cloned()
            .collect()
    }

    /// Contenu d'un `struct X { … }` ou `impl X { … }` : lignes entre les accolades.
    fn body(&self, k: usize) -> Vec<String> {
        let lines = &self.0[k];
        let Some(open) = lines.iter().position(|l| l.trim_end().ends_with('{')) else {
            return Vec::new();
        };
        let close = lines.iter().rposition(|l| l.trim() == "}").unwrap_or(lines.len());
        lines[open + 1..close.max(open + 1)].to_vec()
    }

    fn all(&self, k: usize) -> &[String] {
        &self.0[k]
    }
}

fn rustfmt(input: &str, edition: &str, dir: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new("rustfmt");
    cmd.args(["--edition", edition, "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn().map_err(|e| format!("rustfmt introuvable : {e}"))?;
    child
        .stdin
        .take()
        .ok_or("stdin de rustfmt")?
        .write_all(input.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

/// Formate un fichier `.rsc`. `dir` : répertoire où chercher `rustfmt.toml`.
///
/// Refuse un fichier qui contient des erreurs Crust : le formatage ne doit
/// jamais changer le sens du code.
pub fn format(src: &str, edition: &str, dir: Option<&Path>) -> Result<String, String> {
    let t = Toks::new(src);
    let items: Vec<Item> = parser::parse(&t, false)
        .into_iter()
        .filter(|i| !matches!(i, Item::FileMod(_)))
        .collect();
    for item in &items {
        match item {
            Item::Broken(_, d) => return Err(format!("[{}] {}", d.code.as_str(), d.message)),
            Item::Class(c) => {
                if let Some(d) = c.header_errors.first() {
                    return Err(format!("[{}] {}", d.code.as_str(), d.message));
                }
                for m in &c.members {
                    if let Member::Error(_, d) = m {
                        return Err(format!("[{}] {}", d.code.as_str(), d.message));
                    }
                }
            }
            _ => {}
        }
    }

    let mut skeleton = String::new();
    let mut pos = 0;
    for (i, item) in items.iter().enumerate() {
        let span = t.range_span(item.range());
        skeleton.push_str(&src[pos..span.start]);
        skeleton.push_str(&format!("const __CRUST_FMT_{i}: () = ();"));
        pos = span.end;
    }
    skeleton.push_str(&src[pos..]);

    let mut batch = Batch::default();
    let plans: Vec<Plan> = items.iter().map(|item| plan(&t, item, &mut batch)).collect();

    let mut input = skeleton;
    if !input.ends_with('\n') {
        input.push('\n');
    }
    for (k, chunk) in batch.chunks.iter().enumerate() {
        input.push_str(&format!("\n// __CRUST_CHUNK_{k}\n{chunk}\n"));
    }
    let output = rustfmt(&input, edition, dir)?;

    // Découpage de la sortie : squelette, puis morceaux.
    let mut parts: Vec<Vec<String>> = vec![Vec::new()];
    for line in output.lines() {
        if line.trim().starts_with("// __CRUST_CHUNK_") {
            parts.push(Vec::new());
        } else {
            parts.last_mut().unwrap().push(line.to_string());
        }
    }
    let skeleton_lines = parts.remove(0);
    let chunks = Chunks(parts);
    if chunks.0.len() != batch.chunks.len() {
        return Err("formatage interrompu : rustfmt a déplacé un marqueur".into());
    }

    let mut out = String::new();
    for line in trim_trailing_blank(&skeleton_lines) {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        match trimmed
            .strip_prefix("const __CRUST_FMT_")
            .and_then(|r| r.split(':').next())
            .and_then(|n| n.parse::<usize>().ok())
        {
            Some(i) => {
                for l in render(&plans[i], &chunks) {
                    if l.is_empty() {
                        out.push('\n');
                    } else {
                        out.push_str(&format!("{indent}{l}\n"));
                    }
                }
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

fn trim_trailing_blank(lines: &[String]) -> &[String] {
    let end = lines.iter().rposition(|l| !l.trim().is_empty()).map_or(0, |i| i + 1);
    &lines[..end]
}

// ------------------------------------------------------------------ plans

enum Plan {
    Class(ClassPlan),
    Import { attrs: Option<usize>, line: String },
}

struct OverPlan {
    vis: String,
    ty: usize,
    name: Option<String>,
}

struct ClassPlan {
    attrs: Option<usize>,
    header: usize,
    overs: Vec<OverPlan>,
    members: Vec<MemberPlan>,
    /// Commentaires après le dernier membre.
    tail: Vec<Comment>,
}

struct MemberPlan {
    blank_before: bool,
    comments: Vec<Comment>,
    /// Ligne vide entre les commentaires et le membre.
    blank_after_comments: bool,
    kind: MemberKind,
    /// Commentaire sur la même ligne, après le membre.
    trailing: Option<String>,
}

enum MemberKind {
    Verbatim(String),
    Field(usize),
    Item(usize),
    TraitImpl {
        attrs: Option<usize>,
        header: usize,
        body: usize,
    },
}

struct Comment {
    text: String,
    blank_before: bool,
}

fn text<'a>(t: &Toks<'a>, span: Span) -> &'a str {
    &t.src[span.start..span.end]
}

fn attrs_chunk(t: &Toks, start: usize, end: usize, batch: &mut Batch) -> Option<usize> {
    let s = t.src[start..end].trim();
    if s.is_empty() {
        return None;
    }
    Some(batch.add(format!("{s}\n{ATTRS_END}")))
}

fn plan(t: &Toks, item: &Item, batch: &mut Batch) -> Plan {
    match item {
        Item::Class(c) => Plan::Class(plan_class(t, c, batch)),
        Item::Import(i) => plan_import(t, i, batch),
        Item::Broken(..) | Item::FileMod(_) | Item::InnerAttr(_) => unreachable!("filtré"),
    }
}

fn plan_import(t: &Toks, imp: &Import, batch: &mut Batch) -> Plan {
    let span = t.range_span(imp.range);
    let attrs_end = if imp.attrs.is_empty() {
        span.start
    } else {
        t.range_span(imp.attrs).end
    };
    let attrs = attrs_chunk(t, span.start, attrs_end, batch);
    let vis = imp.vis.map(|v| format!("{} ", oneline_text(t, v))).unwrap_or_default();
    let list = match &imp.list {
        ImportList::Single(n) => t.text(*n).to_string(),
        ImportList::Glob(_) => "*".to_string(),
        ImportList::Many(names) => {
            let inner: Vec<String> = names
                .iter()
                .map(|(n, a)| match a {
                    Some(a) => format!("{} as {}", t.text(*n), t.text(*a)),
                    None => t.text(*n).to_string(),
                })
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
    };
    Plan::Import {
        attrs,
        line: format!("{vis}import {list} from {};", t.text(imp.path)),
    }
}

/// Commentaires d'un intervalle entre deux membres.
fn gap_comments(t: &Toks, gap: Span) -> (Vec<Comment>, Option<String>, bool) {
    let mut comments = Vec::new();
    let mut trailing = None;
    let mut newlines_seen = 0;
    let mut blank_pending = false;
    for tok in crate::lexer::tokenize(&t.src[gap.start..gap.end]) {
        let s = &t.src[gap.start + tok.start..gap.start + tok.end];
        match tok.kind {
            crate::lexer::TokenKind::Whitespace => {
                let n = s.matches('\n').count();
                newlines_seen += n;
                if n >= 2 {
                    blank_pending = true;
                }
            }
            _ => {
                if newlines_seen == 0 && comments.is_empty() && trailing.is_none() {
                    trailing = Some(s.trim_end().to_string());
                } else {
                    comments.push(Comment {
                        text: s.trim_end().to_string(),
                        blank_before: blank_pending,
                    });
                    blank_pending = false;
                }
            }
        }
    }
    (comments, trailing, blank_pending)
}

fn plan_class(t: &Toks, c: &Class, batch: &mut Batch) -> ClassPlan {
    let span = t.range_span(c.range);
    let header_start = t.span(c.header).start;
    let attrs = attrs_chunk(t, span.start, header_start, batch);

    let vis = c.vis.map(|v| format!("{} ", oneline_text(t, v))).unwrap_or_default();
    let generics = c
        .generics
        .as_ref()
        .map(|g| oneline_text(t, g.range))
        .unwrap_or_default();
    let where_clause = c
        .where_preds
        .filter(|p| !p.is_empty())
        .map(|p| format!(" where {}", oneline_text(t, p)))
        .unwrap_or_default();
    // Membre factice : avec un corps vide, rustfmt laisserait `where` sur la ligne.
    let header = batch.add(format!(
        "{vis}struct {}{generics}{where_clause} {{ __crust: () }}",
        t.text(c.name)
    ));
    // Chaque composant : visibilité, type formaté par rustfmt, nom explicite.
    let overs: Vec<OverPlan> = c
        .overs
        .iter()
        .map(|o| OverPlan {
            vis: o.vis.map(|v| format!("{} ", oneline_text(t, v))).unwrap_or_default(),
            ty: batch.add(format!("type __CrustTy = {};", oneline_text(t, o.ty))),
            name: o.explicit_name.map(|n| t.text(n).to_string()),
        })
        .collect();

    let mut members = Vec::new();
    let mut prev_end = t.span(c.open).end;
    for m in &c.members {
        let ms = t.range_span(m.range());
        let (mut comments, trailing, blank_after) = gap_comments(t, Span::new(prev_end, ms.start));
        // Commentaire sur la ligne du membre précédent (ou de l'accolade ouvrante).
        if let Some(tr) = trailing {
            match members.last_mut() {
                Some(last) => {
                    let last: &mut MemberPlan = last;
                    last.trailing = Some(tr);
                }
                None => comments.insert(
                    0,
                    Comment {
                        text: tr,
                        blank_before: false,
                    },
                ),
            }
        }
        let blank_before = !members.is_empty()
            && (comments.first().is_some_and(|c| c.blank_before) || (comments.is_empty() && blank_after));
        let blank_after_comments = !comments.is_empty() && blank_after;
        let kind = match m {
            Member::Field(f) => {
                let field = text(t, t.range_span(f.range)).trim();
                MemberKind::Field(batch.add(format!("struct __CrustFields {{\n{field},\n}}")))
            }
            Member::Item(r) if t.inner_attr_end(r.start, r.end).is_some() => {
                // rustfmt supprime les attributs internes d'un `impl` : recopiés tels quels.
                MemberKind::Verbatim(text(t, t.range_span(*r)).trim().to_string())
            }
            Member::Item(r) => {
                let item = text(t, t.range_span(*r)).trim();
                MemberKind::Item(batch.add(format!("impl __CrustItems {{\n{item}\n}}")))
            }
            Member::TraitImpl(ti) => plan_trait_impl(t, ti, batch),
            Member::Error(..) => unreachable!("refusé plus haut"),
        };
        members.push(MemberPlan {
            blank_before,
            comments,
            blank_after_comments,
            kind,
            trailing: None,
        });
        prev_end = ms.end;
    }
    let (tail, trailing, _) = gap_comments(t, Span::new(prev_end, t.span(c.close).start));
    if let (Some(tr), Some(last)) = (trailing, members.last_mut()) {
        last.trailing = Some(tr);
    }
    ClassPlan {
        attrs,
        header,
        overs,
        members,
        tail,
    }
}

fn plan_trait_impl(t: &Toks, ti: &TraitImpl, batch: &mut Batch) -> MemberKind {
    let start = t.range_span(ti.range).start;
    let header_start = t.span(ti.header).start;
    let attrs = attrs_chunk(t, start, header_start, batch);
    let unsafe_kw = if ti.is_unsafe { "unsafe " } else { "" };
    let generics = ti
        .generics
        .as_ref()
        .map(|g| oneline_text(t, g.range))
        .unwrap_or_default();
    let neg = if ti.negative { "!" } else { "" };
    let where_clause = ti
        .where_preds
        .filter(|p| !p.is_empty())
        .map(|p| format!(" where {}", oneline_text(t, p)))
        .unwrap_or_default();
    let header = batch.add(format!(
        "{unsafe_kw}impl{generics} {neg}{} for __CrustSelf{where_clause} {{ fn __crust() {{}} }}",
        oneline_text(t, ti.trait_path)
    ));
    let body_span = Span::new(t.span(ti.open).end, t.span(ti.close).start);
    let body = text(t, body_span).trim();
    let body = batch.add(format!("impl __CrustItems {{\n{body}\n}}"));
    MemberKind::TraitImpl { attrs, header, body }
}

// ------------------------------------------------------------------ rendu

/// En-tête formaté par rustfmt → lignes de signature, sans l'accolade ouvrante
/// ni le membre factice : `X {` + … ou `X` + `where` + … + `{` + ….
fn signature(lines: &[String]) -> Vec<String> {
    let mut sig = Vec::new();
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim() == "{" {
            break;
        }
        if let Some(s) = trimmed.strip_suffix(" {") {
            sig.push(s.to_string());
            break;
        }
        sig.push(trimmed.to_string());
    }
    sig
}

fn render(plan: &Plan, chunks: &Chunks) -> Vec<String> {
    match plan {
        Plan::Import { attrs, line } => {
            let mut out: Vec<String> = attrs.map(|k| chunks.attrs(k)).unwrap_or_default();
            out.push(line.clone());
            out
        }
        Plan::Class(c) => render_class(c, chunks),
    }
}

fn render_class(c: &ClassPlan, chunks: &Chunks) -> Vec<String> {
    let mut out: Vec<String> = c.attrs.map(|k| chunks.attrs(k)).unwrap_or_default();

    // `pub struct Nom<T>` → `pub class Nom<T> over …`, `where` éventuel en dessous.
    let mut sig = signature(chunks.all(c.header));
    if let Some(first) = sig.first_mut() {
        *first = first.replacen("struct ", "class ", 1);
    }
    let where_at = sig.iter().position(|l| l.trim() == "where");
    let name_line = where_at.map_or(sig.len().saturating_sub(1), |w| w.saturating_sub(1));
    if !c.overs.is_empty()
        && let Some(l) = sig.get_mut(name_line)
    {
        let overs: Vec<String> = c
            .overs
            .iter()
            .map(|o| {
                let ty = chunks.all(o.ty).join(" ");
                let ty = ty
                    .trim()
                    .trim_start_matches("type __CrustTy =")
                    .trim()
                    .trim_end_matches(';')
                    .trim();
                match &o.name {
                    Some(n) => format!("{}{ty}({n})", o.vis),
                    None => format!("{}{ty}", o.vis),
                }
            })
            .collect();
        l.push_str(&format!(" over {}", overs.join(", ")));
    }
    let empty = c.members.is_empty() && c.tail.is_empty();
    match (where_at.is_some(), empty) {
        (false, true) => {
            let last = sig.pop().unwrap_or_default();
            out.extend(sig);
            out.push(format!("{last} {{}}"));
            return out;
        }
        (false, false) => {
            let last = sig.pop().unwrap_or_default();
            out.extend(sig);
            out.push(format!("{last} {{"));
        }
        (true, _) => {
            out.extend(sig);
            out.push("{".into());
        }
    }

    for m in &c.members {
        if m.blank_before {
            out.push(String::new());
        }
        for com in &m.comments {
            if com.blank_before && out.last().is_some_and(|l| !l.is_empty() && l != "{") {
                out.push(String::new());
            }
            out.extend(reindent(&com.text, "    "));
        }
        if m.blank_after_comments {
            out.push(String::new());
        }
        let start = out.len();
        match &m.kind {
            MemberKind::Verbatim(s) => out.extend(reindent(s, "    ")),
            MemberKind::Field(k) | MemberKind::Item(k) => out.extend(chunks.body(*k)),
            MemberKind::TraitImpl { attrs, header, body } => {
                if let Some(a) = attrs {
                    out.extend(chunks.attrs(*a).into_iter().map(|l| indent(&l, "    ")));
                }
                let mut sig = signature(chunks.all(*header));
                for l in &mut sig {
                    *l = l.replacen(" for __CrustSelf", "", 1);
                }
                let has_where = sig.iter().any(|l| l.trim() == "where");
                let body_lines = chunks.body(*body);
                let last = if has_where { None } else { sig.pop() };
                out.extend(sig.into_iter().map(|l| indent(&l, "    ")));
                let open = match last {
                    Some(l) => indent(&format!("{l} {{"), "    "),
                    None => "    {".to_string(),
                };
                if body_lines.is_empty() && !has_where {
                    out.push(format!("{open}}}"));
                } else if body_lines.is_empty() {
                    out.push(open);
                    out.push("    }".to_string());
                } else {
                    out.push(open);
                    out.extend(body_lines.into_iter().map(|l| indent(&l, "    ")));
                    out.push("    }".to_string());
                }
            }
        }
        let last_line = start.max(out.len().saturating_sub(1));
        if let Some(tr) = &m.trailing
            && let Some(last) = out.get_mut(last_line)
        {
            last.push(' ');
            last.push_str(tr);
        }
    }
    for com in &c.tail {
        if com.blank_before {
            out.push(String::new());
        }
        out.extend(reindent(&com.text, "    "));
    }
    out.push("}".into());
    out
}

fn indent(line: &str, by: &str) -> String {
    if line.is_empty() {
        String::new()
    } else {
        format!("{by}{line}")
    }
}

/// Commentaire (éventuellement sur plusieurs lignes) réindenté.
fn reindent(text: &str, by: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let common = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let l = if i == 0 {
                l.trim_start()
            } else {
                &l[common.min(l.len() - l.trim_start().len())..]
            };
            indent(l.trim_end(), by)
        })
        .collect()
}
