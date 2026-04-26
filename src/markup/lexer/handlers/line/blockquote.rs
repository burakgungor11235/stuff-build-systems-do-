use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::Span;
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct BlockquoteHandler;

impl LineLexer for BlockquoteHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        state.at_line_start && line.starts_with('>')
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let mut level = 0;
        let mut end = 0;

        // Count consecutive > characters
        for (i, ch) in line.char_indices() {
            if ch == '>' {
                level += 1;
                end = i + ch.len_utf8();
            } else if ch == ' ' {
                // Allow single space after >
                if level > 0 && i == end {
                    end = i + 1;
                    continue;
                }
                break;
            } else {
                break;
            }
        }

        // Also skip additional leading spaces after >
        while end < line.len() && line[end..].starts_with(' ') {
            end += 1;
        }

        let span = Span::new(0, end, 1, 1);
        sink.emit(Token::Blockquote { level, span });
        state.clear_line_start();

        // Emit remaining as text if any
        if end < line.len() {
            use crate::markup::lexer::span::StrSpan;
            let text = StrSpan::new(&line[end..]);
            sink.emit(Token::Text(text));
        }
    }

    fn priority(&self) -> u8 {
        190 // High priority - blockquotes at line start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_blockquote(input: &str) -> Vec<Token<'static>> {
        let handler = BlockquoteHandler;
        let mut state = LexerState::new();
        state.reset_line_state(); // Set at_line_start = true
        
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
    fn test_blockquote_level_1() {
        let tokens = lex_blockquote("> Nobody will believe you");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
    }

    #[test]
    fn test_blockquote_level_3() {
        let tokens = lex_blockquote(">>> Nested quote");
        assert!(matches!(tokens[0], Token::Blockquote { level: 3, .. }));
    }

    #[test]
    fn test_blockquote_with_content() {
        let tokens = lex_blockquote("> This is a quote");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
        assert!(matches!(tokens[1], Token::Text(_)));
    }
    #[test]
    fn test_blockquote_with_space_no_content() {
        let tokens = lex_blockquote("> ");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
    }

    #[test]
    fn test_blockquote_no_space_directly() {
        let tokens = lex_blockquote(">No space here");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
    }

    #[test]
    fn test_blockquote_multiple_spaces() {
        let tokens = lex_blockquote(">    Multiple spaces");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
    }

    #[test]
    fn test_blockquote_no_content() {
        let tokens = lex_blockquote(">");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
    }

    #[test]
    fn test_blockquote_greater_than_in_text() {
        let tokens = lex_blockquote("> Check if 5 > 3");
        assert!(matches!(tokens[0], Token::Blockquote { level: 1, .. }));
    }
}
