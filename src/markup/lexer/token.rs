use super::span::{Span, StrSpan};
use super::ListKind;

#[derive(Debug, Clone)]
pub enum Token<'a> {
    // structural blocks
    Heading {
        level: u8,
        span: Span,
    },
    Blockquote {
        level: u8,
        span: Span,
    },
    HorizontalRule {
        span: Span,
    },

    ListItem {
        indent: usize,
        kind: ListKind,
        marker: StrSpan<'a>,
        span: Span,
    },

    ParagraphBreak,

    // inline
    Text(StrSpan<'a>),

    BoldStart,
    BoldEnd,
    ItalicStart,
    ItalicEnd,
    StrikeStart,
    StrikeEnd,

    Image {
        alt: StrSpan<'a>,
        url: StrSpan<'a>,
        span: Span,
    },

    // chunks
    ChunkStartImplicit {
        name: StrSpan<'a>,
        span: Span,
    },
    ChunkStartExplicit,
    ChunkExplicit {
        name: StrSpan<'a>,
        span: Span,
    },
    ChunkEnd {
        name: StrSpan<'a>,
        span: Span,
    },

    // directives
    Directive {
        name: StrSpan<'a>,
        params: StrSpan<'a>,
        span: Span,
    },

    // references
    ChunkRef(StrSpan<'a>),
    
    // comment
    Comment(StrSpan<'a>),

    // control
    Newline,
    EOF,
}

impl<'a> Token<'a> {
    pub fn span(&self) -> Option<Span> {
        match self {
            Token::Heading { span, .. } => Some(*span),
            Token::Blockquote { span, .. } => Some(*span),
            Token::HorizontalRule { span } => Some(*span),
            Token::ListItem { span, .. } => Some(*span),
            Token::ParagraphBreak => None,
            Token::Text(_) => None,
            Token::BoldStart | Token::BoldEnd => None,
            Token::ItalicStart | Token::ItalicEnd => None,
            Token::StrikeStart | Token::StrikeEnd => None,
            Token::Image { span, .. } => Some(*span),
            Token::ChunkStartImplicit { span, .. } => Some(*span),
            Token::ChunkStartExplicit => None,
            Token::ChunkExplicit { span, .. } => Some(*span),
            Token::ChunkEnd { span, .. } => Some(*span),
            Token::Directive { span, .. } => Some(*span),
            Token::ChunkRef(_) => None,
            Token::Comment(_) => None,
            Token::Newline => None,
            Token::EOF => None,
        }
    }
}
