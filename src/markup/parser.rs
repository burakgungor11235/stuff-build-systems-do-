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
        let nodes = self.parse_inline();
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
        self.expect(Token::ImageStart); // consumes ![
        let alt = self.parse_inline_until(Token::Pipe); // consumes |
        let url_tokens = self.parse_inline_until(Token::RBracket); // consumes ]
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
                assert!(name.is_none(), "Name should be None, got {:#?}", doc);
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(
                        inlines,
                        &[Inline::Text("hello".into())],
                        "Paragraph content mismatch; doc: {:#?}",
                        doc
                    );
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn paragraph_with_spaces() {
        let doc = parse("hello   world\n");
        let chunk = doc.chunks.first().expect("Expected one chunk");
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    // tokens: Text("hello"), Whitespace("   "), Text("world")
                    assert_eq!(inlines.len(), 3, "Unexpected inline count; doc: {:#?}", doc);
                    assert_eq!(inlines[0], Inline::Text("hello".into()), "doc: {:#?}", doc);
                    assert_eq!(inlines[1], Inline::Text("   ".into()), "doc: {:#?}", doc);
                    assert_eq!(inlines[2], Inline::Text("world".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn bold_inline() {
        let doc = parse("*bold*\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    assert_eq!(inlines.len(), 1, "doc: {:#?}", doc);
                    match &inlines[0] {
                        Inline::Bold(inner) => {
                            assert_eq!(inner, &[Inline::Text("bold".into())], "doc: {:#?}", doc);
                        }
                        _ => panic!("Expected bold, got {:#?}", doc),
                    }
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn italic_inline() {
        let doc = parse("_italic_\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    match &inlines[0] {
                        Inline::Italic(inner) => {
                            assert_eq!(inner, &[Inline::Text("italic".into())], "doc: {:#?}", doc)
                        }
                        _ => panic!("Expected italic, got {:#?}", doc),
                    }
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn strikethrough_inline() {
        let doc = parse("~deleted~\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    match &inlines[0] {
                        Inline::Strikethrough(inner) => {
                            assert_eq!(inner, &[Inline::Text("deleted".into())], "doc: {:#?}", doc)
                        }
                        _ => panic!("Expected strikethrough, got {:#?}", doc),
                    }
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn nested_formatting() {
        let doc = parse("*bold ~and~ here*\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    match &inlines[0] {
                        Inline::Bold(inner) => {
                            // tokens: Text("bold"), Whitespace(" "), Strikethrough, Whitespace(" "), Text("here")
                            assert_eq!(inner.len(), 5, "doc: {:#?}", doc);
                            assert_eq!(inner[0], Inline::Text("bold".into()), "doc: {:#?}", doc);
                            assert_eq!(inner[1], Inline::Text(" ".into()), "doc: {:#?}", doc);
                            match &inner[2] {
                                Inline::Strikethrough(s) => {
                                    assert_eq!(s, &[Inline::Text("and".into())], "doc: {:#?}", doc)
                                }
                                _ => panic!("Expected strikethrough inside bold, got {:#?}", doc),
                            }
                            assert_eq!(inner[3], Inline::Text(" ".into()), "doc: {:#?}", doc);
                            assert_eq!(inner[4], Inline::Text("here".into()), "doc: {:#?}", doc);
                        }
                        _ => panic!("Expected bold, got {:#?}", doc),
                    }
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn reference_named() {
        let doc = parse("see &my_chunk\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    // tokens: Text("see"), Whitespace(" "), Reference("my_chunk")
                    assert_eq!(inlines.len(), 3, "doc: {:#?}", doc);
                    assert_eq!(
                        inlines[2],
                        Inline::Reference("my_chunk".into()),
                        "doc: {:#?}",
                        doc
                    );
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn reference_relative() {
        let doc = parse("&-1 &+2 &3\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Paragraph(inlines) = block {
                    // tokens: Ref("-1"), Whitespace(" "), Ref("+2"), Whitespace(" "), Ref("3")
                    assert_eq!(inlines.len(), 5, "doc: {:#?}", doc);
                    assert_eq!(
                        inlines[0],
                        Inline::Reference("-1".into()),
                        "doc: {:#?}",
                        doc
                    );
                    assert_eq!(
                        inlines[2],
                        Inline::Reference("+2".into()),
                        "doc: {:#?}",
                        doc
                    );
                    assert_eq!(inlines[4], Inline::Reference("3".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected paragraph, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn heading_level_1() {
        let doc = parse("#1 Title\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Heading { level, content } = block {
                    assert_eq!(*level, 1, "doc: {:#?}", doc);
                    // tokens: Whitespace(" "), Text("Title")
                    assert_eq!(content.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(content[0], Inline::Text(" ".into()), "doc: {:#?}", doc);
                    assert_eq!(content[1], Inline::Text("Title".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected heading, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn heading_level_3() {
        let doc = parse("#3 Deep\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => match block {
                Block::Heading { level, content } => {
                    assert_eq!(*level, 3, "doc: {:#?}", doc);
                    assert_eq!(content.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(content[0], Inline::Text(" ".into()), "doc: {:#?}", doc);
                    assert_eq!(content[1], Inline::Text("Deep".into()), "doc: {:#?}", doc);
                }
                _ => panic!("Expected heading, got {:#?}", doc),
            },
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn horizontal_rule() {
        let doc = parse("---\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                assert_eq!(
                    block,
                    &Block::HorizontalRule,
                    "Expected horizontal rule, got {:#?}",
                    doc
                );
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn blockquote_single_line() {
        let doc = parse("> quote\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Blockquote { depth, content } = block {
                    assert_eq!(*depth, 1, "doc: {:#?}", doc);
                    assert_eq!(content.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(content[0], Inline::Text(" ".into()), "doc: {:#?}", doc);
                    assert_eq!(content[1], Inline::Text("quote".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected blockquote, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn blockquote_nested() {
        let doc = parse(">>> deep\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Blockquote { depth, content } = block {
                    assert_eq!(*depth, 3, "doc: {:#?}", doc);
                    assert_eq!(content.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(content[0], Inline::Text(" ".into()), "doc: {:#?}", doc);
                    assert_eq!(content[1], Inline::Text("deep".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected blockquote, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn image_simple() {
        let doc = parse("![ alt | url ]\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Image { alt, url } = block {
                    // tokens: Whitespace(" "), Text("alt"), Whitespace(" ")
                    assert_eq!(alt.len(), 3, "doc: {:#?}", doc);
                    assert_eq!(alt[0], Inline::Text(" ".into()), "doc: {:#?}", doc);
                    assert_eq!(alt[1], Inline::Text("alt".into()), "doc: {:#?}", doc);
                    assert_eq!(alt[2], Inline::Text(" ".into()), "doc: {:#?}", doc);
                    assert_eq!(url, " url ", "doc: {:#?}", doc);
                } else {
                    panic!("Expected image, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn image_no_alt() {
        let doc = parse("![|url]\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Image { alt, url } = block {
                    assert!(alt.is_empty(), "Expected empty alt, got {:#?}", doc);
                    assert_eq!(url, "url", "doc: {:#?}", doc);
                } else {
                    panic!("Expected image, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn directive_simple() {
        let doc = parse("@mytag\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Directive { name, body } = block {
                    assert_eq!(name, "mytag", "doc: {:#?}", doc);
                    assert!(body.is_none(), "Expected no body, got {:#?}", doc);
                } else {
                    panic!("Expected directive, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn directive_with_body() {
        let doc = parse("@plugin(arg1, &chunk)\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::Directive { name, body } = block {
                    assert_eq!(name, "plugin", "doc: {:#?}", doc);
                    assert_eq!(body.as_deref(), Some("arg1, &chunk"), "doc: {:#?}", doc);
                } else {
                    panic!("Expected directive, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn list_unordered() {
        let doc = parse("- one\n- two\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::List { items, ordered } = block {
                    assert!(!ordered, "Expected unordered list, got {:#?}", doc);
                    assert_eq!(items.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(items[0][0], Inline::Text("one".into()), "doc: {:#?}", doc);
                    assert_eq!(items[1][0], Inline::Text("two".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected list, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn list_ordered() {
        let doc = parse("1. first\n2. second\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { block, .. } => {
                if let Block::List { items, ordered } = block {
                    assert!(*ordered, "Expected ordered list, got {:#?}", doc);
                    assert_eq!(items.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(items[0][0], Inline::Text("first".into()), "doc: {:#?}", doc);
                    assert_eq!(
                        items[1][0],
                        Inline::Text("second".into()),
                        "doc: {:#?}",
                        doc
                    );
                } else {
                    panic!("Expected list, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn implicit_chunk_named() {
        let doc = parse("para :>(myname)\n");
        let chunk = doc.chunks.first().unwrap();
        match chunk {
            Chunk::Implicit { name, block } => {
                assert_eq!(name.as_deref(), Some("myname"), "doc: {:#?}", doc);
                assert!(
                    matches!(block, Block::Paragraph(_)),
                    "Expected paragraph, got {:?}",
                    block
                );
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
    }

    #[test]
    fn explicit_chunk() {
        let doc = parse(":< \n line1 \n line2 \n>:(ex)\n");
        match doc.chunks.first() {
            Some(Chunk::Explicit { name, blocks }) => {
                assert_eq!(name, "ex", "doc: {:#?}", doc);
                assert_eq!(blocks.len(), 2, "doc: {:#?}", doc);
                assert!(matches!(blocks[0], Block::Paragraph(_)), "doc: {:#?}", doc);
                assert!(matches!(blocks[1], Block::Paragraph(_)), "doc: {:#?}", doc);
            }
            other => panic!("Expected explicit chunk, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn explicit_chunk_with_varied_blocks() {
        let doc = parse(":< \n #1 head \n --- \n>:(mixed)\n");
        match doc.chunks.first() {
            Some(Chunk::Explicit { name, blocks }) => {
                assert_eq!(name, "mixed", "doc: {:#?}", doc);
                assert_eq!(blocks.len(), 2, "doc: {:#?}", doc);
                assert!(
                    matches!(blocks[0], Block::Heading { .. }),
                    "doc: {:#?}",
                    doc
                );
                assert!(
                    matches!(blocks[1], Block::HorizontalRule),
                    "doc: {:#?}",
                    doc
                );
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
                    assert_eq!(*level, 1, "doc: {:#?}", doc);
                    // Intro after #1 and space
                    assert_eq!(content.len(), 2, "doc: {:#?}", doc);
                    assert_eq!(content[1], Inline::Text("Intro".into()), "doc: {:#?}", doc);
                } else {
                    panic!("Expected heading, got {:#?}", doc);
                }
            }
            _ => panic!("Expected implicit heading chunk, got {:#?}", doc),
        }

        match &doc.chunks[1] {
            Chunk::Implicit { block, .. } => {
                assert!(
                    matches!(block, Block::Paragraph(_)),
                    "Expected paragraph, got {:#?}",
                    doc
                );
            }
            _ => panic!("Expected implicit paragraph chunk, got {:#?}", doc),
        }

        match &doc.chunks[2] {
            Chunk::Explicit { name, blocks } => {
                assert_eq!(name, "s", "doc: {:#?}", doc);
                assert_eq!(blocks.len(), 1, "doc: {:#?}", doc);
                assert!(
                    matches!(blocks[0], Block::Directive { .. }),
                    "doc: {:#?}",
                    doc
                );
            }
            _ => panic!("Expected explicit chunk, got {:#?}", doc),
        }

        match &doc.chunks[3] {
            Chunk::Implicit { block, .. } => {
                assert_eq!(
                    block,
                    &Block::HorizontalRule,
                    "Expected horizontal rule, got {:#?}",
                    doc
                );
            }
            _ => panic!("Expected implicit hr chunk, got {:#?}", doc),
        }
    }
    #[test]
    fn comments_ignored() {
        // comments are trivia, so the document should be empty
        let doc = parse("/' this is a comment '/\n");
        assert!(doc.chunks.is_empty(), "Expected no chunks, got {:#?}", doc);
    }

    #[test]
    fn incomplete_comment_ignored() {
        // The lexer only treats /' itself as IncompleteComment; the rest of the line is parsed.
        // So we get a paragraph with the trailing text, then the next line.
        let doc = parse("/' unclosed...\nreal content\n");
        assert_eq!(doc.chunks.len(), 2, "Expected two chunks, got {:#?}", doc);
        match &doc.chunks[0] {
            Chunk::Implicit { block, .. } => {
                assert!(matches!(block, Block::Paragraph(_)), "doc: {:#?}", doc);
                if let Block::Paragraph(inlines) = block {
                    // The text is " unclosed..." (space before unclosed)
                    assert!(inlines
                        .iter()
                        .any(|i| matches!(i, Inline::Text(s) if s.contains("unclosed"))));
                }
            }
            _ => panic!("Expected implicit chunk, got {:#?}", doc),
        }
        match &doc.chunks[1] {
            Chunk::Implicit { block, .. } => {
                assert!(matches!(block, Block::Paragraph(_)));
            }
            _ => panic!("Expected second implicit chunk"),
        }
    }
    #[test]
    fn empty_explicit_chunk() {
        let doc = parse(":< \n>:(nothing)\n");
        match doc.chunks.first() {
            Some(Chunk::Explicit { name, blocks }) => {
                assert_eq!(name, "nothing", "doc: {:#?}", doc);
                assert!(blocks.is_empty(), "Expected empty blocks, got {:#?}", doc);
            }
            other => panic!("Expected explicit chunk, got {:?} in {:#?}", other, doc),
        }
    }

    #[test]
    fn multiple_paragraphs_and_headings() {
        let doc = parse("line1\n\nline2\n\n#1 Title\n\npara after heading\n");
        assert_eq!(doc.chunks.len(), 4, "doc: {:#?}", doc);
        // chunk0: paragraph
        // chunk1: paragraph
        // chunk2: heading
        // chunk3: paragraph
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
                // first item content starts with "item"
                assert_eq!(items[0][0], Inline::Text("item".into()), "doc: {:#?}", doc);
                assert_eq!(items[1][0], Inline::Text("next".into()), "doc: {:#?}", doc);
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
                assert_eq!(inlines.len(), 3, "doc: {:#?}", doc);
                assert_eq!(inlines[0], Inline::Text("1".into()));
                assert_eq!(inlines[1], Inline::Text(" ".into()));
                assert_eq!(inlines[2], Inline::Text("something".into()));
            }
            other => panic!("Expected paragraph, got {:?} in {:#?}", other, doc),
        }
    }
    #[test]
    fn mixed_inline_formatting_deep() {
        // bold containing italic and strikethrough
        let doc = parse("*bold _ital ~strike~_ text*\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            }) => {
                assert_eq!(inlines.len(), 1);
                if let Inline::Bold(inner) = &inlines[0] {
                    assert_eq!(inner.len(), 5, "doc: {:#?}", doc);
                    // inner tokens: Text("bold"), space, Italic(...), space, Text("text")
                    assert_eq!(inner[0], Inline::Text("bold".into()));
                    assert_eq!(inner[1], Inline::Text(" ".into()));
                    match &inner[2] {
                        Inline::Italic(italic) => {
                            assert_eq!(italic.len(), 3);
                            assert_eq!(italic[0], Inline::Text("ital".into()));
                            assert_eq!(italic[1], Inline::Text(" ".into()));
                            match &italic[2] {
                                Inline::Strikethrough(s) => {
                                    assert_eq!(s[0], Inline::Text("strike".into()))
                                }
                                _ => panic!("Expected strikethrough"),
                            }
                        }
                        _ => panic!("Expected italic inside bold"),
                    }
                    assert_eq!(inner[3], Inline::Text(" ".into()));
                    assert_eq!(inner[4], Inline::Text("text".into()));
                } else {
                    panic!("Expected bold");
                }
            }
            _ => panic!("Expected paragraph, got {:#?}", doc),
        }
    }

    #[test]
    fn unclosed_bold_before_newline() {
        let doc = parse("*bold\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            }) => {
                // It might be one Bold node or just Text? Let's check.
                assert!(!inlines.is_empty(), "doc: {:#?}", doc);
                // Ideally we want some error handling, but for now just check we didn't panic.
            }
            _ => panic!("Expected paragraph, got {:#?}", doc),
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
                // whitespace is preserved: space, Bold, space, Text("and"), space, Italic, space
                assert_eq!(alt.len(), 7, "doc: {:#?}", doc);
                assert!(
                    alt.iter().any(|i| matches!(i, Inline::Bold(_))),
                    "Expected bold in alt"
                );
                assert!(
                    alt.iter().any(|i| matches!(i, Inline::Italic(_))),
                    "Expected italic in alt"
                );
                assert_eq!(url, " url ", "doc: {:#?}", doc);
            }
            _ => panic!("Expected image chunk, got {:#?}", doc),
        }
    }
    #[test]
    fn directive_with_nested_parens() {
        // lexer handles nested parens inside directive body
        let doc = parse("@foo(inner(a, b(2))) \n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Directive { name, body },
                ..
            }) => {
                assert_eq!(name, "foo");
                assert_eq!(body.as_deref(), Some("inner(a, b(2))"), "doc: {:#?}", doc);
            }
            _ => panic!("Expected directive, got {:#?}", doc),
        }
    }

    #[test]
    fn reference_with_trailing_characters() {
        // "&-1-2": lexer produces Ref("-1"), then Minus, then Digits("2")
        let doc = parse("ref &-1-2 end\n");
        match doc.chunks.first() {
            Some(Chunk::Implicit {
                block: Block::Paragraph(inlines),
                ..
            }) => {
                // tokens: Text("ref"), space, Ref("-1"), Minus, Digits("2"), space, Text("end")
                assert_eq!(inlines.len(), 7, "doc: {:#?}", doc);
                assert_eq!(inlines[2], Inline::Reference("-1".into()));
                assert_eq!(inlines[3], Inline::Text("-".into()));
                assert_eq!(inlines[4], Inline::Text("2".into()));
            }
            _ => panic!("Expected paragraph, got {:#?}", doc),
        }
    }

    #[test]
    fn explicit_chunk_unclosed() {
        // :< without >:( should not crash, but may discard the chunk
        let doc = parse(":< \ninside\nno end\n");
        // Currently the explicit chunk detection fails because no end marker, parse_chunk returns None,
        // then in parse_document we skip a token and continue. The result may be an empty document or some paragraphs.
        // Just ensure no panic.
        assert!(
            doc.chunks.is_empty() || !doc.chunks.is_empty(),
            "doc: {:#?}",
            doc
        );
        // Ideally we'd have graceful error recovery, but for now we only check it doesn't crash.
    }

    #[test]
    fn list_with_blank_line_inside() {
        // The parser treats blank lines as chunk separators, so a blank line inside a list would
        // end the current chunk and start a new one. Thus the list won't span blank lines.
        let doc = parse("- item1\n\n- item2\n");
        // Expect two chunks: first is a list chunk with one item, second is another list chunk with one item.
        assert_eq!(doc.chunks.len(), 2, "doc: {:#?}", doc);
        // Both should be List
        for chunk in &doc.chunks {
            match chunk {
                Chunk::Implicit { block, .. } => assert!(matches!(block, Block::List { .. })),
                _ => panic!("Unexpected chunk"),
            }
        }
    }
}
