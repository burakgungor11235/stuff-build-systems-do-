//
// bold
// italic
// strikethrough
// heading
// blockquote

// Docs? hah! ask your AI for it :D
// Honestly the most basic pipeline you've ever seen
// Lex -> Parse -> Render.
//
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Bold,
    Italic,
    Strikethrough,
    Heading(u8),
    Blockquote(u8), // level
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

    fn consume_text(&mut self) -> String {
        let mut s = String::new();

        while let Some(ch) = self.peek() {
            match ch {
                '*' | '_' | '~' | '#' | '>' | '\n' => break,
                _ => {
                    s.push(ch);
                    self.advance();
                }
            }
        }

        s
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
                let mut level = 0;

                while self.peek() == Some('>') {
                    self.advance();
                    level += 1;
                }

                Token::Blockquote(level.min(6) as u8) 
            }
            '\n' => {
                self.advance();
                Token::Newline
            }
            _ => {
                let text = self.consume_text();
                Token::Text(text)
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
    Blockquote { lvl: u8, children: Vec<AstNode> },
    Text(String),
    Paragraph(Vec<AstNode>),
}

impl AstNode {
    pub fn to_str(&self) -> String {
        self.to_str_indented(0)
    }

    fn to_str_indented(&self, indent: usize) -> String {
        let pad = "  ".repeat(indent);

        match self {
            AstNode::Bold(node) => {
                format!("{}Bold\n{}", pad, node.to_str_indented(indent + 1))
            }
            AstNode::Italic(node) => {
                format!("{}Italic\n{}", pad, node.to_str_indented(indent + 1))
            }
            AstNode::Strikethrough(node) => {
                format!("{}Strikethrough\n{}", pad, node.to_str_indented(indent + 1))
            }
            AstNode::Heading(level, node) => {
                format!(
                    "{}Heading(level={})\n{}",
                    pad,
                    level,
                    node.to_str_indented(indent + 1)
                )
            }
            AstNode::Blockquote { lvl, children } => {
                let mut out = format!("{}Blockquote(lvl={lvl})\n", pad);

                for n in children {
                    out.push_str(&n.to_str_indented(indent + 1));
                    out.push('\n');
                }

                out
            }
            AstNode::Paragraph(nodes) => {
                let mut out = format!("{}Paragraph\n", pad);

                for n in nodes {
                    out.push_str(&n.to_str_indented(indent + 1));
                    out.push('\n');
                }

                out
            }
            AstNode::Text(text) => {
                format!("{}Text({:?})", pad, text)
            }
        }
    }
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
                Some(Token::Blockquote(lvl)) => {
                    let level = *lvl;
                    self.advance();
                    // Pass expected level so we can isolate different blockquotes properly
                    let bq_nodes = self.parse_blockquote(level);
                    if !bq_nodes.is_empty() {
                        nodes.push(AstNode::Blockquote {
                            lvl: level,
                            children: bq_nodes,
                        });
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

    fn parse_blockquote(&mut self, current_level: u8) -> Vec<AstNode> {
        let mut nodes = Vec::new();

        while self.peek() != Some(&Token::Eof) {
            match self.peek() {
                Some(Token::Blockquote(lvl)) => {
                    // If the nested level differentiates, break the blockquote container!
                    if *lvl != current_level {
                        break;
                    }
                    self.advance();
                    if let Some(node) = self.parse_inline() {
                        nodes.push(node);
                    }
                }
                Some(Token::Newline) => {
                    self.advance();
                    // Peek at the NEXT token after new line
                    if let Some(Token::Blockquote(lvl)) = self.peek() {
                        if *lvl == current_level {
                            continue;
                        } else {
                            break;
                        }
                    }
                    if self.is_blockquote_end() {
                        break;
                    }
                    if let Some(node) = self.parse_inline() {
                        nodes.push(node);
                    }
                }
                _ => {
                    if self.is_blockquote_end() {
                        break;
                    }
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
        matches!(
            self.peek(),
            Some(Token::Eof)
                | Some(Token::Heading(_))
                | Some(Token::Bold)
                | Some(Token::Italic)
                | Some(Token::Strikethrough)
        )
    }

    fn parse_text_line(&mut self) -> String {
        let mut text = String::new();
        while let Some(tok) = self.peek() {
            match tok {
                Token::Text(s) if s == "\n" => break,
                Token::Newline => break,
                Token::Eof => break,
                Token::Heading(_) => break,
                Token::Blockquote(lvl) => {
                    text.push_str(&">".repeat(*lvl as usize));
                    self.advance();
                }
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
                Token::Blockquote(lvl) => {
                    // Escape inline blockquote tokens that slip in (E.g. parsing `> >`)
                    let l = *lvl;
                    self.advance();
                    nodes.push(AstNode::Text(">".repeat(l as usize)));
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
                Token::Blockquote(lvl) => {
                    // Make sure they act as escaped characters inside formatted tags
                    text.push_str(&">".repeat(*lvl as usize));
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
