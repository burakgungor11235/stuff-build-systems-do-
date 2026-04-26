use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::{Span, StrSpan};
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct DirectiveHandler;

impl LineLexer for DirectiveHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        state.at_line_start && line.trim_start().starts_with('@')
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let trimmed = line.trim_start();
        let leading_spaces = line.len() - trimmed.len();

        // Skip the @ prefix
        let after_at = if trimmed.starts_with('@') {
            &trimmed[1..]
        } else {
            trimmed
        };

        // Find the directive name
        let mut name_end = 0;
        for (i, ch) in after_at.char_indices() {
            if ch.is_alphanumeric() || ch == '_' {
                name_end = i + ch.len_utf8();
            } else {
                break;
            }
        }

        let name = &after_at[..name_end];
        let rest = &after_at[name_end..];

        // Parse parameters if present: (param1, param2, ...)
        let params = if rest.starts_with('(') {
            // Find matching closing paren
            let mut depth = 0;
            let mut param_end = 0;
            for (i, ch) in rest.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        if depth == 1 {
                            param_end = i;
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            
            if param_end > 0 {
                &rest[1..param_end]
            } else {
                ""
            }
        } else {
            ""
        };

        let full_span = Span::new(leading_spaces, leading_spaces + name_end + rest.len(), 1, leading_spaces + 1);
        let name_span = StrSpan::new(name);
        let params_span = StrSpan::new(params);

        sink.emit(Token::Directive {
            name: name_span,
            params: params_span,
            span: full_span,
        });
        state.clear_line_start();
    }

    fn priority(&self) -> u8 {
        160 // After chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_directive(input: &str) -> Vec<Token<'static>> {
        let handler = DirectiveHandler;
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
    fn test_directive_simple() {
        let tokens = lex_directive("@name");
        assert!(matches!(tokens[0], Token::Directive { name, .. } if name.as_str() == "name"));
    }

    #[test]
    fn test_directive_with_params() {
        let tokens = lex_directive("@name(param1, param2)");
        assert!(matches!(
            tokens[0],
            Token::Directive { name, params, .. } 
            if name.as_str() == "name" && params.as_str() == "param1, param2"
        ));
    }


    #[test]
    fn test_directive_with_leading_spaces() {
        let tokens = lex_directive("  @indented");
        assert!(matches!(tokens[0], Token::Directive { name, .. } if name.as_str() == "indented"));
    }

    #[test]
    fn test_directive_underscore_in_name() {
        let tokens = lex_directive("@my_directive");
        assert!(matches!(tokens[0], Token::Directive { name, .. } if name.as_str() == "my_directive"));
    }

    #[test]
    fn test_directive_no_params_empty_parens() {
        let tokens = lex_directive("@name()");
        assert!(matches!(tokens[0], Token::Directive { params, .. } if params.as_str().is_empty()));
    }

    #[test]
    fn test_directive_mixed_params() {
        let tokens = lex_directive("@func(&chunk, \"string\", 123, true)");
        assert!(matches!(tokens[0], Token::Directive { name, .. } if name.as_str() == "func"));
    }

    #[test]
    fn test_directive_no_name() {
        let tokens = lex_directive("@");
        // No name - what happens?
        assert!(!tokens.is_empty());
    }
}
