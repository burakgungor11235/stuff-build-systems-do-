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
    }, // single‑line blockquote; multiple lines become separate blockquote chunks
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
    Reference(String), // the string inside &... , e.g. "-1", "my_chunk"
}

use std::fmt;

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Document {{")?;
        for (i, chunk) in self.chunks.iter().enumerate() {
            write!(f, "  chunk[{}]: ", i)?;
            // Indent inner chunk debug output
            let inner = format!("{:?}", chunk);
            let indented = inner.lines().collect::<Vec<_>>().join("\n    ");
            writeln!(f, "{}", indented)?;
        }
        writeln!(f, "}}")
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
                // Indent block debug
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
            Block::Paragraph(inlines) => {
                write!(f, "Paragraph({:?})", inlines)
            }
            Block::Heading { level, content } => {
                write!(f, "Heading(level={}, {:?})", level, content)
            }
            Block::Blockquote { depth, content } => {
                write!(f, "Blockquote(depth={}, {:?})", depth, content)
            }
            Block::HorizontalRule => {
                write!(f, "HorizontalRule")
            }
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
            Inline::Reference(r) => write!(f, "Ref({:?})", r),
        }
    }
}
