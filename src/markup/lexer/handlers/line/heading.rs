use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::{Span, StrSpan};
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct HeadingHandler;



impl LineLexer for HeadingHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        // Must be at line start to be a heading
        state.at_line_start && line.starts_with('#')
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let start = 0;
        let mut end = 1;
        let mut level: u8 = 1;

        // Parse optional level: #, #1, #2, etc.
        while end < line.len() {
            let ch = line[end..].chars().next();
            if let Some(c) = ch {
                if c.is_ascii_digit() {
                    level = c.to_digit(10).unwrap() as u8;
                    end += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Skip whitespace after level
        while end < line.len() && line[end..].starts_with(' ') {
            end += 1;
        }

        let span = Span::new(start, end, 1, start + 1);
        // clamp to max 6
        sink.emit(Token::Heading { level: if level >= 6 {6} else { level } , span });
        state.clear_line_start();

        // If there's content after heading, emit as text
        if end < line.len() {
            let text = StrSpan::new(&line[end..]);
            sink.emit(Token::Text(text));
        }
    }

    fn priority(&self) -> u8 {
        200 // High priority - headings at line start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_heading(input: &str) -> Vec<Token<'static>> {
        let handler = HeadingHandler;
        let mut state = LexerState::new();
        state.reset_line_state(); // Set at_line_start = true (simulates start of new line)
        
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
        handler.lex(input, &mut state, &mut sink);
        println!("{} : {:?}", input, sink.tokens);
        sink.tokens
    }

    #[test]
    fn test_heading_level_1() {
        let tokens = lex_heading("# Heading");
        assert!(matches!(tokens[0], Token::Heading { level: 1, .. }));
    }

    #[test]
    fn test_heading_level_3() {
        let tokens = lex_heading("#3 Level Three");
        assert!(matches!(tokens[0], Token::Heading { level: 3, .. }));
    }

    #[test]
    fn test_heading_with_content() {
        let tokens = lex_heading("# Title of Doc");
        assert!(matches!(tokens[0], Token::Heading { level: 1, .. }));
        assert!(matches!(tokens[1], Token::Text(_)));
    }
    #[test]
    fn test_heading_level_0() {
        let tokens = lex_heading("#0 Should this be heading?");
        assert!(matches!(tokens.first(), Some(Token::Heading { level: 0, .. })));
    }

    #[test]
    fn test_heading_level_6_max() {
        let tokens = lex_heading("#6 Level Six");
        assert!(matches!(tokens[0], Token::Heading { level: 6, .. }));
    }

    #[test]
    fn test_heading_level_7_not_a_gimmick() {
        // now technically who said that we shoul;
        let tokens = lex_heading("#7 Should fall back");
        assert!(matches!(tokens[0], Token::Heading { level: 6, .. }));
    }

    #[test]
    fn test_heading_only_hash() {
        let tokens = lex_heading("#");
        assert!(matches!(tokens[0], Token::Heading { level: 1, .. }));
    }

    #[test]
    fn test_heading_hash_followed_by_letter() {
        let tokens = lex_heading("#a Not a heading level");
        assert!(matches!(tokens[0], Token::Heading { level: 1, .. }));
    }
}
