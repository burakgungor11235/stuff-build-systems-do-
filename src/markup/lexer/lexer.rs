use crate::markup::lexer::default_handlers::default_line_handlers;
use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::inline::InlineLexer;
use crate::markup::lexer::sink::{TokenSink, TokenVec};
use crate::markup::lexer::span::StrSpan;
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct Lexer<'a> {
    input: &'a str,
    line_handlers: Vec<Box<dyn LineLexer>>,
    inline_lexer: InlineLexer,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            line_handlers: default_line_handlers(),
            inline_lexer: InlineLexer::new(),
        }
    }

    pub fn with_handlers(
        input: &'a str,
        line_handlers: Vec<Box<dyn LineLexer>>,
    ) -> Self {
        Self {
            input,
            line_handlers,
            inline_lexer: InlineLexer::new(),
        }
    }

    pub fn tokenize(&self) -> Vec<Token<'a>> {
        let mut sink = TokenVec::with_capacity(self.input.len() / 10);
        self.lex(&mut sink);
        
        // Process inline formatting on text tokens
        let tokens = sink.into_tokens();
        self.process_inline(tokens)
    }

    fn process_inline(&self, tokens: Vec<Token<'a>>) -> Vec<Token<'a>> {
        let mut result = Vec::with_capacity(tokens.len() * 2);
        
        for token in tokens {
            match token {
                Token::Text(span) => {
                    // First pass through inline lexer
                    let mut inline_sink1 = InlineTokenSink::new();
                    self.inline_lexer.lex(span.as_str(), &mut inline_sink1);
                    let first_pass = inline_sink1.into_tokens();
                    
                    // Second pass: re-process any text that might contain nested delimiters
                    let mut final_tokens = Vec::new();
                    for t in first_pass {
                        match t {
                            Token::Text(s) => {
                                let mut inline_sink2 = InlineTokenSink::new();
                                self.inline_lexer.lex(s.as_str(), &mut inline_sink2);
                                final_tokens.extend(inline_sink2.into_tokens());
                            }
                            _ => final_tokens.push(t),
                        }
                    }
                    
                    result.extend(final_tokens);
                }
                _ => {
                    result.push(token);
                }
            }
        }
        
        result
    }

    pub fn lex(&self, sink: &mut dyn TokenSink) {
        let mut state = LexerState::new();
        let mut prev_was_newline = true;

        for (_line_idx, line) in self.input.lines().enumerate() {
            state.reset_line_state();
            state.at_line_start = true;

            // Handle paragraph breaks (double newline)
            if prev_was_newline && !line.trim().is_empty() {
                // Start of new paragraph
            } else if prev_was_newline && line.trim().is_empty() {
                prev_was_newline = true;
                continue;
            } else if !prev_was_newline && line.trim().is_empty() {
                sink.emit(Token::ParagraphBreak);
                prev_was_newline = true;
                continue;
            }

            prev_was_newline = false;

            // Dispatch to appropriate line handler
            self.lex_line(line, &mut state, sink);

            // Emit newline after each line
            sink.emit(Token::Newline);
        }

        sink.emit(Token::EOF);
    }

    fn lex_line(&self, line: &'a str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let mut handlers = self.line_handlers.iter().collect::<Vec<_>>();
        handlers.sort_by(|a, b| b.priority().cmp(&a.priority()));

        for handler in handlers {
            if handler.can_handle(line, state) {
                handler.lex(line, state, sink);
                return;
            }
        }

        sink.emit(Token::Text(StrSpan::new(line)));
    }
}

struct InlineTokenSink {
    tokens: Vec<Token<'static>>,
}

impl InlineTokenSink {
    fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    fn into_tokens(self) -> Vec<Token<'static>> {
        self.tokens
    }
}

impl TokenSink for InlineTokenSink {
    fn emit(&mut self, token: Token<'_>) {
        let token_static = unsafe {
            std::mem::transmute::<Token<'_>, Token<'static>>(token)
        };
        self.tokens.push(token_static);
    }
}