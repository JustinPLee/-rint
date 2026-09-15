use crate::diagnostic::Diagnostic;
use crate::location::Location;
use colored::Colorize;

// radius of context below and above target line
const CONTEXT_LINES: usize = 1;

pub fn report(source: &[u8], diag: &Diagnostic) {
    let file_info = format_file_info(&diag.location, diag.location.filename.clone());
    let diag_header = format!(
        "{} {}",
        diag.kind
            .color(&format!("{}:", diag.kind.to_string().to_lowercase()))
            .bold(),
        file_info
    );

    let lines = collect_source_lines(source);
    let error_line = (diag.location.line as usize).saturating_sub(1); // 0-indexed

    let context_start = error_line.saturating_sub(CONTEXT_LINES);
    let context_end = (error_line + CONTEXT_LINES).min(lines.len().saturating_sub(1));

    // col is 1-indexed, span covers the lexeme
    let span = diag.location.span;
    let hl_start = diag.location.col as usize - 1; // 0-indexed within line
    let hl_len = (span.end as usize).saturating_sub(span.start as usize);

    let mut output = vec![diag_header];

    for line_idx in context_start..=context_end {
        let line_bytes = lines[line_idx];
        let line_str = String::from_utf8_lossy(line_bytes);
        let line = (line_idx + 1) as u32; // 1-indexed

        if line_idx == error_line {
            // highlighted line so color the lexeme
            let before = &line_str[..hl_start.min(line_str.len())];
            let hl_end = (hl_start + hl_len).min(line_str.len());
            let lexeme = &line_str[hl_start.min(line_str.len())..hl_end];
            let after = &line_str[hl_end.min(line_str.len())..];

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

/// acceptable for now
fn collect_source_lines(source: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    for i in 0..source.len() {
        if source[i] == b'\n' {
            lines.push(&source[start..i]);
            start = i + 1;
        }
    }
    // last line
    if start <= source.len() {
        lines.push(&source[start..]);
    }
    lines
}

fn format_file_info(location: &Location, file_name: String) -> String {
    format!("@{}:{}:{}", file_name, location.line, location.col)
        .green()
        .to_string()
}

fn format_sidebar(line: Option<u32>) -> String {
    let sidebar_width = 6;
    if let Some(line) = line {
        let s = line.to_string();
        let num_spaces = sidebar_width - s.len() - 1;
        format!("{}{}|", " ".repeat(num_spaces), s)
            .cyan()
            .bold()
            .to_string()
    } else {
        format!("{}|", " ".repeat(sidebar_width - 1))
            .cyan()
            .to_string()
    }
}
