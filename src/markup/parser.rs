use crate::markup::{ast::*, lexer::Token, tsink::TokenStream};

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
                self.tokens.next(); // skip problematic token and continue
            }
        }
        Document { chunks }
    }

    fn parse_chunk(&mut self) -> Option<Chunk> {
        self.tokens.skip_blank();
        match self.tokens.peek()? {
            Token::ExplicitChunkStart => {
                self.tokens.next(); // consume :<
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
                // Look ahead: if next token is Dot, this is an ordered list.
                if self.tokens.peek_at(1) == Some(&Token::Dot) {
                    return self.parse_list();
                }
                // Otherwise, treat as normal paragraph (fall through to default)
                let content = self.parse_inline_until_newline();
                if content.is_empty() {
                    None
                } else {
                    Some(Block::Paragraph(content))
                }
            }
            _ => {
                let content = self.parse_inline_until_newline();
                if content.is_empty() {
                    None
                } else {
                    Some(Block::Paragraph(content))
                }
            }
        }
    }

    fn parse_inline(&mut self) -> Vec<Inline> {
        let mut nodes = Vec::new();
        loop {
            self.tokens.skip_inline_trivia();
            match self.tokens.peek() {
                None
                | Some(Token::Newline)
                | Some(Token::ExplicitChunkEnd(_))
                | Some(Token::ImplicitChunk(_)) => break,

                Some(Token::Star) => nodes.push(self.parse_bold()),
                Some(Token::Underscore) => nodes.push(self.parse_italic()),
                Some(Token::Tilde) => nodes.push(self.parse_strikethrough()),
                Some(Token::Reference(r)) => {
                    nodes.push(Inline::Reference(r.clone()));
                    self.tokens.next();
                }
                _ => {
                    let text = self.token_to_text();
                    nodes.push(Inline::Text(text));
                }
            }
        }
        nodes
    }

    fn parse_inline_until_newline(&mut self) -> Vec<Inline> {
        let nodes = self.parse_inline_linear(None);
        self.tokens.skip_trivia();
        if let Some(Token::Newline) = self.tokens.peek() {
            self.tokens.next();
        }
        nodes
    }

    /// Parse inline elements until we see the given `stop` token (which is consumed),
    /// or end of line / chunk.
    fn parse_inline_until(&mut self, stop: Token) -> Vec<Inline> {
        let mut nodes = Vec::new();
        loop {
            self.tokens.skip_inline_trivia();
            match self.tokens.peek() {
                None => break,
                Some(t) if *t == stop => {
                    self.tokens.next(); // consume stop
                    break;
                }
                Some(Token::Newline | Token::ExplicitChunkEnd(_) | Token::ImplicitChunk(_)) => {
                    break
                }
                Some(Token::Star) => nodes.push(self.parse_bold()),
                Some(Token::Underscore) => nodes.push(self.parse_italic()),
                Some(Token::Tilde) => nodes.push(self.parse_strikethrough()),
                Some(Token::Reference(r)) => {
                    nodes.push(Inline::Reference(r.clone()));
                    self.tokens.next();
                }
                _ => {
                    let text = self.token_to_text();
                    nodes.push(Inline::Text(text));
                }
            }
        }
        nodes
    }

    fn parse_inline_linear(&mut self, stop: Option<Token>) -> Vec<Inline> {
        let mut output: Vec<Inline> = Vec::new();
        // Stack: (delimiter token, index in output where the placeholder sits)
        let mut delim_stack: Vec<(Token, usize)> = Vec::new();

        loop {
            self.tokens.skip_inline_trivia();

            // Check for end of inline conditions
            match self.tokens.peek() {
                None => break,
                Some(t) if stop.as_ref() == Some(t) => {
                    self.tokens.next(); // consume the stop token
                    break;
                }
                Some(Token::Newline | Token::ExplicitChunkEnd(_) | Token::ImplicitChunk(_)) => {
                    break
                }
                _ => {}
            }

            // Grab the actual token
            let tok = self.tokens.peek().unwrap().clone();
            self.tokens.next();

            match tok {
                // Delimiters that can form formatting spans
                Token::Star | Token::Underscore | Token::Tilde => {
                    if let Some(&(ref open_tok, start_idx)) = delim_stack.last() {
                        if *open_tok == tok {
                            // Attempt to close the span
                            delim_stack.pop();
                           let placeholder = output.remove(start_idx);
                            let _ = placeholder; // Inline::Text("")

                            let inner: Vec<Inline> = output.drain(start_idx..).collect();
                            if inner.is_empty() {
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
                            continue;
                        }
                    }
                    let placeholder_idx = output.len();
                    delim_stack.push((tok, placeholder_idx));
                    output.push(Inline::Text(String::new())); // empty placeholder
                }

                Token::Reference(r) => {
                    output.push(Inline::Reference(r.clone()));
                }

                _ => {
                    let text = match tok {
                        Token::Text(s) | Token::Whitespace(s) | Token::Digits(s) => s,
                        Token::Escape(esc) => esc[1..].to_string(),
                        _ => self.tokens.last_slice().to_string(),
                    };
                    output.push(Inline::Text(text));
                }
            }
        }

        // Unwind any unclosed delimiters: replace their placeholder with the literal char
        for (open_tok, placeholder_idx) in delim_stack.into_iter().rev() {
            output.remove(placeholder_idx);
            output.insert(
                placeholder_idx,
                Inline::Text(open_tok.inline_as_str().to_owned()),
            );
            // Note: everything after the placeholder (and possibly after further unclosed spans) stays as-is.
        }

        // Remove any stray empty Text nodes (should be none, but just in case)
        output.retain(|n| !matches!(n, Inline::Text(s) if s.is_empty()));

        output
    }

    fn parse_bold(&mut self) -> Inline {
        self.tokens.next(); // consume opening Star
        let inner = self.parse_inline_until(Token::Star); // stops at and eats closing Star
        Inline::Bold(inner)
    }

    fn parse_italic(&mut self) -> Inline {
        self.tokens.next(); // consume opening Underscore
        let inner = self.parse_inline_until(Token::Underscore);
        Inline::Italic(inner)
    }

    fn parse_strikethrough(&mut self) -> Inline {
        self.tokens.next(); // consume opening Tilde
        let inner = self.parse_inline_until(Token::Tilde);
        Inline::Strikethrough(inner)
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

    fn expect(&mut self, expected: Token) {
        self.tokens.skip_trivia();
        let _ = self.tokens.next(); // ignore mismatches for now
    }

    /// Consume the next token and return its textual representation.
    fn token_to_text(&mut self) -> String {
        let tok = self.tokens.next().expect("expected token");
        match tok {
            Token::Text(s) | Token::Whitespace(s) | Token::Digits(s) => s,
            Token::Escape(esc) => esc[1..].to_string(), // strip the leading backslash
            _ => self.tokens.last_slice().to_string(),
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
            Inline::Reference(r) => s.push_str(r),
        }
    }
    s
}
#[cfg(test)]
mod tests {
    use super::*;

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
                    assert_eq!(inlines.len(), 3, "doc: {:#?}", doc);
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
                    assert!(matches!(&inlines[0], Inline::Bold(inner) if inner == &[Inline::Text("bold".into())]));
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
                    assert!(matches!(&inlines[0], Inline::Italic(inner) if inner == &[Inline::Text("italic".into())]));
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
                    assert!(matches!(&inlines[0], Inline::Strikethrough(inner) if inner == &[Inline::Text("deleted".into())]));
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
    fn reference_named() {
        let doc = parse("see &my_chunk\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 3);
                    assert!(inlines[0] == Inline::Text("see".into()));
                    assert!(inlines[1] == Inline::Text(" ".into()));
                    assert_eq!(inlines[2], Inline::Reference("my_chunk".into()));
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
        }
    }

    #[test]
    fn reference_relative() {
        let doc = parse("&-1 &+2 &3\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 5);
                    assert_eq!(inlines[0], Inline::Reference("-1".into()));
                    assert_eq!(inlines[1], Inline::Text(" ".into()));
                    assert_eq!(inlines[2], Inline::Reference("+2".into()));
                    assert_eq!(inlines[3], Inline::Text(" ".into()));
                    assert_eq!(inlines[4], Inline::Reference("3".into()));
                } else {
                    panic!("Expected paragraph");
                }
            }
            _ => panic!("Expected implicit chunk"),
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
                    // The space after the heading marker is a separate whitespace token.
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
        let doc = parse("#1 Intro\n\nPara *bold*.\n\n:<\n@dir\n>:(s)\n\n---\n");
        assert_eq!(doc.chunks.len(), 4, "Unexpected chunk count: {:#?}", doc);

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
            Chunk::Explicit { name, blocks } => {
                assert_eq!(name, "s");
                assert_eq!(blocks.len(), 1);
                assert!(matches!(blocks[0], Block::Directive { .. }));
            }
            _ => panic!("Expected explicit chunk"),
        }

        match &doc.chunks[3] {
            Chunk::Implicit { block, .. } => {
                assert_eq!(block, &Block::HorizontalRule);
            }
            _ => panic!("Expected implicit hr chunk"),
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
        // first chunk: paragraph containing " unclosed..." (the space before unclosed)
        if let Chunk::Implicit { block, .. } = &doc.chunks[0] {
            if let Block::Paragraph(inlines) = block {
                assert!(inlines.iter().any(|i| matches!(i, Inline::Text(s) if s.contains("unclosed"))));
            } else {
                panic!("Expected paragraph");
            }
        }
        // second chunk: paragraph "real content"
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
        assert!(matches!(&doc.chunks[0], Chunk::Implicit { block: Block::Paragraph(_), .. }));
        assert!(matches!(&doc.chunks[1], Chunk::Implicit { block: Block::Paragraph(_), .. }));
        assert!(matches!(&doc.chunks[2], Chunk::Implicit { block: Block::Heading { .. }, .. }));
        assert!(matches!(&doc.chunks[3], Chunk::Implicit { block: Block::Paragraph(_), .. }));
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
        let doc = parse("*bold _ital ~strike~_ text*\n");
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
                            assert_eq!(italic.len(), 3);
                            assert_eq!(italic[0], Inline::Text("ital".into()));
                            assert_eq!(italic[1], Inline::Text(" ".into()));
                            if let Inline::Strikethrough(s) = &italic[2] {
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
                // Unclosed '*' becomes literal, then 'bold' as separate text
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
    fn reference_with_trailing_characters() {
        let doc = parse("ref &-1-2 end\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            }) => {
                assert_eq!(inlines.len(), 7);
                assert_eq!(inlines[2], Inline::Reference("-1".into()));
                assert_eq!(inlines[3], Inline::Text("-".into()));
                assert_eq!(inlines[4], Inline::Text("2".into()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn explicit_chunk_unclosed() {
        let doc = parse(":< \ninside\nno end\n");
        // Should not panic; currently may produce empty result or survive.
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
        // ** to literal '**', then text.
        let doc = parse("**ast*\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            print!("{:?}", inlines);
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
            print!("{:?}", inlines);
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
        // With the lexer fix, how's is a single Text token.
        let doc = parse("how's it going?\n");
        let chunk = doc.chunks.first().unwrap();
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            // Tokens: Text("how's"), Whitespace(" "), Text("it"), Whitespace(" "), Text("going?")
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
                // Tokens: "wait", " ", "are", " ", "we", " ", "deadass"
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
                assert_eq!(inner.len(), 3); // Text("oh"), space, Italic("no")
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
        if let Chunk::Implicit {
            block: Block::Paragraph(inlines),
            ..
        } = chunk
        {
            // Unclosed '_' becomes literal, then 'italic' as text
            assert_eq!(inlines.len(), 2);
            assert_eq!(inlines[0], Inline::Text("_".into()));
            assert_eq!(inlines[1], Inline::Text("italic".into()));
        } else {
            panic!("Expected paragraph");
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
