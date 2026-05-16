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

impl Document {
    pub fn len(&self) -> usize {
        self.chunks.len()
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
