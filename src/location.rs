use std::fmt;

#[derive(Default, Eq, PartialOrd, Ord, PartialEq, Hash, Copy, Clone, Debug)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct Location {
    pub span: Span,
    pub line: u32,
    pub col: u32,
    pub filename: String,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct Located<T> {
    pub data: T,
    pub location: Location,
}

pub fn loc<T>(data: T, location: Location) -> Located<T> {
    Located { data, location }
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        assert!(end >= start);

        Self { start, end }
    }
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl Location {
    pub fn new(span: Span, line: u32, col: u32, filename: String) -> Self {
        assert!(line >= 1);
        assert!(col >= 1);
        Self {
            span,
            line,
            col,
            filename,
        }
    }

    // assumes same file location
    pub fn merge(self, other: &Location) -> Self {
        let span = self.span.merge(&other.span);
        let start = if self.span.start <= other.span.start {
            &self
        } else {
            other
        };

        Self {
            span,
            line: start.line,
            col: start.col,
            filename: start.filename.clone(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Located<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data)
    }
}
