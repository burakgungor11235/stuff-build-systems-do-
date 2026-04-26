use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::StrSpan;
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct ParagraphHandler;

impl LineLexer for ParagraphHandler {
    fn can_handle(&self, _line: &str, state: &LexerState) -> bool {
        // Fallback handler - always handles what other handlers don't
        state.at_line_start
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let line = line.trim_end();
        if !line.is_empty() {
            let text = StrSpan::new(line);
            sink.emit(Token::Text(text));
        }
        state.clear_line_start();
    }

    fn priority(&self) -> u8 {
        0 // Lowest priority - fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_paragraph(input: &str) -> Vec<Token<'static>> {
        let handler = ParagraphHandler;
        let mut state = LexerState::new();
        state.reset_line_state();
        
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
        println!("paragraph: {:?} -> {:?}", input, sink.tokens);
        sink.tokens
    }

    #[test]
    fn test_paragraph_simple() {
        let tokens = lex_paragraph("Simple paragraph text");
        assert!(matches!(tokens[0], Token::Text(t) if t.as_str() == "Simple paragraph text"));
    }

    #[test]
    fn test_paragraph_empty_line() {
        let tokens = lex_paragraph("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_paragraph_whitespace_only() {
        let tokens = lex_paragraph("   ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_paragraph_with_leading_spaces() {
        let tokens = lex_paragraph("  Indented text");
        // Leading spaces are kept (per current implementation)
        assert!(matches!(tokens[0], Token::Text(t) if t.as_str() == "  Indented text"));
    }

    #[test]
    fn test_paragraph_with_trailing_spaces() {
        let tokens = lex_paragraph("Text with trailing   ");
        // Should trim trailing
        assert!(matches!(tokens[0], Token::Text(t) if t.as_str() == "Text with trailing"));
    }

    #[test]
    fn test_paragraph_single_word() {
        let tokens = lex_paragraph("word");
        assert!(matches!(tokens[0], Token::Text(t) if t.as_str() == "word"));
    }
}
