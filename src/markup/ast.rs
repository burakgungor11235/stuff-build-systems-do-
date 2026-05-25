#[derive(Clone, PartialEq, Debug)]
pub enum RefExpr {
    /// &NAME : named chunk in current file
    Named(String),
    /// &-N or &+N : relative offset from current chunk
    Relative(i32),
    /// &N : absolute chunk index in current file
    Absolute(usize),
    /// &a..b : range of chunks (relative offsets)
    Range(i32, i32),
    /// &(expr1, expr2, ...) : explicit list of references
    List(Vec<RefExpr>),
    /// &file.N : Nth chunk (0-indexed) of external file
    FileByIndex(String, usize),
    /// &file.name : named chunk of external file
    FileByName(String, String),
    /// &file#heading : first chunk under heading in external file
    FileByHeading(String, String),
    /// &file#heading.N : Nth chunk under heading in external file
    FileByHeadingIndex(String, String, usize),
    /// &file#heading.name : named chunk under heading in external file
    FileByHeadingName(String, String, String),
    /// &#heading.. : all chunks below heading (current file)
    HeadingRange(String),
}

#[derive(Clone)]
pub struct Document {
    pub chunks: Vec<Chunk>,
}

#[derive(Clone)]
pub enum Chunk {
    Implicit {
        name: Option<String>, // from :>(name) after the block
        block: Block,
    },
    Explicit {
        name: String, // always present (from the end marker)
        blocks: Vec<Block>,
    },
}

impl Chunk {
    pub fn blocks(&self) -> Box<dyn Iterator<Item = &Block> + '_> {
        match self {
            Chunk::Implicit { block, .. } => Box::new(std::iter::once(block)),
            Chunk::Explicit { blocks, .. } => Box::new(blocks.iter()),
        }
    }
    pub fn size_upper_estimate(&self) -> usize {
        self.blocks().map(|s| s.size_upper_estimate()).sum()
    }
    /// Inline content suitable for inline transclusion.
    /// For multi-block chunks, uses the first block's inline content.
    pub fn inline_content(&self) -> Vec<Inline> {
        self.blocks().next().map(|b| b.inline_content()).unwrap_or_default()
    }
}

#[derive(Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        level: u32,
        content: Vec<Inline>,
    },
    Blockquote {
        depth: u32,
        content: Vec<Inline>,
    }, // Single line block quote; multiple lines become separate Blockquote chunks
    HorizontalRule,
    Image {
        alt: Vec<Inline>,
        url: String,
    },
    Directive {
        name: String,
        body: Option<String>,
    }, // None if simple, Some(body) if complex
    List {
        items: Vec<Vec<Inline>>,
        ordered: bool,
    }, // each item is a sequence of inlines
}

impl Block {
    pub fn heading_and_text(&self) -> Option<String> {
        match self {
            Block::Heading { content, .. } | Block::Paragraph(content) => {
                let text = inlines_to_plain_text(content);
                if text.is_empty() { None } else { Some(text) }
            }
            Block::Blockquote { content, .. } => {
                let text = inlines_to_plain_text(content);
                if text.is_empty() { None } else { Some(text) }
            }
            Block::List { items, .. } => items
                .first()
                .map(|first| inlines_to_plain_text(first)),
            Block::Image { alt, .. } => {
                let text = inlines_to_plain_text(alt);
                if text.is_empty() { None } else { Some(text) }
            }
            _ => None,
        }
    }

    pub fn is_heading(&self) -> bool {
        matches!(self, Block::Heading { .. })
    }

    /// Inline content for inline transclusion purposes.
    pub fn inline_content(&self) -> Vec<Inline> {
        match self {
            Block::Paragraph(inlines)
            | Block::Heading { content: inlines, .. }
            | Block::Blockquote { content: inlines, .. } => inlines.to_vec(),
            Block::List { items, .. } => items.first().cloned().unwrap_or_default(),
            Block::Image { alt, .. } => alt.to_vec(),
            Block::HorizontalRule | Block::Directive { .. } => vec![],
        }
    }

    // used to make vec::new() faster by allocating what we need (and more, it's a rough estimate)
    // should be used for while iterating over a Chunk / Block
    // NOTE: stuff like `Block::Image`'s  alt.len() is the size, don't expect string as something which is in the
    // size.
    // A fancy wrapper for the flattened lenght
    pub fn size_upper_estimate(&self) -> usize {
        match self {
            Block::Paragraph(inlines)
            | Block::Heading {
                content: inlines, ..
            }
            | Block::Blockquote {
                content: inlines, ..
            } => inlines.len(),
            Block::Image { alt, .. } => alt.len(),
            Block::List { items, .. } => items.iter().map(|i| i.len()).sum(),
            Block::Directive { .. } => 0,
            Block::HorizontalRule => 0,
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Reference(RefExpr),
    Link {
        target: String,
        display: Vec<Inline>,
    },
    Transclusion(RefExpr),
}

use std::fmt;

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Document {{")?;
        for (i, chunk) in self.chunks.iter().enumerate() {
            write!(f, "  chunk[{}]: ", i)?;
            let inner = format!("{:?}", chunk);
            let indented = inner.lines().collect::<Vec<_>>().join("\n    ");
            writeln!(f, "{}", indented)?;
        }
        writeln!(f, "}}")
    }
}

impl RefExpr {
    pub fn extract_file_name(&self) -> String {
        match self {
            RefExpr::Named(n) => n.clone(),
            RefExpr::Relative(n) => n.to_string(),
            RefExpr::Absolute(n) => n.to_string(),
            _ => String::new(),
        }
    }
}

impl fmt::Debug for Chunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Chunk::Implicit { name, block } => {
                write!(f, "Implicit")?;
                if let Some(n) = name {
                    write!(f, " name={:?}", n)?;
                } else {
                    write!(f, " name=NONE")?;
                }
                write!(f, " -> ")?;
                let inner = format!("{:?}", block);
                let indented = inner.lines().collect::<Vec<_>>().join("\n      ");
                write!(f, "{}", indented)
            }
            Chunk::Explicit { name, blocks } => {
                write!(f, "Explicit name={:?} blocks=[", name)?;
                for (i, block) in blocks.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", block)?;
                }
                write!(f, "]")
            }
        }
    }
}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Block::Paragraph(inlines) => write!(f, "Paragraph({:?})", inlines),
            Block::Heading { level, content } => {
                write!(f, "Heading(level={}, {:?})", level, content)
            }
            Block::Blockquote { depth, content } => {
                write!(f, "Blockquote(depth={}, {:?})", depth, content)
            }
            Block::HorizontalRule => write!(f, "HorizontalRule"),
            Block::Image { alt, url } => {
                write!(f, "Image(alt={:?}, url={:?})", alt, url)
            }
            Block::Directive { name, body } => {
                if let Some(b) = body {
                    write!(f, "Directive(name={:?}, body={:?})", name, b)
                } else {
                    write!(f, "Directive(name={:?}, body=NONE)", name)
                }
            }
            Block::List { items, ordered } => {
                let tag = if *ordered { "ordered" } else { "unordered" };
                write!(f, "List({}, [", tag)?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", item)?;
                }
                write!(f, "])")
            }
        }
    }
}

impl fmt::Debug for Inline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inline::Text(s) => write!(f, "Text({:?})", s),
            Inline::Bold(inner) => write!(f, "Bold({:?})", inner),
            Inline::Italic(inner) => write!(f, "Italic({:?})", inner),
            Inline::Strikethrough(inner) => write!(f, "Strikethrough({:?})", inner),
            Inline::Link { target, display } => {
                write!(f, "Link({:?} -> {:?})", target, display)
            }
            Inline::Transclusion(expr) => write!(f, "Transclude({:?})", expr),
            Inline::Reference(expr) => write!(f, "Ref({:?})", expr),
        }
    }
}

pub fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Bold(inner) | Inline::Italic(inner) | Inline::Strikethrough(inner) => {
                s.push_str(&inlines_to_plain_text(inner));
            }
            Inline::Reference(_) => {}
            Inline::Link { .. } => {}
            Inline::Transclusion(_) => {}
        }
    }
    s
}

fn collect_from_inlines<'a>(inlines: &'a [Inline], refs: &mut Vec<&'a RefExpr>) {
    for inline in inlines {
        match inline {
            Inline::Transclusion(expr) => refs.push(expr),
            Inline::Bold(inner) | Inline::Italic(inner) | Inline::Strikethrough(inner) => {
                collect_from_inlines(inner, refs);
            }
            Inline::Link { display, .. } => {
                collect_from_inlines(display, refs);
            }
            _ => {}
        }
    }
}

pub fn extract_transclusion_refs(chunk: &Chunk) -> Vec<&RefExpr> {
    let mut refs = Vec::with_capacity(chunk.size_upper_estimate());
    for block in chunk.blocks() {
        collect_from_inlines_for_chunk(block, &mut refs);
    }
    refs
}

fn collect_from_inlines_for_chunk<'a>(block: &'a Block, refs: &mut Vec<&'a RefExpr>) {
    match block {
        Block::Paragraph(inlines)
        | Block::Heading {
            content: inlines, ..
        }
        | Block::Blockquote {
            content: inlines, ..
        } => {
            collect_from_inlines(inlines, refs);
        }
        Block::List { items, .. } => {
            for item in items {
                collect_from_inlines(item, refs);
            }
        }
        _ => {}
    }
}
