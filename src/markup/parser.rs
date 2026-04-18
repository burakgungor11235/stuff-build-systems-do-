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
                let mut level: u8 = 0;

                while self.pos < self.input.len() {
                    let ch = self.input[self.pos];
                    if ch == '>' {
                        level += 1;
                        self.pos += 1;
                    } else if ch == ' ' {
                        // Look ahead to see if there is another '>' after the spaces
                        let mut next_is_gt = false;
                        let mut temp_pos = self.pos;
                        while temp_pos < self.input.len() {
                            if self.input[temp_pos] == ' ' {
                                temp_pos += 1;
                            } else if self.input[temp_pos] == '>' {
                                next_is_gt = true;
                                break;
                            } else {
                                break;
                            }
                        }

                        if next_is_gt {
                            self.pos = temp_pos;
                        } else {
                           self.pos += 1;
                            break;
                        }
                    } else {
                        break;
                    }
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
        let mut blocks = Vec::new();
        let mut current_paragraph = Vec::new();

        // Helper to commit accumulated inline text into a clean paragraph block
        let commit_paragraph = |p: &mut Vec<AstNode>, b: &mut Vec<AstNode>| {
            if !p.is_empty() {
                // Trim trailing newlines before committing
                while let Some(AstNode::Text(t)) = p.last() {
                    if t == "\n" {
                        p.pop();
                    } else {
                        break;
                    }
                }
                if !p.is_empty() {
                    b.push(AstNode::Paragraph(std::mem::take(p)));
                } else {
                    p.clear();
                }
            }
        };

        while let Some(tok) = self.peek().cloned() {
            if tok == Token::Eof {
                break;
            }

            match tok {
                Token::Blockquote(lvl) => {
                    commit_paragraph(&mut current_paragraph, &mut blocks);
                    self.advance();
                    let bq_nodes = self.parse_blockquote(lvl);
                    if !bq_nodes.is_empty() {
                        blocks.push(AstNode::Blockquote {
                            lvl,
                            children: bq_nodes,
                        });
                    }
                }
                Token::Heading(lvl) => {
                    commit_paragraph(&mut current_paragraph, &mut blocks);
                    self.advance();
                    self.skip_newlines();
                    let text = self.parse_text_line();
                    if !text.is_empty() {
                        // Optional trim to ensure headings are clean
                        blocks.push(AstNode::Heading(
                            lvl,
                            Box::new(AstNode::Text(text.trim().to_string())),
                        ));
                    }
                }
                Token::Newline => {
                    self.advance();
                    if self.peek() == Some(&Token::Newline) {
                        // Double newline commits the paragraph and resets
                        commit_paragraph(&mut current_paragraph, &mut blocks);
                        self.skip_newlines();
                    } else {
                        // Single newline inside a paragraph is treated as whitespace
                        current_paragraph.push(AstNode::Text("\n".to_string()));
                    }
                }
                _ => {
                    if let Some(node) = self.parse_inline() {
                        current_paragraph.push(node);
                    }
                }
            }
        }

        commit_paragraph(&mut current_paragraph, &mut blocks);
        blocks
    }

    // I would just like to say that I have blockquotes with a passion.
    // No implementation of mine ever works first try.
    // They are still cool tho. 
    fn parse_blockquote(&mut self, current_level: u8) -> Vec<AstNode> {
        let mut nodes = Vec::new();

        while let Some(tok) = self.peek().cloned() {
            if tok == Token::Eof {
                break;
            }

            match tok {
                Token::Blockquote(lvl) => {
                    if lvl < current_level {
                        // Hand back to outer layer
                        break;
                    } else if lvl > current_level {
                        // Greedily capture nested blockquotes
                        self.advance();
                        let nested = self.parse_blockquote(lvl);
                        nodes.push(AstNode::Blockquote {
                            lvl: lvl - current_level,
                            children: nested,
                        });
                    } else {
                        // Normal continuation
                        self.advance();
                    }
                }
                Token::Newline => {
                    self.advance();

                    // Double newline forces complete escape from blockquotes
                    if self.peek() == Some(&Token::Newline) {
                        break;
                    }

                    if let Some(Token::Blockquote(lvl)) = self.peek().cloned() {
                        if lvl < current_level {
                            break;
                        }
                        continue;
                    }

                    // Lazy continuation for standard text on a new line
                    nodes.push(AstNode::Text("\n".to_string()));
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
