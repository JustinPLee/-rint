#[derive(Default, Eq, PartialOrd, Ord, PartialEq, Copy, Clone, Debug)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Debug, Default)]
pub struct Location {
    pub span: Span,
    pub line: u32,
    pub col: u32,
    pub filename: String,
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

    pub fn merge(self, other: &Location) -> Self {
        Self {
            span: self.span.merge(&other.span),
            line: self.line.min(other.line),
            col: self.col,
            filename: self.filename.clone(),
        }
    }
}

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Debug)]
pub struct Located<T>(pub T, pub Location);
pub fn loc<T: Clone>(data: T, location: Location) -> Located<T> {
    Located(data, location)
}

pub fn unloc<T: Clone>(data: &Located<T>) -> T {
    data.0.clone()
}
