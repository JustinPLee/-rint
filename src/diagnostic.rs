use crate::location::Location;
use colored::{ColoredString, Colorize};
use core::fmt;

#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum DiagnosticKind {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub description: String,
    pub location: Location,
}

pub fn report_diags<T: Into<Diagnostic> + Clone>(source: &[u8], diagnostics: &[T]) -> bool {
    let mut diagnostics: Vec<Diagnostic> = diagnostics.iter().cloned().map(|d| d.into()).collect();
    // sort by file, then location
    diagnostics.sort_by(|a, b| {
        a.location
            .filename
            .cmp(&b.location.filename)
            .then(a.location.span.cmp(&b.location.span))
    });

    for diag in &diagnostics {
        report(source, diag);
    }

    !diagnostics.is_empty()
}

// radius of context below and above target line
const CONTEXT_LINES: usize = 1;
const SIDEBAR_WIDTH: usize = 6;

fn report(source: &[u8], diag: &Diagnostic) {
    let file_info = format_file_info(&diag.location);
    let diag_header = format!(
        "{} {}",
        diag.kind
            .color(&format!("{}:", diag.kind.to_string().to_lowercase()))
            .bold(),
        file_info
    );

    let lines: Vec<&[u8]> = source.split(|&byte| byte == b'\n').collect();
    let error_line = (diag.location.line as usize).saturating_sub(1); // 0-indexed

    let context_start = error_line.saturating_sub(CONTEXT_LINES);
    let context_end = error_line
        .saturating_add(CONTEXT_LINES)
        .min(lines.len().saturating_sub(1));

    // col is 1-indexed, span covers the lexeme
    let span = diag.location.span;
    let hl_len = (span.end as usize).saturating_sub(span.start as usize);

    let mut output = vec![diag_header];

    for line_idx in context_start..=context_end {
        let line_str = String::from_utf8_lossy(lines[line_idx]);
        let line = (line_idx + 1) as u32; // 1-indexed

        if line_idx == error_line {
            // highlighted line so color the lexeme
            let hl_start = ((diag.location.col as usize).max(1) - 1).min(line_str.len());
            let hl_end = hl_start.saturating_add(hl_len).min(line_str.len());
            let hl_len = hl_end - hl_start;
            let before = &line_str[..hl_start];
            let lexeme = &line_str[hl_start..hl_end];
            let after = &line_str[hl_end..];

            let colored_line = format!("{}{}{}", before, diag.kind.color(lexeme).bold(), after);
            output.push(format!("{}{}", format_sidebar(Some(line)), colored_line));

            // arrows + description
            let arrows = format!("{}{}", " ".repeat(hl_start), "^".repeat(hl_len.max(1)));
            output.push(format!(
                "{}{}",
                format_sidebar(None),
                diag.kind.color(&arrows)
            ));
            output.push(format!(
                "{}{}{}",
                format_sidebar(None),
                " ".repeat(hl_start),
                diag.kind.color(&diag.description)
            ));
        } else {
            // context line, no highlighting
            output.push(format!("{}{}", format_sidebar(Some(line)), line_str));
        }
    }

    println!("{}\n", output.join("\n"));
}

fn format_file_info(location: &Location) -> String {
    format!("@{}:{}:{}", location.filename, location.line, location.col)
        .green()
        .to_string()
}

fn format_sidebar(line: Option<u32>) -> String {
    if let Some(line) = line {
        let s = line.to_string();
        let num_spaces = SIDEBAR_WIDTH - s.len() - 1;
        format!("{}{}|", " ".repeat(num_spaces), s)
            .cyan()
            .bold()
            .to_string()
    } else {
        format!("{}|", " ".repeat(SIDEBAR_WIDTH - 1))
            .cyan()
            .to_string()
    }
}

impl DiagnosticKind {
    pub fn color(&self, message: &str) -> ColoredString {
        match self {
            DiagnosticKind::Error => message.red(),
            DiagnosticKind::Warning => message.purple(),
            DiagnosticKind::Info => message.blue(),
        }
    }
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let k = match self {
            DiagnosticKind::Error => "Error",
            DiagnosticKind::Warning => "Warning",
            DiagnosticKind::Info => "Info",
        };
        write!(f, "{k}")
    }
}
