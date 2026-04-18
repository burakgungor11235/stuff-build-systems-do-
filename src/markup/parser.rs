// stuff we are going to support
//
// bold
// italic 
// strikethrough
// heading 
// blockquote

// Docs? hah! ask your AI for it :D 
// Honestly the most basic pipeline you've ever seen
// Lex -> Parse -> Render.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Bold,
    Italic,
    Strikethrough,
    Heading(u8),
    Blockquote,
    Text(String),
    Newline,
    Eof,
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while self.peek().is_some() {
            tokens.push(self.next_token());
        }
        tokens.push(Token::Eof);
        tokens
    }

    fn next_token(&mut self) -> Token {
        let Some(ch) = self.peek() else {
            return Token::Eof;
        };

        match ch {
            '*' => {
                self.advance();
                Token::Bold
            }
            '_' => {
                self.advance();
                Token::Italic
            }
            '~' => {
                self.advance();
                Token::Strikethrough
            }
            '#' => {
                self.advance();
                if let Some(c) = self.peek().and_then(|c| c.to_digit(10)) {
                    self.advance();
                    Token::Heading(c as u8)
                } else {
                    Token::Text("#".to_string())
                }
            }
            '>' => {
                self.advance();
                Token::Blockquote
            }
            '\n' => {
                self.advance();
                Token::Newline
            }
            _ => {
                self.advance();
                Token::Text(ch.to_string())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    Bold(Box<AstNode>),
    Italic(Box<AstNode>),
    Strikethrough(Box<AstNode>),
    Heading(u8, Box<AstNode>),
    Blockquote(Vec<AstNode>),
    Text(String),
    Paragraph(Vec<AstNode>),
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        self.tokens.get(self.pos).cloned()
    }
    
    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    pub fn parse(&mut self) -> Vec<AstNode> {
        let mut nodes = Vec::new();

        while self.peek() != Some(&Token::Eof) {
            match self.peek() {
                Some(Token::Blockquote) => {
                    self.advance();
                    let bq_nodes = self.parse_blockquote();
                    if !bq_nodes.is_empty() {
                        nodes.push(AstNode::Blockquote(bq_nodes));
                    }
                }
                Some(Token::Heading(level)) => {
                    let lvl = *level;
                    self.advance();
                    self.skip_newlines();
                    let text = self.parse_text_line();
                    if !text.is_empty() {
                        nodes.push(AstNode::Heading(lvl, Box::new(AstNode::Text(text))));
                    }
                }
                Some(Token::Newline) => {
                    self.advance();
                }
                _ => {
                    if let Some(node) = self.parse_inline() {
                        nodes.push(node);
                    }
                }
            }
        }

        if nodes.is_empty() {
            nodes
        } else {
            vec![AstNode::Paragraph(nodes)]
        }
    }

    fn parse_blockquote(&mut self) -> Vec<AstNode> {
        let mut nodes = Vec::new();

        while self.peek() != Some(&Token::Eof) {
            match self.peek() {
                Some(Token::Blockquote) => {
                    self.advance();
                    if let Some(node) = self.parse_inline() {
                        nodes.push(node);
                    }
                }
                Some(Token::Newline) => {
                    self.advance();
                    if self.peek() == Some(&Token::Blockquote) {
                        continue;
                    }
                    if self.is_blockquote_end() {
                        break;
                    }
                    if let Some(node) = self.parse_inline() {
                        nodes.push(node);
                    }
                }
                _ => {
                    if let Some(node) = self.parse_inline() {
                        nodes.push(node);
                    }
                }
            }
        }

        nodes
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Token::Newline) {
            self.advance();
        }
    }

    fn is_blockquote_end(&self) -> bool {
        matches!(self.peek(), Some(Token::Eof) | Some(Token::Heading(_)) | Some(Token::Bold) | Some(Token::Italic) | Some(Token::Strikethrough))
    }

    fn parse_text_line(&mut self) -> String {
        let mut text = String::new();
        while let Some(tok) = self.peek() {
            match tok {
                Token::Text(s) if s == "\n" => break,
                Token::Newline => break,
                Token::Eof => break,
                Token::Heading(_) => break,
                Token::Blockquote => break,
                _ => {
                    if let Token::Text(s) = tok {
                        text.push_str(s);
                    }
                    self.advance();
                }
            }
        }
        text
    }

    fn parse_inline(&mut self) -> Option<AstNode> {
        let mut nodes = Vec::new();

        while let Some(tok) = self.peek() {
            match tok {
                Token::Bold => {
                    self.advance();
                    let inner = self.parse_inline_content(Token::Bold);
                    nodes.push(AstNode::Bold(Box::new(inner)));
                }
                Token::Italic => {
                    self.advance();
                    let inner = self.parse_inline_content(Token::Italic);
                    nodes.push(AstNode::Italic(Box::new(inner)));
                }
                Token::Strikethrough => {
                    self.advance();
                    let inner = self.parse_inline_content(Token::Strikethrough);
                    nodes.push(AstNode::Strikethrough(Box::new(inner)));
                }
                Token::Text(s) => {
                    if s != "\n" {
                        nodes.push(AstNode::Text(s.clone()));
                    }
                    self.advance();
                }
                Token::Newline => {
                    break;
                }
                _ => {
                    self.advance();
                }
            }
        }

        if nodes.is_empty() {
            None
        } else if nodes.len() == 1 {
            Some(nodes.remove(0))
        } else {
            Some(AstNode::Paragraph(nodes))
        }
    }

    fn parse_inline_content(&mut self, end_token: Token) -> AstNode {
        let mut nodes = Vec::new();
        let mut text = String::new();

        while let Some(tok) = self.peek() {
            match tok {
                t if *t == end_token => {
                    self.advance();
                    break;
                }
                Token::Bold => {
                    self.advance();
                    let inner = self.parse_inline_content(Token::Bold);
                    nodes.push(AstNode::Bold(Box::new(inner)));
                }
                Token::Italic => {
                    self.advance();
                    let inner = self.parse_inline_content(Token::Italic);
                    nodes.push(AstNode::Italic(Box::new(inner)));
                }
                Token::Strikethrough => {
                    self.advance();
                    let inner = self.parse_inline_content(Token::Strikethrough);
                    nodes.push(AstNode::Strikethrough(Box::new(inner)));
                }
                Token::Text(s) => {
                    text.push_str(s);
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }

        if !text.is_empty() {
            nodes.insert(0, AstNode::Text(text));
        }

        if nodes.is_empty() {
            AstNode::Text(String::new())
        } else if nodes.len() == 1 {
            nodes.remove(0)
        } else {
            AstNode::Paragraph(nodes)
        }
    }
}

pub fn parse(input: &str) -> Vec<AstNode> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    parser.parse()
}
