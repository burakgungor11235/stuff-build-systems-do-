use crate::markup::lexer::Token;
use logos::Logos;
use std::collections::VecDeque;

pub struct TokenStream<'a> {
    lexer: logos::Lexer<'a, Token>,
    buf: VecDeque<(Token, &'a str)>,
    last_slice: &'a str,
}

impl<'a> TokenStream<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            lexer: Token::lexer(source),
            buf: VecDeque::new(),
            last_slice: "",
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
                    self.last_slice = self.lexer.slice();
                    Some(tok)
                }
                Err(_) => {
                    self.last_slice = "";
                    None
                }
            }
        } else {
            let (tok, slice) = self.buf.pop_front().unwrap();
            self.last_slice = slice;
            Some(tok)
        }
    }

    pub fn last_slice(&self) -> &str {
        self.last_slice
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
mod tests {
    use super::*;
    use logos::Logos;

    /// Collect all tokens with their source slices.
    fn lex(input: &str) -> Vec<(Token, String)> {
        let mut lexer = Token::lexer(input);
        let mut tokens = Vec::new();
        while let Some(result) = lexer.next() {
            let slice = lexer.slice().to_string();
            match result {
                Ok(token) => tokens.push((token, slice)),
                Err(()) => tokens.push((Token::Text(slice.clone()), slice)), // should never happen
            }
        }
        tokens
    }

    /// Lex and assert that the concatenated slices equal the original input.
    fn assert_lossless(input: &str) {
        let mut lexer = Token::lexer(input);
        let mut reconstructed = String::new();
        while let Some(result) = lexer.next() {
            match result {
                Ok(_) | Err(()) => reconstructed.push_str(lexer.slice()),
            }
        }
        assert_eq!(input, reconstructed, "Lossless round-trip failed");
    }

// Basic tokens
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
        match &tokens[0] {
            (Token::Text(s), slice) => {
                assert_eq!(s, "hello");
                assert_eq!(slice, "hello");
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
        let tokens = lex("#1 #2 #99");
        // Three headings with spaces/text between? Actually "#1 " is heading then whitespace then "#2" etc.
        // Let's lex "#1 Title" to be precise.
        let tokens = lex("#1 Title");
        assert!(matches!(tokens[0].0, Token::Heading(1)));
        assert!(matches!(tokens[1].0, Token::Whitespace(_)));
        assert!(matches!(tokens[2].0, Token::Text(ref s) if s == "Title"));
    }

    #[test]
    fn heading_no_level_zero() {
        let tokens = lex("#0 not a heading");
        // `#0` should be Text because regex only matches #[1-9][0-9]*
        print!("{:?}", tokens);
        assert!(matches!(tokens[0].0, Token::Text(ref s) if s == "#0"));
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
                assert_eq!(data.body, "bar baz"); // everything after '('
            }
            _ => panic!("Expected Directive with rest of input as body"),
        }
        // Also verify losslessness
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
    fn reference_named() {
        let tokens = lex("&chunk");
        match &tokens[0].0 {
            Token::Reference(r) => assert_eq!(r, "chunk"),
            _ => panic!("Expected Reference"),
        }
    }

    #[test]
    fn reference_previous() {
        let tokens = lex("&-1");
        assert!(matches!(&tokens[0].0, Token::Reference(r) if r == "-1"));
    }

    #[test]
    fn reference_next() {
        let tokens = lex("&+2");
        assert!(matches!(&tokens[0].0, Token::Reference(r) if r == "+2"));
    }

    #[test]
    fn reference_absolute() {
        let tokens = lex("&42");
        assert!(matches!(&tokens[0].0, Token::Reference(r) if r == "42"));
    }

    #[test]
    fn ampersand_alone_is_text() {
        let tokens = lex("&");
        // Not part of a reference pattern, should be Text
        assert!(matches!(tokens[0].0, Token::Text(ref s) if s == "&"));
    }

    #[test]
    fn reference_then_dash_number() {
        let tokens = lex("&-1-2");
        // Should be Ref("-1"), then Minus, then Digits("2")
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0].0, Token::Reference(r) if r == "-1"));
        assert!(matches!(tokens[1].0, Token::Minus));
        assert!(matches!(&tokens[2].0, Token::Digits(d) if d == "2"));
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
        assert!(matches!(tokens[1].0, Token::Text(_))); // "alt"
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
        // Digit "1" then Dot
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
        // The regex allows `'` not followed by `/`
        let tokens = lex("/' it's okay '/");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Comment));
    }

    #[test]
    fn unclosed_comment_marker_then_text() {
        let input = "/' hello";
        let tokens = lex(input);
        assert!(matches!(tokens[0].0, Token::IncompleteComment));
        // remaining tokens: whitespace? Actually input: "/' hello" -> IncompleteComment, then Whitespace, then Text
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
        // `![` should be ImageStart, not `!` + `[`
        let tokens = lex("![");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::ImageStart));
    }

    #[test]
    fn directive_priority_over_simple() {
        // @foo( should be Directive, not SimpleDirective + LParen
        let tokens = lex("@foo(");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Directive(_)));
    }

    #[test]
    fn reference_priority_over_ampersand_as_text() {
        // &-1 must be Reference, not Text("&") + Minus + Digit
        let tokens = lex("&-1");
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].0, Token::Reference(_)));
    }

    #[test]
    fn digits_not_text() {
        // 123 should be Digits, not Text
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
