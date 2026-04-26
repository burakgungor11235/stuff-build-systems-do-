use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::{Span, StrSpan};
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct CommentHandler;

impl LineLexer for CommentHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        state.at_line_start && (line.starts_with("/'") || line.starts_with("*/'"))
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let line = line.trim_end();
        
        // Handle both /'comment'/ and */'comment'/*
        let (content, _) = if line.starts_with("/'") {
            (&line[2..], 2)
        } else {
            let text = StrSpan::new(line);
            sink.emit(Token::Text(text));
            state.clear_line_start();
            return;
        };

        // Find closing '/
        let end_marker = if content.contains("'/") {
            "'/"
        } else {
            ""
        };

        if !end_marker.is_empty() {
            let end_pos = content.find(end_marker).unwrap_or(content.len());
            let comment_content = &content[..end_pos];
            
            let comment_span = StrSpan::new(comment_content);
            sink.emit(Token::Comment(comment_span));
        } else {
            // Unclosed comment - emit as text
            let text = StrSpan::new(line);
            sink.emit(Token::Text(text));
        }
        
        state.clear_line_start();
    }

    fn priority(&self) -> u8 {
        195 // After blockquote, before heading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_comment(input: &str) -> Vec<Token<'static>> {
        let handler = CommentHandler{};
        let state = LexerState::new();
        
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
        handler.lex(input, &mut state.clone(), &mut sink);
        println!("{} : {:?}", input, sink.tokens);
        sink.tokens
    }

    #[test]
    fn test_comment_simple() {
        let tokens = lex_comment("/'Comment'/");
        assert!(matches!(tokens[0], Token::Comment(c) if c.as_str() == "Comment"));
    }

    #[test]
    fn test_comment_multiline_style() {
        let tokens = lex_comment("/'Multi\nline'/");
        // Should handle single-line still
        assert!(matches!(tokens[0], Token::Comment(_)));
    }

    #[test]
    fn test_comment_unclosed() {
        let tokens = lex_comment("/'Unclosed comment");
        // Unclosed → emit as text
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_comment_empty() {
        let tokens = lex_comment("/'/'/");
        // Parsed as Comment with "/"
        assert!(matches!(tokens[0], Token::Comment(c) if c.as_str() == "/"));
    }
}
