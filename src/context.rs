use crate::diagnostic::Diagnostic;
use crate::reporter;

#[derive(Default)]
pub struct Context {
    pub diagnostics: Vec<Diagnostic>,
    pub source: Vec<u8>,
}

impl Context {
    pub fn new(source: &[u8]) -> Self {
        Self {
            diagnostics: Vec::new(),
            source: source.to_owned(),
        }
    }

    pub fn emit_diag(&mut self, diag: Diagnostic) {
        self.diagnostics.push(diag);
    }

    pub fn report_diags(&self) {
        let mut sorted_diagnostics = self.diagnostics.clone();
        sorted_diagnostics.sort_by(|a, b| {
            a.location
                .filename
                .cmp(&b.location.filename)
                .then(a.location.span.cmp(&b.location.span))
        });
        for diag in &self.diagnostics {
            reporter::report(&self.source, diag);
        }
    }
}
