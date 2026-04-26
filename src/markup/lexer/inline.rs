use crate::markup::lexer::span::StrSpan;
use crate::markup::lexer::token::Token;
use crate::markup::lexer::TokenSink;

pub struct InlineLexer {
    handlers: Vec<Box<dyn InlineHandler>>,
}

impl InlineLexer {
    pub fn new() -> Self {
        Self {
            handlers: default_inline_handlers(),
        }
    }

    pub fn with_handlers(handlers: Vec<Box<dyn InlineHandler>>) -> Self {
        Self { handlers }
    }

    pub fn lex(&self, text: &str, sink: &mut dyn TokenSink) {
        if text.is_empty() {
            return;
        }

        let mut remaining = text;
        
        while !remaining.is_empty() {
            // Find the earliest inline delimiter
            let mut best_match: Option<(&dyn InlineHandler, usize, &str, &str)> = None;
            
            for handler in &self.handlers {
                if let Some((start_pos, content, end_marker)) = handler.find_delimiter(remaining) {
                    match &best_match {
                        None => {
                            best_match = Some((handler.as_ref(), start_pos, content, end_marker));
                        }
                        Some((_, best_pos, _, _)) if start_pos < *best_pos => {
                            best_match = Some((handler.as_ref(), start_pos, content, end_marker));
                        }
                        _ => {}
                    }
                }
            }

            match best_match {
                Some((handler, start_pos, content, end_marker)) => {
                    // Emit text before the delimiter
                    if start_pos > 0 {
                        let before = &remaining[..start_pos];
                        sink.emit(Token::Text(StrSpan::new(before)));
                    }

                    // Emit the inline token
                    handler.emit_token(content, sink);

                    // Skip past the delimiter
                    let end_pos = if end_marker.is_empty() {
                        // For delimiters without an end marker, we already consumed the content
                        start_pos + handler.start_marker().len() + content.len()
                    } else {
                        start_pos + handler.start_marker().len() + content.len() + handler.end_marker().len()
                    };
                    remaining = &remaining[end_pos..];
                }
                None => {
                    // No more delimiters - emit the rest as text
                    sink.emit(Token::Text(StrSpan::new(remaining)));
                    break;
                }
            }
        }
    }
}

impl Default for InlineLexer {
    fn default() -> Self {
        Self::new()
    }
}

pub trait InlineHandler: Send + Sync {
    fn start_marker(&self) -> &str;
    fn end_marker(&self) -> &str;
    fn token_type(&self) -> InlineType;

    fn find_delimiter<'a>(&self, text: &'a str) -> Option<(usize, &'a str, &'a str)> {
        // Try to find start marker
        if let Some(start_pos) = text.find(self.start_marker()) {
            let after_start = &text[start_pos + self.start_marker().len()..];
            
            // Try to find end marker in the remaining text
            if let Some(end_pos) = after_start.find(self.end_marker()) {
                let content = &after_start[..end_pos];
                let end_marker = &after_start[end_pos..end_pos + self.end_marker().len()];
                return Some((start_pos, content, end_marker));
            }
        }
        None
    }

    fn emit_token(&self, content: &str, sink: &mut dyn TokenSink) {
        use crate::markup::lexer::span::StrSpan;
        
        match self.token_type() {
            InlineType::Bold => {
                sink.emit(Token::BoldStart);
                sink.emit(Token::Text(StrSpan::new(content)));
                sink.emit(Token::BoldEnd);
            }
            InlineType::Italic => {
                sink.emit(Token::ItalicStart);
                sink.emit(Token::Text(StrSpan::new(content)));
                sink.emit(Token::ItalicEnd);
            }
            InlineType::Strike => {
                sink.emit(Token::StrikeStart);
                sink.emit(Token::Text(StrSpan::new(content)));
                sink.emit(Token::StrikeEnd);
            }
            InlineType::Image => {
                // Parse alt | url
                let parts: Vec<&str> = content.splitn(2, '|').collect();
                let alt = parts.get(0).map(|s| s.trim()).unwrap_or("");
                let url = parts.get(1).map(|s| s.trim()).unwrap_or("");
                
                sink.emit(Token::Image {
                    alt: StrSpan::new(alt),
                    url: StrSpan::new(url),
                    span: crate::markup::lexer::span::Span::new(0, content.len(), 1, 1),
                });
            }
            InlineType::ChunkRef => {
                sink.emit(Token::ChunkRef(StrSpan::new(content)));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineType {
    Bold,
    Italic,
    Strike,
    Image,
    ChunkRef,
}

pub struct BoldHandler;
pub struct ItalicHandler;
pub struct StrikeHandler;
pub struct ImageHandler;
pub struct ChunkRefHandler;

impl InlineHandler for BoldHandler {
    fn start_marker(&self) -> &str { "*" }
    fn end_marker(&self) -> &str { "*" }
    fn token_type(&self) -> InlineType { InlineType::Bold }
}

impl InlineHandler for ItalicHandler {
    fn start_marker(&self) -> &str { "_" }
    fn end_marker(&self) -> &str { "_" }
    fn token_type(&self) -> InlineType { InlineType::Italic }
}

impl InlineHandler for StrikeHandler {
    fn start_marker(&self) -> &str { "~" }
    fn end_marker(&self) -> &str { "~" }
    fn token_type(&self) -> InlineType { InlineType::Strike }
}

impl InlineHandler for ImageHandler {
    fn start_marker(&self) -> &str { "![" }
    fn end_marker(&self) -> &str { "]" }
    fn token_type(&self) -> InlineType { InlineType::Image }
}

impl InlineHandler for ChunkRefHandler {
    fn start_marker(&self) -> &str { "&" }
    fn end_marker(&self) -> &str { "" } // No end marker - consume until whitespace or special char
    fn token_type(&self) -> InlineType { InlineType::ChunkRef }
    
    fn find_delimiter<'a>(&self, text: &'a str) -> Option<(usize, &'a str, &'a str)> {
        if let Some(start_pos) = text.find('&') {
            let rest = &text[start_pos + 1..];
            let mut end = 0;
            for (i, ch) in rest.char_indices() {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                    end = i + ch.len_utf8();
                } else {
                    break;
                }
            }
            if end > 0 {
                let content = &rest[..end];
                return Some((start_pos, content, ""));
            }
        }
        None
    }
}

fn default_inline_handlers() -> Vec<Box<dyn InlineHandler>> {
    vec![
        Box::new(ImageHandler),
        Box::new(ChunkRefHandler),
        Box::new(BoldHandler),
        Box::new(ItalicHandler),
        Box::new(StrikeHandler),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_inline(input: &str) -> Vec<Token<'static>> {
        let lexer = InlineLexer::new();
        
        #[derive(Default)]
        struct TestSink { tokens: Vec<Token<'static>> }
        
        impl TokenSink for TestSink {
            fn emit(&mut self, token: Token<'_>) {
                let token_static = unsafe {
                    std::mem::transmute::<Token<'_>, Token<'static>>(token)
                };
                self.tokens.push(token_static);
            }
        }
        
        let mut sink = TestSink::default();
        lexer.lex(input, &mut sink);
        println!("inline: {:?} -> {:?}", input, sink.tokens);
        sink.tokens
    }

    // ============ BOLD TESTS ============

    #[test]
    fn test_bold_simple() {
        let tokens = lex_inline("*bold*");
        assert!(matches!(tokens[0], Token::BoldStart));
        assert!(matches!(tokens[1], Token::Text(_)));
        assert!(matches!(tokens[2], Token::BoldEnd));
    }

    #[test]
    fn test_bold_empty() {
        let tokens = lex_inline("**");
        // Empty between delimiters - starts, then (empty) text, then ends
        assert!(matches!(tokens[0], Token::BoldStart));
        // tokens[1] is empty text - that's fine
        assert!(matches!(tokens[2], Token::BoldEnd));
    }

    #[test]
    fn test_bold_adjacent() {
        let tokens = lex_inline("*a* *b*");
        assert!(tokens.len() >= 6);
    }

    #[test]
    fn test_bold_no_closing() {
        let tokens = lex_inline("*no closing");
        // Should emit as plain text
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_bold_start_no_text() {
        let tokens = lex_inline("*");
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    // ============ ITALIC TESTS ============

    #[test]
    fn test_italic_simple() {
        let tokens = lex_inline("_italic_");
        assert!(matches!(tokens[0], Token::ItalicStart));
        assert!(matches!(tokens[1], Token::Text(_)));
        assert!(matches!(tokens[2], Token::ItalicEnd));
    }

    #[test]
    fn test_italic_empty() {
        let tokens = lex_inline("__");
        // Empty between delimiters
        assert!(matches!(tokens[0], Token::ItalicStart));
        assert!(matches!(tokens[2], Token::ItalicEnd));
    }

    // ============ STRIKETHROUGH TESTS ============

    #[test]
    fn test_strike_simple() {
        let tokens = lex_inline("~strike~");
        assert!(matches!(tokens[0], Token::StrikeStart));
        assert!(matches!(tokens[1], Token::Text(_)));
        assert!(matches!(tokens[2], Token::StrikeEnd));
    }

    // ============ IMAGE TESTS ============

    #[test]
    fn test_image_simple() {
        let tokens = lex_inline("![alt text | ./image.png]");
        assert!(matches!(tokens[0], Token::Image { alt, url, .. } if alt.as_str() == "alt text" && url.as_str() == "./image.png"));
    }

    #[test]
    fn test_image_no_pipe() {
        let tokens = lex_inline("![no pipe]");
        // Should still parse but url will be empty
        assert!(matches!(tokens[0], Token::Image { .. }));
    }

    #[test]
    fn test_image_empty() {
        let tokens = lex_inline("![]");
        assert!(matches!(tokens[0], Token::Image { alt, url, .. } if alt.as_str().is_empty()));
    }

    #[test]
    fn test_image_only_bracket() {
        let tokens = lex_inline("![");
        // Unclosed - should emit as text
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    // ============ MIXED/EDGE CASES ============

    #[test]
    fn test_nested_bold_italic() {
        let tokens = lex_inline("*bold _italic_*");
        // Test nested: should this work?
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_adjacent_different_formats() {
        let tokens = lex_inline("*~strike~* _italic_");
        assert!(!tokens.is_empty());
    }
}
