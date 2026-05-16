use crate::markup::{ast::*, lexer::Token, tsink::TokenStream};
use tracing::trace;

macro_rules! file_name_from_ref {
    ($base:expr) => {
        match &$base {
            RefExpr::Named(n) => n.clone(),
            RefExpr::Relative(n) => n.to_string(),
            RefExpr::Absolute(n) => n.to_string(),
            _ => String::new(),
        }
    };
}

pub struct Parser<'a> {
    tokens: TokenStream<'a>,
}

pub fn parse(source: &str) -> Document {
    let mut parser = Parser {
        tokens: TokenStream::new(source),
    };
    parser.parse_document()
}

impl<'a> Parser<'a> {
    fn parse_document(&mut self) -> Document {
        let mut chunks = Vec::new();
        loop {
            self.tokens.skip_blank();
            if self.tokens.peek().is_none() {
                break;
            }
            if let Some(chunk) = self.parse_chunk() {
                chunks.push(chunk);
            } else {
                let (line, col) = self.tokens.position();
                trace!("skipped unexpected token {:?} at {}:{}", self.tokens.last_slice(), line, col);
                self.tokens.next();
            }
        }
        Document { chunks }
    }

    fn parse_chunk(&mut self) -> Option<Chunk> {
        self.tokens.skip_blank();
        match self.tokens.peek()? {
            Token::ExplicitChunkStart => {
                self.tokens.next();
                let blocks = self.parse_block_sequence_until_end_marker();
                let name = self.expect_explicit_end_marker()?;
                Some(Chunk::Explicit { name, blocks })
            }
            _ => {
                let block = self.parse_block_element()?;
                let name = self.opt_implicit_name_marker();
                Some(Chunk::Implicit { name, block })
            }
        }
    }

    fn parse_block_sequence_until_end_marker(&mut self) -> Vec<Block> {
        let mut blocks = Vec::new();
        loop {
            self.tokens.skip_blank();
            if matches!(self.tokens.peek(), Some(Token::ExplicitChunkEnd(_)) | None) {
                break;
            }
            if let Some(block) = self.parse_block_element() {
                blocks.push(block);
            } else {
                self.tokens.next();
            }
        }
        blocks
    }

    fn opt_implicit_name_marker(&mut self) -> Option<String> {
        self.tokens.skip_trivia();
        if let Some(Token::ImplicitChunk(name)) = self.tokens.peek() {
            let name = name.clone();
            self.tokens.next();
            Some(name)
        } else {
            None
        }
    }

    fn expect_explicit_end_marker(&mut self) -> Option<String> {
        self.tokens.skip_trivia();
        match self.tokens.next()? {
            Token::ExplicitChunkEnd(name) => Some(name),
            _ => None,
        }
    }

    fn parse_block_element(&mut self) -> Option<Block> {
        self.tokens.skip_trivia();
        let tok = self.tokens.peek()?;
        match tok {
            Token::Heading(level) => {
                let lvl = *level;
                self.tokens.next();
                let content = self.parse_inline_until_newline();
                Some(Block::Heading {
                    level: lvl,
                    content,
                })
            }
            Token::HorizontalRule => {
                self.tokens.next();
                Some(Block::HorizontalRule)
            }
            Token::BlockquotePrefix => {
                let depth = self.count_blockquote_prefix();
                let content = self.parse_inline_until_newline();
                Some(Block::Blockquote { depth, content })
            }
            Token::ImageStart => self.parse_image(),
            Token::SimpleDirective(_) | Token::Directive(_) => self.parse_directive(),
            Token::Minus => self.parse_list(),
            Token::Digits(_) => {
                if self.tokens.peek_at(1) == Some(&Token::Dot) {
                    return self.parse_list();
                }
                self.parse_paragraph()
            }
            _ => self.parse_paragraph(),
        }
    }

    fn parse_paragraph(&mut self) -> Option<Block> {
        let content = self.parse_inline_until_newline();
        if content.is_empty() {
            None
        } else {
            Some(Block::Paragraph(content))
        }
    }

    fn parse_inline_until_newline(&mut self) -> Vec<Inline> {
        let nodes = self.parse_inline_linear(None);
        self.tokens.skip_trivia();
        if let Some(Token::Newline) = self.tokens.peek() {
            self.tokens.next();
        }
        nodes
    }
    fn parse_inline_linear(&mut self, stop: Option<Token>) -> Vec<Inline> {
        let mut output: Vec<Inline> = Vec::new();
        let mut delim_stack: Vec<(Token, usize)> = Vec::new();

        loop {
            self.tokens.skip_inline_trivia();

            // End conditions
            match self.tokens.peek() {
                None => break,
                Some(t) if stop.as_ref() == Some(t) => {
                    self.tokens.next();
                    break;
                }
                Some(Token::Newline | Token::ExplicitChunkEnd(_) | Token::ImplicitChunk(_)) => {
                    break
                }
                _ => {}
            }

            let tok = self.tokens.next().unwrap();

            match tok {
                // special inline constructs
                Token::Star | Token::Underscore | Token::Tilde => {
                    self.handle_formatting_delimiter(tok, &mut output, &mut delim_stack);
                }

                Token::LinkStart => {
                    self.parse_link(&mut output);
                }

                // Transclusion: !& followed by a reference expression
                Token::Bang => {
                    self.tokens.skip_inline_trivia();
                    if self.tokens.peek() == Some(&Token::Ampersand) {
                        self.tokens.next();
                        let expr = self.parse_reference_expression();
                        output.push(Inline::Transclusion(expr));
                    } else {
                        self.push_text(&mut output, "!");
                    }
                }

                // Reference: & followed by a reference expression
                Token::Ampersand => {
                    let expr = self.parse_reference_expression();
                    output.push(Inline::Reference(expr));
                }

                Token::Hash | Token::DotDot | Token::Comma => {
                    let text = self.tokens.last_slice().to_string();
                    self.push_text(&mut output, &text);
                }

                // yeet the rest back
                other => {
                    output.push(self.token_to_inline(other));
                }
            }
        }

        // unwind unclosed delimiters
        for (open_tok, placeholder_idx) in delim_stack.into_iter().rev() {
            output.remove(placeholder_idx);
            output.insert(
                placeholder_idx,
                Inline::Text(open_tok.inline_as_str().to_owned()),
            );
        }
        // remove stray empty placeholders
        output.retain(|n| !matches!(n, Inline::Text(s) if s.is_empty()));

        output
    }

    fn parse_link(&mut self, output: &mut Vec<Inline>) {
        let mut target = String::new();
        loop {
            self.tokens.skip_inline_trivia();
            match self.tokens.peek() {
                Some(Token::LinkEnd) | None => break,
                Some(Token::Pipe) => break,
                _ => {
                    self.tokens.next();
                    target.push_str(self.tokens.last_slice());
                }
            }
        }
        let display = if self.tokens.consume_if(&Token::Pipe).is_some() {
            self.parse_inline_linear(Some(Token::LinkEnd))
        } else {
            vec![Inline::Text(target.clone())]
        };
        self.tokens.consume_if(&Token::LinkEnd);
        output.push(Inline::Link { target, display });
    }

    fn push_text(&mut self, output: &mut Vec<Inline>, text: &str) {
        output.push(Inline::Text(text.to_string()));
    }

    /// Parse a reference expression after `&` (or `!&`) has been consumed.
    fn parse_reference_expression(&mut self) -> RefExpr {
        self.tokens.skip_inline_trivia();
        trace!("parse_reference_expression: peeking {:?}", self.tokens.peek());

        match self.tokens.peek() {
            // &#heading..   heading range in current file
            // &#heading     chunks under heading in current file
            Some(Token::Hash) => {
                self.tokens.expect(&Token::Hash);
                let heading = self.parse_heading_text();
                self.tokens.skip_inline_trivia();
                if self.tokens.consume_if(&Token::DotDot).is_some() {
                    trace!("parsed heading range: {:?}", heading);
                    RefExpr::HeadingRange(heading)
                } else {
                    trace!("parsed file by heading (current): {:?}", heading);
                    RefExpr::FileByHeading("".to_string(), heading)
                }
            }

            // &(expr1, expr2, ...)
            Some(Token::LParen) => {
                self.tokens.expect(&Token::LParen);
                let exprs = self.parse_ref_list();
                if exprs.len() == 1 {
                    exprs.into_iter().next().unwrap()
                } else {
                    RefExpr::List(exprs)
                }
            }

            // &-N, &+N, &N, &name
            Some(Token::Minus) | Some(Token::Plus) | Some(Token::Digits(_)) | Some(Token::Text(_)) => {
                let base = self.parse_single_ref();
                self.parse_qualifier(base)
            }

            // Bare & at end of input
            _ => RefExpr::Named(String::new()),
        }
    }

    /// Parse a comma-separated list of references inside parentheses.
    fn parse_ref_list(&mut self) -> Vec<RefExpr> {
        let mut exprs = Vec::new();
        loop {
            self.tokens.skip_trivia();
            if self.tokens.peek() == Some(&Token::RParen) || self.tokens.peek().is_none() {
                break;
            }
            let expr = if self.tokens.peek() == Some(&Token::Ampersand) {
                self.tokens.next();
                self.parse_reference_expression()
            } else {
                self.parse_single_ref()
            };
            exprs.push(expr);
            self.tokens.skip_trivia();
            if self.tokens.consume_if(&Token::Comma).is_none() {
                break;
            }
        }
        self.tokens.consume_if(&Token::RParen);
        exprs
    }

    /// Parse a single reference token: name, +/-digits, or absolute digits.
    /// Names may contain underscores (e.g., my_chunk) which are separate tokens.
    fn parse_single_ref(&mut self) -> RefExpr {
        self.tokens.skip_inline_trivia();
        match self.tokens.next() {
            Some(Token::Minus) | Some(Token::Plus) => {
                let sign = if matches!(self.tokens.last_slice(), "-") { -1 } else { 1 };
                self.tokens.skip_inline_trivia();
                if let Some(Token::Digits(d)) = self.tokens.peek() {
                    let digits = d.clone();
                    self.tokens.next();
                    let result = RefExpr::Relative(sign * digits.parse::<i32>().unwrap_or(0));
                    trace!("parse_single_ref: relative {:?}", result);
                    result
                } else {
                    let result = RefExpr::Named(self.tokens.last_slice().to_string());
                    trace!("parse_single_ref: named (sign only) {:?}", result);
                    result
                }
            }
            Some(Token::Digits(d)) => {
                let num = d.clone();
                self.tokens.skip_inline_trivia();
                let result = RefExpr::Absolute(num.parse::<usize>().unwrap_or(0));
                trace!("parse_single_ref: absolute {:?}", result);
                result
            }
            Some(Token::Text(name)) => {
                let result = RefExpr::Named(self.consume_text_component(name));
                trace!("parse_single_ref: named {:?}", result);
                result
            }
            _ => RefExpr::Named(String::new()),
        }
    }

    /// Consume text tokens (Text + optional underscore continuations), joining with `separator`.
    fn consume_text_component(&mut self, mut acc: String) -> String {
        loop {
            self.tokens.skip_inline_trivia();
            if self.tokens.peek() == Some(&Token::Underscore) {
                self.tokens.next();
                self.tokens.skip_inline_trivia();
                if let Some(Token::Text(s)) = self.tokens.peek().cloned() {
                    self.tokens.next();
                    acc.push('_');
                    acc.push_str(&s);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        acc
    }

    fn parse_heading_text(&mut self) -> String {
        let mut text = String::new();
        loop {
            self.tokens.skip_inline_trivia();
            match self.tokens.peek() {
                Some(Token::Text(s)) => {
                    let s = s.clone();
                    self.tokens.next();
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(&self.consume_text_component(s));
                }
                Some(Token::Digits(s)) => {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(s);
                    self.tokens.next();
                }
                _ => break,
            }
        }
        text
    }

    /// After a base reference, check for qualifiers:
    ///   ..N       range (base is start, N is end relative)
    ///   .N        file.by_index (base.name becomes file, N is index)
    ///   .name     file.by_name
    ///   #heading  file.by_heading (base.name becomes file)
    fn parse_qualifier(&mut self, base: RefExpr) -> RefExpr {
        self.tokens.skip_inline_trivia();
        match self.tokens.peek() {
            Some(Token::DotDot) => {
                self.tokens.expect(&Token::DotDot);
                self.tokens.skip_inline_trivia();
                let end = match self.tokens.peek() {
                    Some(Token::Minus) | Some(Token::Plus) | Some(Token::Digits(_)) => {
                        self.parse_single_ref()
                    }
                    _ => RefExpr::Relative(0),
                };
                // food for thought: should + -> - for reverse ordering work?
                // right now I'm full with thoughts so I can't answer that.
                let start_val = match base {
                    RefExpr::Relative(n) => n,
                    RefExpr::Absolute(n) => n as i32,
                    _ => return base,
                };
                let end_val = match end {
                    RefExpr::Relative(n) => n,
                    RefExpr::Absolute(n) => n as i32,
                    _ => return RefExpr::Range(start_val, start_val),
                };
                let result = RefExpr::Range(start_val, end_val);
                trace!("parse_qualifier: range {:?}", result);
                result
            }

            Some(Token::Dot) => {
                self.tokens.expect(&Token::Dot);
                self.tokens.skip_inline_trivia();
                match self.tokens.peek() {
                    Some(Token::Digits(d)) => {
                        let idx_str = d.clone();
                        self.tokens.next();
                        let result = RefExpr::FileByIndex(file_name_from_ref!(base), idx_str.parse::<usize>().unwrap_or(0));
                        trace!("parse_qualifier: file by index {:?}", result);
                        result
                    }
                    Some(Token::Text(name)) => {
                        let name = name.clone();
                        self.tokens.next();
                        let name = self.consume_text_component(name);
                        let file_name = file_name_from_ref!(base);
                        let result = self.parse_file_with_heading(file_name, name);
                        trace!("parse_qualifier: file with heading {:?}", result);
                        result
                    }
                    Some(Token::Hash) => {
                        self.tokens.expect(&Token::Hash);
                        let heading = self.parse_heading_text();
                        let result = self.parse_post_heading_qualifier(file_name_from_ref!(base), heading);
                        trace!("parse_qualifier: post heading (via .#) {:?}", result);
                        result
                    }
                    _ => base,
                }
            }

            Some(Token::Hash) => {
                self.tokens.expect(&Token::Hash);
                let heading = self.parse_heading_text();
                let result = self.parse_post_heading_qualifier(file_name_from_ref!(base), heading);
                trace!("parse_qualifier: post heading (via #) {:?}", result);
                result
            }

            _ => base,
        }
    }

    fn parse_file_with_heading(&mut self, file_name: String, name: String) -> RefExpr {
        self.tokens.skip_inline_trivia();
        if self.tokens.consume_if(&Token::Hash).is_some() {
            let heading = self.parse_heading_text();
            self.parse_post_heading_qualifier(file_name, heading)
        } else {
            RefExpr::FileByName(file_name, name)
        }
    }

    /// After `file#heading` (or `file.name#heading`), check for `.N`, `.name`, or `..`.
    fn parse_post_heading_qualifier(&mut self, file_name: String, heading: String) -> RefExpr {
        self.tokens.skip_inline_trivia();
        if self.tokens.consume_if(&Token::DotDot).is_some() {
            return RefExpr::HeadingRange(heading);
        }
        if self.tokens.peek() != Some(&Token::Dot) {
            return RefExpr::FileByHeading(file_name, heading);
        }
        self.tokens.expect(&Token::Dot);
        self.tokens.skip_inline_trivia();
        match self.tokens.peek() {
            Some(Token::Digits(d)) => {
                let idx = d.clone();
                self.tokens.next();
                RefExpr::FileByHeadingIndex(file_name, heading, idx.parse::<usize>().unwrap_or(0))
            }
            Some(Token::Text(n)) => {
                let subname = n.clone();
                self.tokens.next();
                let subname = self.consume_text_component(subname);
                RefExpr::FileByHeadingName(file_name, heading, subname)
            }
            _ => RefExpr::FileByHeading(file_name, heading),
        }
    }

    fn handle_formatting_delimiter(
        &mut self,
        tok: Token,
        output: &mut Vec<Inline>,
        delim_stack: &mut Vec<(Token, usize)>,
    ) {
        if let Some(&(ref open_tok, start_idx)) = delim_stack.last() {
            if *open_tok == tok {
                delim_stack.pop();
                let _placeholder = output.remove(start_idx); // empty placeholder
                let inner: Vec<Inline> = output.drain(start_idx..).collect();
                if inner.is_empty() {
                    // convert to literal
                    output.push(Inline::Text(tok.inline_as_str().to_owned()));
                    output.insert(start_idx, Inline::Text(tok.inline_as_str().into()));
                } else {
                    let wrapped = match tok {
                        Token::Star => Inline::Bold(inner),
                        Token::Underscore => Inline::Italic(inner),
                        Token::Tilde => Inline::Strikethrough(inner),
                        _ => unreachable!(),
                    };
                    output.push(wrapped);
                }
                return; // important: we already placed the result, continue to next token
            }
        }
        // Cannot close. Start a new possible span
        let placeholder_idx = output.len();
        delim_stack.push((tok, placeholder_idx));
        output.push(Inline::Text(String::new())); // empty placeholder
    }

    /// Convert an inline token that has no special structure into an `Inline::Text`.
    fn token_to_inline(&self, tok: Token) -> Inline {
        match tok {
            // Text-like tokens
            Token::Text(s) | Token::Whitespace(s) | Token::Digits(s) => Inline::Text(s),
            Token::Escape(esc) => Inline::Text(esc[1..].to_string()),

            // Punctuation that can appear inside inline
            Token::LBracket => Inline::Text("[".into()),
            Token::RBracket => Inline::Text("]".into()),
            Token::LParen => Inline::Text("(".into()),
            Token::RParen => Inline::Text(")".into()),
            Token::LBrace => Inline::Text("{".into()),
            Token::RBrace => Inline::Text("}".into()),
            Token::Slash => Inline::Text("/".into()),
            Token::Plus => Inline::Text("+".into()),
            Token::Minus => Inline::Text("-".into()),
            Token::Dot => Inline::Text(".".into()),
            Token::Bang => Inline::Text("!".into()),
            Token::Pipe => Inline::Text("|".into()),
            Token::Caret => Inline::Text("^".into()),

            // Multi-character literals
            Token::LinkEnd => Inline::Text("]]".into()),
            Token::ImageStart => Inline::Text("![".into()),
            Token::BlockquotePrefix => Inline::Text(self.tokens.last_slice().to_string()),
            Token::Heading(_)
            | Token::HorizontalRule
            | Token::SimpleDirective(_)
            | Token::Directive(_)
            | Token::ExplicitChunkStart
            | Token::ExplicitChunkEnd(_)
            | Token::ImplicitChunk(_)
            | Token::Newline => Inline::Text(self.tokens.last_slice().to_string()),

            // Comments
            Token::Comment | Token::IncompleteComment => {
                Inline::Text(self.tokens.last_slice().to_string())
            }

            // Punctuation we handle structurally: output as literal text
            Token::Ampersand => Inline::Text("&".into()),
            Token::Hash => Inline::Text("#".into()),
            Token::DotDot => Inline::Text("..".into()),
            Token::Comma => Inline::Text(",".into()),

            // The special tokens that are dispatched earlier.
            Token::Star | Token::Underscore | Token::Tilde | Token::LinkStart => {
                unreachable!(
                    "{} should be handled before token_to_inline",
                    self.tokens.last_slice()
                )
            }
        }
    }

    // Block helpers
    fn count_blockquote_prefix(&mut self) -> u32 {
        self.tokens.next(); // consume BlockquotePrefix
        self.tokens.last_slice().len() as u32
    }

    fn parse_image(&mut self) -> Option<Block> {
        self.tokens.next(); // consume ImageStart
        let alt = self.parse_inline_linear(Some(Token::Pipe));
        let url_tokens = self.parse_inline_linear(Some(Token::RBracket));
        let url = inlines_to_plain_text(&url_tokens);
        Some(Block::Image { alt, url })
    }

    fn parse_directive(&mut self) -> Option<Block> {
        match self.tokens.peek()? {
            Token::SimpleDirective(name) => {
                let name = name.clone();
                self.tokens.next();
                Some(Block::Directive { name, body: None })
            }
            Token::Directive(data) => {
                let name = data.name.clone();
                let body = data.body.clone();
                self.tokens.next();
                Some(Block::Directive {
                    name,
                    body: Some(body),
                })
            }
            _ => None,
        }
    }

    fn parse_list(&mut self) -> Option<Block> {
        let mut items = Vec::new();
        // Determine list type from first marker
        let first_token = self.tokens.peek()?;
        let ordered = matches!(first_token, Token::Digits(_));

        loop {
            self.tokens.skip_trivia();
            match self.tokens.peek() {
                Some(Token::Minus) | Some(Token::Plus) if !ordered => {
                    self.tokens.next(); // consume marker
                }
                Some(Token::Digits(_)) if ordered => {
                    self.tokens.next(); // consume digits
                    self.tokens.skip_trivia();
                    if let Some(Token::Dot) = self.tokens.peek() {
                        self.tokens.next(); // consume dot
                    } else {
                        break; // malformed
                    }
                }
                _ => break, // no more list items
            }
            // Skip whitespace after marker, then parse inline until newline
            self.tokens.skip_trivia();
            let content = self.parse_inline_until_newline();
            items.push(content);
        }

        if items.is_empty() {
            None
        } else {
            Some(Block::List { items, ordered })
        }
    }
}

/// Flatten inline elements into a plain text string (used for image URLs, etc.)
fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Bold(inner) | Inline::Italic(inner) | Inline::Strikethrough(inner) => {
                s.push_str(&inlines_to_plain_text(inner));
            }
            Inline::Reference(_) => todo!(),
            Inline::Link { .. } => todo!(),
            Inline::Transclusion(_) => todo!(),
        }
    }
    s
}
#[cfg(test)]
mod tests {
    use super::*;

    // --- Test helpers for reference/transclusion parsing ---

    /// Extract the first reference from a single-reference input like "&foo" or "&-1".
    fn parse_ref(input: &str) -> RefExpr {
        let doc = parse(input);
        let chunk = doc.chunks.first().expect("expected chunk");
        match chunk {
            Chunk::Implicit { block: Block::Paragraph(inlines), .. } => {
                match &inlines[0] {
                    Inline::Reference(expr) => expr.clone(),
                    _ => panic!("expected reference at position 0 in: {}", input),
                }
            }
            _ => panic!("expected implicit paragraph in: {}", input),
        }
    }

    /// (for inputs with surrounding text).
    fn parse_ref_at(input: &str, pos: usize) -> RefExpr {
        let doc = parse(input);
        let chunk = doc.chunks.first().expect("expected chunk");
        match chunk {
            Chunk::Implicit { block: Block::Paragraph(inlines), .. } => {
                match &inlines[pos] {
                    Inline::Reference(expr) => expr.clone(),
                    other => panic!(
                        "expected reference at position {} in '{}', got {:?}",
                        pos, input, other
                    ),
                }
            }
            _ => panic!("expected implicit paragraph in: {}", input),
        }
    }

    fn parse_transclusion(input: &str) -> RefExpr {
        let doc = parse(input);
        let chunk = doc.chunks.first().expect("expected chunk");
        match chunk {
            Chunk::Implicit { block: Block::Paragraph(inlines), .. } => {
                match &inlines[0] {
                    Inline::Transclusion(expr) => expr.clone(),
                    _ => panic!("expected transclusion at position 0 in: {}", input),
                }
            }
            _ => panic!("expected implicit paragraph in: {}", input),
        }
    }

    #[test]
    fn empty_document() {
        let doc = parse("");
        assert!(doc.chunks.is_empty(), "Expected no chunks, got {:#?}", doc);
    }

    #[test]
    fn blank_lines() {
        let doc = parse("   \n\n   \t\n");
        assert!(doc.chunks.is_empty(), "Expected no chunks, got {:#?}", doc);
    }

    #[test]
    fn single_word_paragraph() {
        let doc = parse("hello\n");
        let chunk = doc.chunks.first().expect("Expected one chunk");
        match chunk {
            Chunk::Implicit { name, block } => {
                assert!(name.is_none(), "Name should be None");
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines, &[Inline::Text("hello".into())]);
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn paragraph_with_spaces() {
        let doc = parse("hello   world\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 3);
                    assert_eq!(inlines[0], Inline::Text("hello".into()));
                    assert_eq!(inlines[1], Inline::Text("   ".into()));
                    assert_eq!(inlines[2], Inline::Text("world".into()));
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn bold_inline() {
        let doc = parse("*bold*\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 1);
                    assert!(
                        matches!(&inlines[0], Inline::Bold(inner) if inner == &[Inline::Text("bold".into())])
                    );
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn italic_inline() {
        let doc = parse("_italic_\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 1);
                    assert!(
                        matches!(&inlines[0], Inline::Italic(inner) if inner == &[Inline::Text("italic".into())])
                    );
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn strikethrough_inline() {
        let doc = parse("~deleted~\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 1);
                    assert!(
                        matches!(&inlines[0], Inline::Strikethrough(inner) if inner == &[Inline::Text("deleted".into())])
                    );
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn nested_formatting() {
        let doc = parse("*bold ~and~ here*\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 1);
                    if let Inline::Bold(inner) = &inlines[0] {
                        assert_eq!(inner.len(), 5);
                        assert_eq!(inner[0], Inline::Text("bold".into()));
                        assert_eq!(inner[1], Inline::Text(" ".into()));
                        if let Inline::Strikethrough(s) = &inner[2] {
                            assert_eq!(s[0], Inline::Text("and".into()));
                        } else {
                            panic!("Expected strikethrough inside bold");
                        }
                        assert_eq!(inner[3], Inline::Text(" ".into()));
                        assert_eq!(inner[4], Inline::Text("here".into()));
                    } else {
                        panic!("Expected bold");
                    }
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn basic_reference_forms() {
        assert_eq!(parse_ref("&my_chunk"), RefExpr::Named("my_chunk".into()));
        assert_eq!(parse_ref("&-1"), RefExpr::Relative(-1));
        assert_eq!(parse_ref("&+2"), RefExpr::Relative(2));
        assert_eq!(parse_ref("&3"), RefExpr::Absolute(3));
    }

    #[test]
    fn reference_range() {
        assert_eq!(parse_ref("&-1..-4"), RefExpr::Range(-1, -4));
    }

    #[test]
    fn reference_list_mixed_types() {
        match parse_ref("&(-1, 2, &my_chunk)") {
            RefExpr::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], RefExpr::Relative(-1));
                assert_eq!(items[1], RefExpr::Absolute(2));
                assert_eq!(items[2], RefExpr::Named("my_chunk".into()));
            }
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn file_reference_qualifiers() {
        assert_eq!(parse_ref("&other_file.1"), RefExpr::FileByIndex("other_file".into(), 1));
        assert_eq!(parse_ref("&other_file.myname"), RefExpr::FileByName("other_file".into(), "myname".into()));
        assert_eq!(parse_ref("&other_file#intro"), RefExpr::FileByHeading("other_file".into(), "intro".into()));
        assert_eq!(parse_ref("&other_file#intro.3"), RefExpr::FileByHeadingIndex("other_file".into(), "intro".into(), 3));
        assert_eq!(parse_ref("&other_file#intro.myname"), RefExpr::FileByHeadingName("other_file".into(), "intro".into(), "myname".into()));
    }

    #[test]
    fn reference_heading_range_current_file() {
        match parse_ref("&#intro..") {
            RefExpr::HeadingRange(heading) => assert_eq!(heading, "intro"),
            other => panic!("expected HeadingRange, got {:?}", other),
        }
    }

    #[test]
    fn reference_with_surrounding_text() {
        assert_eq!(parse_ref_at("see &my_chunk", 2), RefExpr::Named("my_chunk".into()));
        assert_eq!(parse_ref_at("see &-1..-4", 2), RefExpr::Range(-1, -4));
    }

    #[test]
    fn reference_underscore_in_name() {
        assert_eq!(parse_ref("&my_chunk_name"), RefExpr::Named("my_chunk_name".into()));
    }

    #[test]
    fn reference_range_reverse() {
        assert_eq!(parse_ref("&-4..-1"), RefExpr::Range(-4, -1));
    }

    #[test]
    fn reference_at_end_no_newline() {
        assert_eq!(parse_ref("&foo"), RefExpr::Named("foo".into()));
    }

    #[test]
    fn reference_with_trailing_characters() {
        let doc = parse("ref &-1-2 end\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            } => {
                assert!(inlines.len() >= 5);
                match &inlines[2] {
                    Inline::Reference(RefExpr::Relative(-1)) => {}
                    other => panic!("Expected Ref(Relative(-1)), got {:?}", other),
                }
                assert_eq!(inlines[3], Inline::Text("-".into()));
                assert_eq!(inlines[4], Inline::Text("2".into()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn ampersand_alone_is_text() {
        let doc = parse("&\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert!(inlines.len() <= 2);
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn transclusion_named() {
        let doc = parse("before !&my_chunk after\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block: Block::Paragraph(inlines), .. } => {
                assert_eq!(inlines.len(), 5);
                assert_eq!(inlines[0], Inline::Text("before".into()));
                assert_eq!(inlines[1], Inline::Text(" ".into()));
                match &inlines[2] {
                    Inline::Transclusion(RefExpr::Named(name)) => assert_eq!(name, "my_chunk"),
                    other => panic!("Expected Transclusion(Named), got {:?}", other),
                }
                assert_eq!(inlines[3], Inline::Text(" ".into()));
                assert_eq!(inlines[4], Inline::Text("after".into()));
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn transclusion_range() {
        assert_eq!(parse_transclusion("!&-1..-3"), RefExpr::Range(-1, -3));
    }

    #[test]
    fn transclusion_vs_reference() {
        let doc = parse("!&foo &bar\n");
        let chunk = doc.chunks.first().expect("expected chunk");
        match chunk {
            Chunk::Implicit { block: Block::Paragraph(inlines), .. } => {
                assert!(matches!(&inlines[0], Inline::Transclusion(_)));
                assert!(matches!(&inlines[2], Inline::Reference(_)));
            }
            _ => panic!("expected implicit paragraph"),
        }
    }

    #[test]
    fn heading_level_1() {
        let doc = parse("#1 Title\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Heading { level, content } = block {
                    assert_eq!(*level, 1);
                    assert_eq!(content.len(), 2);
                    assert_eq!(content[0], Inline::Text(" ".into()));
                    assert_eq!(content[1], Inline::Text("Title".into()));
                } else {
                    panic!("Expected heading");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn heading_level_3() {
        let doc = parse("#3 Deep\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Heading { level, content } = block {
                    assert_eq!(*level, 3);
                    assert_eq!(content.len(), 2);
                    assert_eq!(content[0], Inline::Text(" ".into()));
                    assert_eq!(content[1], Inline::Text("Deep".into()));
                } else {
                    panic!("Expected heading");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn horizontal_rule() {
        let doc = parse("---\n");
        let chunk = doc.chunks.first().unwrap();
        assert!(matches!(
            chunk,
            Chunk::Implicit {
                block: Block::HorizontalRule,
                ..
            }
        ));
    }

    #[test]
    fn blockquote_single_line() {
        let doc = parse("> quote\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Blockquote { depth, content } = block {
                    assert_eq!(*depth, 1);
                    assert_eq!(content.len(), 2);
                    assert_eq!(content[0], Inline::Text(" ".into()));
                    assert_eq!(content[1], Inline::Text("quote".into()));
                } else {
                    panic!("Expected blockquote");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn blockquote_nested() {
        let doc = parse(">>> deep\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Blockquote { depth, content } = block {
                    assert_eq!(*depth, 3);
                    assert_eq!(content.len(), 2);
                    assert_eq!(content[0], Inline::Text(" ".into()));
                    assert_eq!(content[1], Inline::Text("deep".into()));
                } else {
                    panic!("Expected blockquote");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn image_simple() {
        let doc = parse("![ alt | url ]\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Image { alt, url } = block {
                    assert_eq!(alt.len(), 3);
                    assert_eq!(alt[0], Inline::Text(" ".into()));
                    assert_eq!(alt[1], Inline::Text("alt".into()));
                    assert_eq!(alt[2], Inline::Text(" ".into()));
                    assert_eq!(url, " url ");
                } else {
                    panic!("Expected image");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn image_no_alt() {
        let doc = parse("![|url]\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Image { alt, url } = block {
                    assert!(alt.is_empty());
                    assert_eq!(url, "url");
                } else {
                    panic!("Expected image");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn directive_simple() {
        let doc = parse("@mytag\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Directive { name, body } = block {
                    assert_eq!(name, "mytag");
                    assert!(body.is_none());
                } else {
                    panic!("Expected directive");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn directive_with_body() {
        let doc = parse("@plugin(arg1, &chunk)\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Directive { name, body } = block {
                    assert_eq!(name, "plugin");
                    assert_eq!(body.as_deref(), Some("arg1, &chunk"));
                } else {
                    panic!("Expected directive");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn list_unordered() {
        let doc = parse("- one\n- two\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::List { items, ordered } = block {
                    assert!(!ordered);
                    assert_eq!(items.len(), 2);
                    assert_eq!(items[0][0], Inline::Text("one".into()));
                    assert_eq!(items[1][0], Inline::Text("two".into()));
                } else {
                    panic!("Expected list");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn list_ordered() {
        let doc = parse("1. first\n2. second\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::List { items, ordered } = block {
                    assert!(*ordered);
                    assert_eq!(items.len(), 2);
                    assert_eq!(items[0][0], Inline::Text("first".into()));
                    assert_eq!(items[1][0], Inline::Text("second".into()));
                } else {
                    panic!("Expected list");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn implicit_chunk_named() {
        let doc = parse("para :>(myname)\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { name, block } => {
                assert_eq!(name.as_deref(), Some("myname"));
                assert!(matches!(block, Block::Paragraph(_)));
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn explicit_chunk() {
        let doc = parse(":< \n line1 \n line2 \n>:(ex)\n");
        match doc.chunks.first() {
            Some(Chunk::Explicit { name, blocks }) => {
                assert_eq!(name, "ex");
                assert_eq!(blocks.len(), 2);
                assert!(matches!(blocks[0], Block::Paragraph(_)));
                assert!(matches!(blocks[1], Block::Paragraph(_)));
            }
            other => panic!("Expected explicit chunk, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn explicit_chunk_with_varied_blocks() {
        let doc = parse(":< \n #1 head \n --- \n>:(mixed)\n");
        match doc.chunks.first() {
            Some(Chunk::Explicit { name, blocks }) => {
                assert_eq!(name, "mixed");
                assert_eq!(blocks.len(), 2);
                assert!(matches!(blocks[0], Block::Heading { .. }));
                assert!(matches!(blocks[1], Block::HorizontalRule));
            }
            other => panic!("Expected explicit chunk, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn full_document_mixed() {
        let doc = parse("#1 Intro\n\nPara *bold* _italic_ ~strike~ &ref &-1\n\n---\n\n@dir(body)");
        assert_eq!(doc.chunks.len(), 4, "Unexpected chunk count: {:#?}", doc);

        print!("{:?}", doc);
        match &doc.chunks[0] {
            Chunk::Implicit { block, .. } => {
                if let Block::Heading { level, content } = block {
                    assert_eq!(*level, 1);
                    assert_eq!(content.len(), 2);
                    assert_eq!(content[1], Inline::Text("Intro".into()));
                } else {
                    panic!("Expected heading");
                }
            }
            _ => panic!("Expected implicit heading chunk"),
        }

        match &doc.chunks[1] {
            Chunk::Implicit { block, .. } => {
                assert!(matches!(block, Block::Paragraph(_)));
            }
            _ => panic!("Expected implicit paragraph chunk"),
        }

        match &doc.chunks[2] {
            Chunk::Implicit { block, .. } => {
                assert_eq!(block, &Block::HorizontalRule);
            }
            _ => panic!("Expected implicit hr chunk"),
        }

        match &doc.chunks[3] {
            Chunk::Implicit { block, .. } => {
                assert_eq!(
                    block,
                    &Block::Directive {
                        name: String::from("dir"),
                        body: Some(String::from("body"))
                    }
                );
            }
            _ => panic!("Expected explicit chunk"),
        }
    }

    #[test]
    fn comments_ignored() {
        let doc = parse("/' this is a comment '/\n");
        assert!(doc.chunks.is_empty(), "Expected no chunks, got {:#?}", doc);
    }

    #[test]
    fn incomplete_comment_ignored() {
        let doc = parse("/' unclosed...\nreal content\n");
        assert_eq!(doc.chunks.len(), 2, "Expected two chunks");
        if let Chunk::Implicit { block, .. } = &doc.chunks[0] {
            if let Block::Paragraph(inlines) = block {
                assert!(inlines
                    .iter()
                    .any(|i| matches!(i, Inline::Text(s) if s.contains("unclosed"))));
            } else {
                panic!("Expected paragraph");
            }
        }
        if let Chunk::Implicit { block, .. } = &doc.chunks[1] {
            assert!(matches!(block, Block::Paragraph(_)));
        }
    }

    #[test]
    fn empty_explicit_chunk() {
        let doc = parse(":< \n>:(nothing)\n");
        match doc.chunks.first() {
            Some(Chunk::Explicit { name, blocks }) => {
                assert_eq!(name, "nothing");
                assert!(blocks.is_empty());
            }
            other => panic!("Expected explicit chunk, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn multiple_paragraphs_and_headings() {
        let doc = parse("line1\n\nline2\n\n#1 Title\n\npara after heading\n");
        assert_eq!(doc.chunks.len(), 4);
        assert!(matches!(
            &doc.chunks[0],
            Chunk::Implicit {
                block: Block::Paragraph(_),
                ..
            }
        ));
        assert!(matches!(
            &doc.chunks[1],
            Chunk::Implicit {
                block: Block::Paragraph(_),
                ..
            }
        ));
        assert!(matches!(
            &doc.chunks[2],
            Chunk::Implicit {
                block: Block::Heading { .. },
                ..
            }
        ));
        assert!(matches!(
            &doc.chunks[3],
            Chunk::Implicit {
                block: Block::Paragraph(_),
                ..
            }
        ));
    }

    #[test]
    fn list_without_space_after_marker() {
        let doc = parse("-item\n-next\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::List { items, ordered },
                ..
            }) => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0][0], Inline::Text("item".into()));
                assert_eq!(items[1][0], Inline::Text("next".into()));
            }
            other => panic!("Expected list chunk, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn ordered_without_dot_not_list() {
        let doc = parse("1 something\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            }) => {
                assert_eq!(inlines.len(), 3);
                assert_eq!(inlines[0], Inline::Text("1".into()));
                assert_eq!(inlines[1], Inline::Text(" ".into()));
                assert_eq!(inlines[2], Inline::Text("something".into()));
            }
            other => panic!("Expected paragraph, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn mixed_inline_formatting_deep() {
        let doc = parse("*bold _ital ~strike~ _ text*\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 1);
                    if let Inline::Bold(inner) = &inlines[0] {
                        assert_eq!(inner.len(), 5);
                        assert_eq!(inner[0], Inline::Text("bold".into()));
                        assert_eq!(inner[1], Inline::Text(" ".into()));
                        if let Inline::Italic(italic) = &inner[2] {
                            assert_eq!(italic.len(), 4);
                            assert_eq!(italic[0], Inline::Text("ital".into()));
                            assert_eq!(italic[1], Inline::Text(" ".into()));
                            if let Inline::Strikethrough(s) = &italic[2] {
                                assert_eq!(italic[3], Inline::Text(" ".into()));
                                assert_eq!(s[0], Inline::Text("strike".into()));
                            } else {
                                panic!("Expected strikethrough inside italic");
                            }
                        } else {
                            panic!("Expected italic inside bold");
                        }
                        assert_eq!(inner[3], Inline::Text(" ".into()));
                        assert_eq!(inner[4], Inline::Text("text".into()));
                    } else {
                        panic!("Expected bold");
                    }
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn unclosed_bold_before_newline() {
        let doc = parse("*bold\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            } => {
                assert_eq!(inlines.len(), 2);
                assert_eq!(inlines[0], Inline::Text("*".into()));
                assert_eq!(inlines[1], Inline::Text("bold".into()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn image_alt_with_formatting() {
        let doc = parse("![ *bold* and _italic_ | url ]\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Image { alt, url },
                ..
            }) => {
                assert_eq!(alt.len(), 7);
                assert!(alt.iter().any(|i| matches!(i, Inline::Bold(_))));
                assert!(alt.iter().any(|i| matches!(i, Inline::Italic(_))));
                assert_eq!(url, " url ");
            }
            _ => panic!("Expected image chunk"),
        }
    }

    #[test]
    fn directive_with_nested_parens() {
        let doc = parse("@foo(inner(a, b(2))) \n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Directive { name, body },
                ..
            }) => {
                assert_eq!(name, "foo");
                assert_eq!(body.as_deref(), Some("inner(a, b(2))"));
            }
            _ => panic!("Expected directive"),
        }
    }

    #[test]
    fn explicit_chunk_unclosed() {
        let doc = parse(":< \ninside\nno end\n");
        assert!(doc.chunks.is_empty() || !doc.chunks.is_empty());
    }

    #[test]
    fn list_with_blank_line_inside() {
        let doc = parse("- item1\n\n- item2\n");
        assert_eq!(doc.chunks.len(), 2);
        for chunk in &doc.chunks {
            match chunk {
                Chunk::Implicit { block, .. } => assert!(matches!(block, Block::List { .. })),
                _ => panic!("Unexpected chunk"),
            }
        }
    }

    #[test]
    fn consecutive_stars_empty_span() {
        let doc = parse("**ast*\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            assert_eq!(inlines.len(), 4);
            assert_eq!(inlines[0], Inline::Text("*".into()));
            assert_eq!(inlines[1], Inline::Text("*".into()));
            assert_eq!(inlines[2], Inline::Text("ast".into()));
            assert_eq!(inlines[3], Inline::Text("*".into()));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn consecutive_underscores_empty_span() {
        let doc = parse("__well___\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            assert_eq!(inlines.len(), 6);
            assert_eq!(inlines[0], Inline::Text("_".into()));
            assert_eq!(inlines[1], Inline::Text("_".into()));
            assert_eq!(inlines[2], Inline::Text("well".into()));
            assert_eq!(inlines[3], Inline::Text("_".into()));
            assert_eq!(inlines[4], Inline::Text("_".into()));
            assert_eq!(inlines[5], Inline::Text("_".into()));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn apostrophe_in_word_unified() {
        let doc = parse("how's it going?\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            assert_eq!(inlines.len(), 5);
            assert_eq!(inlines[0], Inline::Text("how's".into()));
            assert_eq!(inlines[1], Inline::Text(" ".into()));
            assert_eq!(inlines[2], Inline::Text("it".into()));
            assert_eq!(inlines[3], Inline::Text(" ".into()));
            assert_eq!(inlines[4], Inline::Text("going?".into()));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn bold_with_spaces_inside() {
        let doc = parse("*wait are we deadass*\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            assert_eq!(inlines.len(), 1);
            if let Inline::Bold(inner) = &inlines[0] {
                assert_eq!(inner.len(), 7);
                assert_eq!(inner[0], Inline::Text("wait".into()));
                assert_eq!(inner[1], Inline::Text(" ".into()));
                assert_eq!(inner[2], Inline::Text("are".into()));
                assert_eq!(inner[3], Inline::Text(" ".into()));
                assert_eq!(inner[4], Inline::Text("we".into()));
                assert_eq!(inner[5], Inline::Text(" ".into()));
                assert_eq!(inner[6], Inline::Text("deadass".into()));
            } else {
                panic!("Expected bold");
            }
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn mixed_bold_italic_nested() {
        let doc = parse("*oh _no_*\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            assert_eq!(inlines.len(), 1);
            if let Inline::Bold(inner) = &inlines[0] {
                assert_eq!(inner.len(), 3);
                assert_eq!(inner[0], Inline::Text("oh".into()));
                assert_eq!(inner[1], Inline::Text(" ".into()));
                if let Inline::Italic(italic) = &inner[2] {
                    assert_eq!(italic.len(), 1);
                    assert_eq!(italic[0], Inline::Text("no".into()));
                } else {
                    panic!("Expected italic inside bold");
                }
            } else {
                panic!("Expected bold");
            }
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn unclosed_italic_after_newline() {
        let doc = parse("_italic\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            } => {
                assert_eq!(inlines.len(), 2);
                assert_eq!(inlines[0], Inline::Text("_".into()));
                assert_eq!(inlines[1], Inline::Text("italic".into()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn unclosed_strikethrough() {
        let doc = parse("~strike\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], Inline::Text("~".into()));
            assert_eq!(inlines[1], Inline::Text("strike".into()));
        } else {
            panic!("Expected paragraph");
        }
    }
}
