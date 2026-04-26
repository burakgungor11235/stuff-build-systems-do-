use std::fmt;

pub mod default_handlers;
pub mod handlers;
pub mod inline;
pub mod lexer;
pub mod sink;
pub mod span;
pub mod state;
pub mod token;

#[cfg(test)]
mod tests;

pub use default_handlers::default_line_handlers;
pub use handlers::LineLexer;
pub use inline::InlineLexer;
pub use lexer::Lexer;
pub use sink::TokenSink;
pub use span::{Span, StrSpan};
pub use state::LexerState;
pub use token::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListKind {
    OrderedNumeric,
    OrderedAlpha,
    OrderedRoman,
    Unordered,
}

impl ListKind {
    pub fn clone(&self) -> Self {
        match self {
            ListKind::OrderedNumeric => ListKind::OrderedNumeric,
            ListKind::OrderedAlpha => ListKind::OrderedAlpha,
            ListKind::OrderedRoman => ListKind::OrderedRoman,
            ListKind::Unordered => ListKind::Unordered,
        }
    }
}

pub struct LexerBuilder {
    line_handlers: Vec<Box<dyn LineLexer>>,
}

impl LexerBuilder {
    pub fn new() -> Self {
        Self {
            line_handlers: Vec::new(),
        }
    }

    pub fn with_line_handler<H: LineLexer + 'static>(mut self, handler: H) -> Self {
        self.line_handlers.push(Box::new(handler));
        self
    }

    pub fn build<'a>(self, input: &'a str) -> Lexer<'a> {
        let line_handlers = if self.line_handlers.is_empty() {
            default_line_handlers()
        } else {
            self.line_handlers
        };
        Lexer::with_handlers(input, line_handlers)
    }
}

impl Default for LexerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
