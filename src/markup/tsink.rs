use crate::markup::lexer::Token;
use logos::Logos;
use std::collections::VecDeque;

pub struct TokenStream<'a> {
    lexer: logos::Lexer<'a, Token>,
    buf: VecDeque<(Token, &'a str)>,
    last_slice: &'a str,
    line: u32,
    column: u32,
}

impl<'a> TokenStream<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            lexer: Token::lexer(source),
            buf: VecDeque::new(),
            last_slice: "",
            line: 1,
            column: 1,
        }
    }

    /// Current (line, column) position in the source.
    pub fn position(&self) -> (u32, u32) {
        (self.line, self.column)
    }

    fn update_position(&mut self, slice: &str) {
        for ch in slice.chars() {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }

    /// Make sure at least `n` tokens are in the lookahead buffer.
    fn fill(&mut self, n: usize) {
        while self.buf.len() < n {
            match self.lexer.next() {
                Some(Ok(tok)) => self.buf.push_back((tok, self.lexer.slice())),
                _ => break,
            }
        }
    }

    pub fn peek(&mut self) -> Option<&Token> {
        self.fill(1);
        self.buf.front().map(|(tok, _)| tok)
    }

    /// Peek at the token `offset` positions ahead (0 = current).
    pub fn peek_at(&mut self, offset: usize) -> Option<&Token> {
        self.fill(offset + 1);
        self.buf.get(offset).map(|(tok, _)| tok)
    }

    pub fn next(&mut self) -> Option<Token> {
        if self.buf.is_empty() {
            match self.lexer.next()? {
                Ok(tok) => {
                    let slice = self.lexer.slice();
                    self.update_position(slice);
                    self.last_slice = slice;
                    Some(tok)
                }
                Err(_) => {
                    self.last_slice = "";
                    None
                }
            }
        } else {
            let (tok, slice) = self.buf.pop_front().unwrap();
            self.update_position(slice);
            self.last_slice = slice;
            Some(tok)
        }
    }

    pub fn last_slice(&self) -> &str {
        self.last_slice
    }

    /// Consume the next token if it matches the expected token.
    pub fn consume_if(&mut self, expected: &Token) -> Option<Token> {
        if self.peek() == Some(expected) {
            self.next()
        } else {
            None
        }
    }

    /// Consume the next token, panicking with position info if it doesn't match.
    /// Use when you've already verified the token via `peek()`.
    pub fn expect(&mut self, expected: &Token) -> Token {
        let (line, col) = self.position();
        match self.next() {
            Some(tok) => tok,
            None => panic!("expected {:?} at line {}:{} but got EOF", expected, line, col),
        }
    }

    pub fn skip_trivia(&mut self) {
        while let Some(tok) = self.peek() {
            match tok {
                Token::Whitespace(_) | Token::Comment | Token::IncompleteComment => {
                    self.next();
                }
                _ => break,
            }
        }
    }

    pub fn skip_inline_trivia(&mut self) {
        while let Some(tok) = self.peek() {
            match tok {
                Token::Comment | Token::IncompleteComment => {
                    self.next();
                }
                _ => break,
            }
        }
    }

    pub fn skip_blank(&mut self) {
        loop {
            self.skip_trivia();
            if let Some(Token::Newline) = self.peek() {
                self.next();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod lex_helpers {
    use super::*;

    /// Collect all tokens with their source slices.
    pub fn lex(input: &str) -> Vec<(Token, String)> {
        let mut lexer = Token::lexer(input);
        let mut tokens = Vec::new();
        while let Some(result) = lexer.next() {
            let slice = lexer.slice().to_string();
            match result {
                Ok(token) => tokens.push((token, slice)),
                Err(()) => tokens.push((Token::Text(slice.clone()), slice)),
            }
        }
        tokens
    }

    /// Lex and assert that the concatenated slices equal the original input.
    pub fn assert_lossless(input: &str) {
        let mut lexer = Token::lexer(input);
        let mut reconstructed = String::new();
        while let Some(result) = lexer.next() {
            match result {
                Ok(_) | Err(()) => reconstructed.push_str(lexer.slice()),
            }
        }
        assert_eq!(input, reconstructed, "Lossless round-trip failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::lex_helpers::{assert_lossless, lex};

    #[test]
    fn empty_input() {
        let tokens = lex("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn only_whitespace() {
        let tokens = lex("   \t  ");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Whitespace(_)));
    }

    #[test]
    fn text_single_word() {
        let tokens = lex("hello");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].0 {
            Token::Text(s) => {
                assert_eq!(s, "hello");
                assert_eq!(&tokens[0].1, "hello");
            }
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn newline() {
        let tokens = lex("\n");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Newline));
    }

    #[test]
    fn heading_levels() {
        let tokens = lex("#1 Title");
        assert!(matches!(tokens[0].0, Token::Heading(1)));
        assert!(matches!(tokens[1].0, Token::Whitespace(_)));
        assert!(matches!(tokens[2].0, Token::Text(ref s) if s == "Title"));
    }

    #[test]
    fn heading_no_level_zero() {
        let tokens = lex("#0 not a heading");
        assert!(matches!(tokens[0].0, Token::Hash));
        assert!(matches!(tokens[1].0, Token::Digits(ref s) if s == "0"));
        assert!(matches!(tokens[2].0, Token::Whitespace(_)));
        assert!(matches!(tokens[3].0, Token::Text(ref s) if s == "not"));
        assert!(matches!(tokens[4].0, Token::Whitespace(_)));
        assert!(matches!(tokens[5].0, Token::Text(ref s) if s == "a"));
        assert!(matches!(tokens[6].0, Token::Whitespace(_)));
        assert!(matches!(tokens[7].0, Token::Text(ref s) if s == "heading"));
    }

    #[test]
    fn horizontal_rule() {
        let tokens = lex("---");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::HorizontalRule));
    }

    #[test]
    fn not_horizontal_rule() {
        let tokens = lex("--");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(tokens[0].0, Token::Minus));
        assert!(matches!(tokens[1].0, Token::Minus));
    }

    #[test]
    fn blockquote_single() {
        let tokens = lex(">");
        assert!(matches!(tokens[0].0, Token::BlockquotePrefix));
    }

    #[test]
    fn blockquote_multiple() {
        let tokens = lex(">>>");
        assert!(matches!(tokens[0].0, Token::BlockquotePrefix));
    }

    #[test]
    fn star_and_underscore() {
        let tokens = lex("*_~");
        assert!(matches!(tokens[0].0, Token::Star));
        assert!(matches!(tokens[1].0, Token::Underscore));
        assert!(matches!(tokens[2].0, Token::Tilde));
    }

    #[test]
    fn simple_directive() {
        let tokens = lex("@foo");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].0 {
            Token::SimpleDirective(name) => assert_eq!(name, "foo"),
            _ => panic!("Expected SimpleDirective"),
        }
    }

    #[test]
    fn directive_with_body() {
        let tokens = lex("@foo(arg)");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].0 {
            Token::Directive(data) => {
                assert_eq!(data.name, "foo");
                assert_eq!(data.body, "arg");
            }
            _ => panic!("Expected Directive"),
        }
    }

    #[test]
    fn directive_nested_parens() {
        let tokens = lex("@a(b(c,d))");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].0 {
            Token::Directive(data) => {
                assert_eq!(data.name, "a");
                assert_eq!(data.body, "b(c,d)");
            }
            _ => panic!("Expected Directive"),
        }
    }

    #[test]
    fn directive_unclosed_swallows_rest() {
        let tokens = lex("@foo(bar baz");
        assert_eq!(tokens.len(), 1, "Must be a single directive token");
        match &tokens[0].0 {
            Token::Directive(data) => {
                assert_eq!(data.name, "foo");
                assert_eq!(data.body, "bar baz");
            }
            _ => panic!("Expected Directive with rest of input as body"),
        }
        assert_lossless("@foo(bar baz");
    }

    #[test]
    fn directive_unclosed_empty_parens() {
        let tokens = lex("@foo(");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].0 {
            Token::Directive(data) => {
                assert_eq!(data.name, "foo");
                assert_eq!(data.body, "");
            }
            _ => panic!("Expected Directive"),
        }
    }

    #[test]
    fn ampersand_is_separate_token() {
        let tokens = lex("&my_chunk");
        assert!(matches!(tokens[0].0, Token::Ampersand));
        assert!(matches!(tokens[1].0, Token::Text(ref s) if s == "my"));
        assert!(matches!(tokens[2].0, Token::Underscore));
        assert!(matches!(tokens[3].0, Token::Text(ref s) if s == "chunk"));
    }

    #[test]
    fn dotdot_is_separate_token() {
        let tokens = lex("..");
        assert!(matches!(tokens[0].0, Token::DotDot));
    }

    #[test]
    fn comma_is_separate_token() {
        let tokens = lex(",");
        assert!(matches!(tokens[0].0, Token::Comma));
    }

    #[test]
    fn hash_is_separate_token() {
        let tokens = lex("#heading");
        assert!(matches!(tokens[0].0, Token::Hash));
        assert!(matches!(tokens[1].0, Token::Text(ref s) if s == "heading"));
    }

    #[test]
    fn explicit_chunk_start() {
        let tokens = lex(":<");
        assert!(matches!(tokens[0].0, Token::ExplicitChunkStart));
    }

    #[test]
    fn explicit_chunk_end() {
        let tokens = lex(">:(name)");
        match &tokens[0].0 {
            Token::ExplicitChunkEnd(n) => assert_eq!(n, "name"),
            _ => panic!("Expected ExplicitChunkEnd"),
        }
    }

    #[test]
    fn implicit_chunk_marker() {
        let tokens = lex(":>(myname)");
        match &tokens[0].0 {
            Token::ImplicitChunk(n) => assert_eq!(n, "myname"),
            _ => panic!("Expected ImplicitChunk"),
        }
    }

    #[test]
    fn image_start() {
        let tokens = lex("![");
        assert!(matches!(tokens[0].0, Token::ImageStart));
    }

    #[test]
    fn image_full() {
        let input = "![alt | url]";
        let tokens = lex(input);
        assert!(matches!(tokens[0].0, Token::ImageStart));
        assert_lossless(input);
    }

    #[test]
    fn digits() {
        let tokens = lex("123");
        assert!(matches!(&tokens[0].0, Token::Digits(d) if d == "123"));
    }

    #[test]
    fn dot_is_separate() {
        let tokens = lex("1.");
        assert_eq!(tokens.len(), 2);
        assert!(matches!(&tokens[0].0, Token::Digits(d) if d == "1"));
        assert!(matches!(tokens[1].0, Token::Dot));
    }

    #[test]
    fn minus_as_list_marker() {
        let tokens = lex("-");
        assert!(matches!(tokens[0].0, Token::Minus));
    }

    #[test]
    fn plus_as_list_marker() {
        let tokens = lex("+");
        assert!(matches!(tokens[0].0, Token::Plus));
    }

    #[test]
    fn complete_comment() {
        let tokens = lex("/' simple '/");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Comment));
    }

    #[test]
    fn comment_with_apostrophe() {
        let tokens = lex("/' it's okay '/");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Comment));
    }

    #[test]
    fn unclosed_comment_marker_then_text() {
        let input = "/' hello";
        let tokens = lex(input);
        assert!(matches!(tokens[0].0, Token::IncompleteComment));
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[1].0, Token::Whitespace(_)));
        assert!(matches!(tokens[2].0, Token::Text(ref s) if s == "hello"));
        assert_lossless(input);
    }

    #[test]
    fn escape_star() {
        let tokens = lex(r"\*");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].0 {
            Token::Escape(esc) => assert_eq!(esc, r"\*"),
            _ => panic!("Expected Escape"),
        }
    }

    #[test]
    fn escape_backslash_itself() {
        let tokens = lex(r"\\");
        assert!(matches!(&tokens[0].0, Token::Escape(esc) if esc == r"\\"));
    }

    #[test]
    fn escape_at_sign() {
        let tokens = lex(r"\@");
        assert!(matches!(&tokens[0].0, Token::Escape(esc) if esc == r"\@"));
    }

    #[test]
    fn escape_ampersand() {
        let tokens = lex(r"\&");
        assert!(matches!(&tokens[0].0, Token::Escape(esc) if esc == r"\&"));
    }

    #[test]
    fn image_start_priority_over_bang_bracket() {
        let tokens = lex("![");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::ImageStart));
    }

    #[test]
    fn directive_priority_over_simple() {
        let tokens = lex("@foo(");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Directive(_)));
    }

    #[test]
    fn ampersand_alone_is_ampersand_token() {
        let tokens = lex("&");
        assert!(matches!(tokens[0].0, Token::Ampersand));
    }

    #[test]
    fn digits_not_text() {
        let tokens = lex("123");
        assert!(matches!(tokens[0].0, Token::Digits(_)));
    }

    #[test]
    fn unicode_text() {
        let tokens = lex("🦀🎯");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0].0, Token::Text(s) if s == "🦀🎯"));
    }

    #[test]
    fn lossless_round_trip_mixed() {
        // Note: & is now its own token, so the full text still round-trips losslessly
        let input = "#1 Intro!\n\n*Bold* _italic_ ~strike~ &ref &-1\n\n---\n\n@dir(body)";
        assert_lossless(input);
    }

    #[test]
    fn lossless_with_escapes() {
        let input = r"\* not bold \@not_a_dir \& not a ref";
        assert_lossless(input);
    }

    #[test]
    fn lossless_unclosed_comment() {
        let input = "/' this is not closed\nnext line";
        assert_lossless(input);
    }
}