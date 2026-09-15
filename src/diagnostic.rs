use core::fmt;

use colored::{ColoredString, Colorize};

use crate::location::Location;

#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum DiagnosticKind {
    Error,
    Warning,
    Info,
    Diagnostic,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub description: String,
    pub kind: DiagnosticKind,
    pub location: Location,
}

impl DiagnosticKind {
    pub fn color(&self, message: &str) -> ColoredString {
        match self {
            DiagnosticKind::Error => message.red(),
            DiagnosticKind::Warning => message.purple(),
            DiagnosticKind::Info => message.blue(),
            DiagnosticKind::Diagnostic => message.black(),
        }
    }
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let k = match self {
            DiagnosticKind::Error => "Error",
            DiagnosticKind::Warning => "Warning",
            DiagnosticKind::Info => "Info",
            DiagnosticKind::Diagnostic => "Diagnostic",
        };
        write!(f, "{k}")
    }
}
