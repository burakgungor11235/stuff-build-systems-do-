use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::Span;
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct HorizontalRuleHandler;


// if I'm honest, I'm not really sold on the design just yet.
impl LineLexer for HorizontalRuleHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        state.at_line_start && (line == "---" || line.starts_with("---"))
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let content = line.trim();
        if content == "---" || content.len() >= 3 && content.chars().all(|c| c == '-') {
            let span = Span::new(0, line.len(), 1, 1);
            sink.emit(Token::HorizontalRule { span });
        } else {
            use crate::markup::lexer::span::StrSpan;
            let text = StrSpan::new(line);
            sink.emit(Token::Text(text));
        }
        state.clear_line_start();
    }

    fn priority(&self) -> u8 {
        210
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_hr(input: &str) -> Vec<Token<'static>> {
        let handler = HorizontalRuleHandler;
        let mut state = LexerState::new();
        state.reset_line_state();

        #[derive(Default)]
        struct TestSink {
            tokens: Vec<Token<'static>>,
        }

        impl TokenSink for TestSink {
            fn emit(&mut self, token: Token<'_>) {
                let token_static =
                    unsafe { std::mem::transmute::<Token<'_>, Token<'static>>(token) };
                self.tokens.push(token_static);
            }
        }

        let mut sink = TestSink::default();
        handler.lex(input, &mut state, &mut sink);
        println!("{} : {:?}", input, sink.tokens);
        sink.tokens
    }

    #[test]
    fn test_horizontal_rule() {
        let tokens = lex_hr("---");
        assert!(matches!(tokens[0], Token::HorizontalRule { .. }));
    }

    #[test]
    fn test_horizontal_rule_long() {
        let tokens = lex_hr("------");
        assert!(matches!(tokens[0], Token::HorizontalRule { .. }));
    }

    #[test]
    fn test_not_horizontal_rule() {
        let tokens = lex_hr("-- not a rule");
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_hr_with_leading_spaces() {
        let tokens = lex_hr("   ---");
        assert!(matches!(tokens[0], Token::HorizontalRule { .. }));
    }

    #[test]
    fn test_hr_two_dashes() {
        let tokens = lex_hr("--");
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_hr_with_text_after() {
        let tokens = lex_hr("--- some text");
        assert!(matches!(tokens[0], Token::Text(_)));
    }
    #[test]
    fn test_this_aint_a_rule() {
        let tokens = lex_hr("- pesky unordered list");

        assert!(matches!(tokens[0], Token::Text(_)));
        print!("{:?}", tokens); // ul should be handled before that so this is a good fallback ig
    }
}

