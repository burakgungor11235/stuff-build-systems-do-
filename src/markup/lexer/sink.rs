use super::token::Token;

pub trait TokenSink {
    fn emit(&mut self, token: Token<'_>);
}

pub struct TokenVec {
    pub tokens: Vec<Token<'static>>,
}

impl TokenVec {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self { tokens: Vec::with_capacity(cap) }
    }

    pub fn push(&mut self, token: Token<'static>) {
        self.tokens.push(token);
    }

    pub fn into_tokens(self) -> Vec<Token<'static>> {
        self.tokens
    }
}

impl TokenSink for TokenVec {
    fn emit(&mut self, token: Token<'_>) {
        let token_static = unsafe {
            std::mem::transmute::<Token<'_>, Token<'static>>(token)
        };
        self.tokens.push(token_static);
    }
}

impl Default for TokenVec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::lexer::span::StrSpan;

    #[test]
    fn test_token_vec_emit() {
        let mut sink = TokenVec::new();
        sink.emit(Token::Text(StrSpan::new("hello")));
        assert_eq!(sink.tokens.len(), 1);
    }

    #[test]
    fn test_token_vec_into_tokens() {
        let mut sink = TokenVec::new();
        sink.emit(Token::Text(StrSpan::new("hello")));
        sink.emit(Token::Text(StrSpan::new("world")));
        
        let tokens = sink.into_tokens();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_token_vec_with_capacity() {
        let sink = TokenVec::with_capacity(100);
        assert!(sink.tokens.capacity() >= 100);
    }
}
