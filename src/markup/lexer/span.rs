use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self { start, end, line, column }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line.min(other.line),
            column: self.column.min(other.column),
        }
    }

    pub fn to(self, end: usize) -> Span {
        Span { end, ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrSpan<'a>(&'a str);

impl<'a> StrSpan<'a> {
    pub fn new(s: &'a str) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &'a str {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a> From<&'a str> for StrSpan<'a> {
    fn from(s: &'a str) -> Self {
        StrSpan(s)
    }
}

impl fmt::Display for StrSpan<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}