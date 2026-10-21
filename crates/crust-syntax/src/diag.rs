//! Diagnostics propres à Crust (codes `CRxxxx`, section 19 du cahier des charges).

use crate::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    /// Syntaxe invalide dans une classe ou un import.
    CR0001,
    /// Champ en collision avec un champ `over`.
    CR0002,
    /// Deux composants `over` avec le même nom.
    CR0003,
    /// `over` sur un type non-chemin sans nom explicite.
    CR0004,
    /// Élément interdit dans le corps d'une classe.
    CR0005,
    /// Fichier d'import introuvable.
    CR0006,
    /// Symbole absent du fichier importé.
    CR0007,
    /// Import sans extension ambigu.
    CR0008,
    /// Nom de fichier ou de répertoire qui n'est pas un identifiant.
    CR0012,
    /// Import hors de `src/`.
    CR0013,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::CR0001 => "CR0001",
            Code::CR0002 => "CR0002",
            Code::CR0003 => "CR0003",
            Code::CR0004 => "CR0004",
            Code::CR0005 => "CR0005",
            Code::CR0006 => "CR0006",
            Code::CR0007 => "CR0007",
            Code::CR0008 => "CR0008",
            Code::CR0012 => "CR0012",
            Code::CR0013 => "CR0013",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    pub message: String,
    /// Position principale, dans le `.rsc`.
    pub span: Span,
    pub labels: Vec<Label>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(code: Code, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            message: message.into(),
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Invocation `compile_error!` injectée dans le code généré (section 19.1).
    pub fn to_compile_error(&self) -> String {
        let mut msg = format!("[{}] {}", self.code.as_str(), self.message);
        if let Some(help) = &self.help {
            msg.push_str("\n= help: ");
            msg.push_str(help);
        }
        format!("compile_error!({msg:?});")
    }
}
