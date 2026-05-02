use std::collections::VecDeque;
use logos::Logos;
use crate::markup::lexer::Token;

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
