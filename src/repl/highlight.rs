use crate::markup::{ast::Document, lexer::Token};
use ansi_term::Colour;
use ansi_term::Style;

// palette
pub const C_BLUE: Colour = Colour::Fixed(33); // Blue
pub const C_TEAL: Colour = Colour::Fixed(37); // Teal
pub const C_GREEN: Colour = Colour::Fixed(35); // Green
pub const C_PURPLE: Colour = Colour::Fixed(135); // Purple
pub const C_RED: Colour = Colour::Fixed(160); // Red
pub const C_GREY: Colour = Colour::Fixed(242); // Mid grey
pub const C_WHITE: Colour = Colour::Fixed(255); // Off-white

pub(crate) fn bold(c: Colour) -> Style {
    c.bold()
}
pub(crate) fn norm(c: Colour) -> Style {
    c.normal()
}

pub fn highlight_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_attr = false;
    let mut in_val = false;

    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                in_attr = false;
                in_val = false;
                result.push_str(&bold(C_BLUE).prefix().to_string());
                result.push('<');
            }
            '>' if in_tag => {
                result.push('>');
                result.push_str(&bold(C_BLUE).suffix().to_string());
                in_tag = false;
            }
            ' ' if in_tag && !in_val => {
                // start of attribute name
                in_attr = true;
                result.push(' ');
            }
            '=' if in_tag && in_attr && !in_val => {
                in_val = true;
                result.push_str(&norm(C_GREEN).prefix().to_string());
                result.push('=');
            }
            '"' if in_val => {
                result.push('"');
                // close value? we simplify: close after closing quote
                in_val = false;
                result.push_str(&norm(C_GREEN).suffix().to_string());
            }
            _ if in_tag && in_attr && !in_val => {
                result.push_str(&bold(C_TEAL).prefix().to_string());
                result.push(c);
                result.push_str(&bold(C_TEAL).suffix().to_string());
                in_attr = false;
            }
            _ if in_val => {
                result.push_str(&norm(C_GREEN).prefix().to_string());
                result.push(c);
                result.push_str(&norm(C_GREEN).suffix().to_string());
            }
            _ => result.push(c),
        }
    }
    result
}

pub fn highlight_tokens(tokens: &[Token]) -> String {
    let mut result = String::from("[");
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            result.push_str(", ");
        }
        result.push_str(&highlight_token(token));
    }
    result.push(']');
    result
}

fn highlight_token(token: &Token) -> String {
    match token {
        // ---- primary structure (blue) ----
        Token::Directive(d) => format!(
            "{}({}, {})",
            bold(C_BLUE).paint("Directive"),
            norm(C_WHITE).paint(&d.name),
            norm(C_PURPLE).paint(&d.body)
        ),
        Token::SimpleDirective(s) => format!(
            "{}({})",
            bold(C_BLUE).paint("SimpleDirective"),
            norm(C_WHITE).paint(s)
        ),
        Token::Heading(level) => format!(
            "{}({})",
            bold(C_BLUE).paint("Heading"),
            norm(C_GREEN).paint(level.to_string())
        ),
        Token::ExplicitChunkStart => bold(C_BLUE).paint("ExplicitChunkStart").to_string(),
        Token::ExplicitChunkEnd(s) => format!(
            "{}({})",
            bold(C_BLUE).paint("ExplicitChunkEnd"),
            norm(C_WHITE).paint(s)
        ),
        Token::ImplicitChunk(s) => format!(
            "{}({})",
            bold(C_BLUE).paint("ImplicitChunk"),
            norm(C_WHITE).paint(s)
        ),

        // ---- secondary structure (teal) ----
        Token::Star => norm(C_TEAL).paint("*").to_string(),
        Token::Underscore => norm(C_TEAL).paint("_").to_string(),
        Token::Tilde => norm(C_TEAL).paint("~").to_string(),
        Token::Caret => norm(C_TEAL).paint("^").to_string(),
        Token::LBrace => norm(C_TEAL).paint("{").to_string(),
        Token::RBrace => norm(C_TEAL).paint("}").to_string(),
        Token::LParen => norm(C_TEAL).paint("(").to_string(),
        Token::RParen => norm(C_TEAL).paint(")").to_string(),
        Token::Slash => norm(C_TEAL).paint("/").to_string(),
        Token::Plus => norm(C_TEAL).paint("+").to_string(),
        Token::Minus => norm(C_TEAL).paint("-").to_string(),
        Token::Dot => norm(C_TEAL).paint(".").to_string(),
        Token::Bang => norm(C_TEAL).paint("!").to_string(),
        Token::LBracket => norm(C_TEAL).paint("[").to_string(),
        Token::RBracket => norm(C_TEAL).paint("]").to_string(),
        Token::Pipe => norm(C_TEAL).paint("|").to_string(),
        Token::BlockquotePrefix => bold(C_TEAL).paint("BlockquotePrefix").to_string(),
        Token::HorizontalRule => bold(C_TEAL).paint("HorizontalRule").to_string(),
        Token::ImageStart => bold(C_TEAL).paint("ImageStart").to_string(),

        Token::LinkStart => norm(C_TEAL).paint("[[").to_string(),
        Token::LinkEnd => norm(C_TEAL).paint("]]").to_string(),
        // ---- metadata (purple) ----
        Token::Reference(s) => format!(
            "{}({})",
            bold(C_PURPLE).paint("Reference"),
            norm(C_WHITE).paint(s)
        ),
        Token::Escape(s) => format!(
            "{}({})",
            bold(C_PURPLE).paint("Escape"),
            norm(C_WHITE).paint(s)
        ),

        // ---- data (green) ----
        Token::Digits(s) => format!(
            "{}({})",
            norm(C_GREEN).paint("Digits"),
            norm(C_WHITE).paint(s)
        ),

        Token::Transclusion(s) => format!(
            "{}({})",
            bold(C_GREEN).paint("Transclusion"),
            norm(C_WHITE).paint(s)
        ),
        // ---- trivia (grey) ----
        Token::Whitespace(s) => norm(C_GREY)
            .paint(s.replace(' ', "·").replace('\t', "␣"))
            .to_string(), // look at me being fancy!
        Token::Newline => norm(C_GREY).paint("\\n").to_string(),
        Token::Comment => norm(C_GREY).paint("Comment").to_string(),

        // ---- error (red) ----
        Token::IncompleteComment => bold(C_RED).paint("IncompleteComment").to_string(),

        // ---- content (white) ----
        Token::Text(s) => norm(C_WHITE).paint(s).to_string(),
    }
}

pub fn highlight_ast(doc: &Document) -> String {
    highlight_chunk_list(&doc.chunks, 0)
}

fn highlight_chunk_list(chunks: &[crate::markup::ast::Chunk], indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    let mut result = String::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if i > 0 {
            result.push_str(&format!("\n{prefix}"));
        }
        result.push_str(&highlight_chunk(chunk, indent));
    }
    result
}

fn highlight_chunk(chunk: &crate::markup::ast::Chunk, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    match chunk {
        crate::markup::ast::Chunk::Implicit { name, block } => {
            let name_str = match name {
                Some(n) => format!(" -> {}", Colour::Yellow.paint(n)),
                None => String::from(" -> NONE"),
            };
            format!(
                "{}{}{} -> {}",
                Colour::Cyan.bold().paint("Implicit"),
                name_str,
                Colour::White.paint("{"),
                highlight_block(block, indent + 1)
            )
        }
        crate::markup::ast::Chunk::Explicit { name, blocks } => {
            let blocks_str = blocks
                .iter()
                .map(|b| highlight_block(b, indent + 1))
                .collect::<Vec<_>>()
                .join(&format!("\n{prefix}   "));
            format!(
                "{}({}) {} {}{}",
                Colour::Cyan.bold().paint("Explicit"),
                Colour::Yellow.paint(name),
                Colour::White.paint("{"),
                blocks_str,
                Colour::White.paint("}")
            )
        }
    }
}

fn highlight_block(block: &crate::markup::ast::Block, _indent: usize) -> String {
    match block {
        crate::markup::ast::Block::Paragraph(inlines) => {
            format!(
                "{}([{}])",
                Colour::Cyan.bold().paint("Paragraph"),
                highlight_inline_list(inlines)
            )
        }
        crate::markup::ast::Block::Heading { level, content } => {
            format!(
                "{}({}) [{}]",
                Colour::Cyan.bold().paint("Heading"),
                Colour::Yellow.paint(level.to_string()),
                highlight_inline_list(content)
            )
        }
        crate::markup::ast::Block::Blockquote { depth, content } => {
            format!(
                "{}({}) [{}]",
                Colour::Cyan.bold().paint("Blockquote"),
                Colour::Yellow.paint(depth.to_string()),
                highlight_inline_list(content)
            )
        }
        crate::markup::ast::Block::HorizontalRule => {
            Colour::Cyan.bold().paint("HorizontalRule").to_string()
        }
        crate::markup::ast::Block::Image { alt, url } => {
            format!(
                "{} {{ alt: [{}], url: {} }}",
                Colour::Cyan.bold().paint("Image"),
                highlight_inline_list(alt),
                Colour::Green.paint(url)
            )
        }
        crate::markup::ast::Block::Directive { name, body } => {
            let body_str = match body {
                Some(b) => format!(", {}", Colour::Yellow.paint(b)),
                None => String::new(),
            };
            format!(
                "{}({}{})",
                Colour::Cyan.bold().paint("Directive"),
                Colour::Green.paint(name),
                body_str
            )
        }
        crate::markup::ast::Block::List { items, ordered } => {
            let tag = if *ordered { "ordered" } else { "unordered" };
            let items_str = items
                .iter()
                .map(|item| format!("[{}]", highlight_inline_list(item)))
                .collect::<Vec<String>>()
                .join(", ");
            format!(
                "{}({}) [{}]",
                Colour::Cyan.bold().paint("List"),
                Colour::Yellow.paint(tag),
                items_str
            )
        }
    }
}

fn highlight_inline_list(inlines: &[crate::markup::ast::Inline]) -> String {
    inlines
        .iter()
        .map(highlight_inline)
        .collect::<Vec<_>>()
        .join(", ")
}

fn highlight_inline(inline: &crate::markup::ast::Inline) -> String {
    match inline {
        crate::markup::ast::Inline::Text(t) => {
            format!(
                "{}({})",
                Colour::Green.paint("Text"),
                Colour::White.paint(t)
            )
        }
        crate::markup::ast::Inline::Bold(inlines) => {
            format!(
                "{}([{}])",
                Colour::Cyan.bold().paint("Bold"),
                highlight_inline_list(inlines)
            )
        }
        crate::markup::ast::Inline::Italic(inlines) => {
            format!(
                "{}([{}])",
                Colour::Cyan.bold().paint("Italic"),
                highlight_inline_list(inlines)
            )
        }
        crate::markup::ast::Inline::Strikethrough(inlines) => {
            format!(
                "{}([{}])",
                Colour::Cyan.bold().paint("Strikethrough"),
                highlight_inline_list(inlines)
            )
        }
        crate::markup::ast::Inline::Reference(r) => {
            format!(
                "{}({})",
                Colour::Yellow.bold().paint("Ref"),
                Colour::White.paint(r)
            )
        }
        crate::markup::ast::Inline::Link { target, display } => {
            format!(
                "{}({}, {})",
                bold(C_BLUE).paint("Link"),
                highlight_inline_list(display),
                norm(C_PURPLE).paint(target)
            )
        }

        crate::markup::ast::Inline::Transclusion(t) => {
            format!(
                "{}({})",
                bold(C_PURPLE).paint("Transclusion"),
                norm(C_WHITE).paint(t)
            )
        }
    }
}
