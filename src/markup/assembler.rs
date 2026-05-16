use std::collections::HashSet;

use crate::markup::ast::*;
use crate::bs::registry::ChunkRegistry;

pub struct RenderContext<'a> {
    pub current_file: &'a str,
    pub current_chunk_index: usize,
    pub registry: &'a ChunkRegistry,
    transclusion_stack: HashSet<(String, usize)>,
}

impl<'a> RenderContext<'a> {
    pub fn new(current_file: &'a str, current_chunk_index: usize, registry: &'a ChunkRegistry) -> Self {
        Self {
            current_file,
            current_chunk_index,
            registry,
            transclusion_stack: HashSet::new(),
        }
    }

    fn transclusion_ctx(&self, file: &str, idx: usize) -> Option<Self> {
        let key = (file.to_string(), idx);
        if self.transclusion_stack.contains(&key) {
            None // cycle detected
        } else {
            let mut new = self.clone();
            new.transclusion_stack.insert(key);
            Some(new)
        }
    }
}

impl<'a> Clone for RenderContext<'a> {
    fn clone(&self) -> Self {
        Self {
            current_file: self.current_file,
            current_chunk_index: self.current_chunk_index,
            registry: self.registry,
            transclusion_stack: self.transclusion_stack.clone(),
        }
    }
}

pub fn render_to_html(doc: &Document, ctx: &RenderContext) -> String {
    let mut html = String::new();
    for chunk in &doc.chunks {
        html.push_str(&render_chunk(chunk, ctx));
    }
    html
}

pub fn render_chunk(chunk: &Chunk, ctx: &RenderContext) -> String {
    match chunk {
        Chunk::Implicit { block, .. } => render_block(block, ctx),
        Chunk::Explicit { blocks, .. } => {
            let mut s = String::new();
            for b in blocks {
                s.push_str(&render_block(b, ctx));
            }
            s
        }
    }
}

fn render_block(block: &Block, ctx: &RenderContext) -> String {
    match block {
        Block::Paragraph(inlines) => {
            format!("<p>{}</p>\n", render_inlines(inlines, ctx))
        }
        Block::Heading { level, content } => {
            let tag = format!("h{}", level);
            format!("<{tag}>{}</{tag}>\n", render_inlines(content, ctx))
        }
        Block::Blockquote { depth, content } => {
            let open = "<blockquote>".repeat(*depth as usize);
            let close = "</blockquote>".repeat(*depth as usize);
            format!("{}{}{}\n", open, render_inlines(content, ctx), close)
        }
        Block::HorizontalRule => "<hr>\n".to_string(),
        Block::Image { alt, url } => {
            let alt_text = render_inlines(alt, ctx);
            format!("<img src=\"{url}\" alt=\"{alt_text}\" />")
        }
        Block::Directive { name, body } => {
            // For now, directives render as HTML comments
            format!(
                "<!-- @{name}{} -->\n",
                match body {
                    Some(s) => format!("({s})"),
                    None => String::new(),
                }
            )
        }
        Block::List { items, ordered } => {
            let tag = if *ordered { "ol" } else { "ul" };
            let items_html: String = items
                .iter()
                .map(|item| format!("<li>{}</li>\n", render_inlines(item, ctx)))
                .collect();
            format!("<{tag}>\n{items_html}</{tag}>\n")
        }
    }
}

fn render_inlines(inlines: &[Inline], ctx: &RenderContext) -> String {
    inlines.iter().map(|i| render_inline(i, ctx)).collect()
}

fn render_inline(inline: &Inline, ctx: &RenderContext) -> String {
    match inline {
        Inline::Text(s) => escape_html(s),
        Inline::Bold(inner) => format!("<strong>{}</strong>", render_inlines(inner, ctx)),
        Inline::Italic(inner) => format!("<em>{}</em>", render_inlines(inner, ctx)),
        Inline::Strikethrough(inner) => format!("<del>{}</del>", render_inlines(inner, ctx)),

        Inline::Reference(expr) => {
            match ctx.registry.resolve(expr, ctx.current_file, ctx.current_chunk_index) {
                Some(target) => {
                    let anchor = &target.anchor_id;
                    let title = target.first_inline_text.as_deref().unwrap_or("");
                    match expr {
                        // For heading ranges, render as anchor link to the heading
                        RefExpr::HeadingRange(_heading) => {
                            format!("<a href=\"#{anchor}\">{title}</a>")
                        }
                        _ => {
                            format!("<a href=\"#{anchor}\">{title}</a>")
                        }
                    }
                }
                None => {
                    format!("<!-- unresolved ref: {:?} -->", expr)
                }
            }
        }

        Inline::Transclusion(expr) => {
            let targets = ctx.registry.resolve_transclusion(expr, ctx.current_file, ctx.current_chunk_index);
            if targets.is_empty() {
                format!("<!-- unresolved transclusion: {:?} -->", expr)
            } else {
                let mut html = String::new();
                for target in targets {
                    if ctx.transclusion_ctx(ctx.current_file, target.index).is_none() {
                        html.push_str("<!-- cyclic transclusion detected -->");
                    } else {
                        html.push_str(&target.html);
                        html.push('\n');
                    }
                }
                html
            }
        }

        Inline::Link { target, display } => {
            let display_html = render_inlines(display, ctx);
            format!("<a href=\"{}\">{}</a>", escape_attr(target), display_html)
        }
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('"', "&quot;")
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::bs::registry::{ChunkRegistry};
    use crate::bs::registry::ChunkInfo;

    fn empty_registry() -> ChunkRegistry {
        ChunkRegistry::default()
    }

    fn make_ctx<'a>(file: &'a str, idx: usize, reg: &'a ChunkRegistry) -> RenderContext<'a> {
        RenderContext::new(file, idx, reg)
    }

#[test]
    fn empty_document_renders_empty() {
        let doc = Document { chunks: vec![] };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(render_to_html(&doc, &ctx), "");
    }

    #[test]
    fn paragraph_with_text() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![Inline::Text("Hello".into())]),
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(render_to_html(&doc, &ctx), "<p>Hello</p>\n");
    }

    #[test]
    fn heading_renders_correct_level() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Heading {
                    level: 3,
                    content: vec![Inline::Text("Title".into())],
                },
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(render_to_html(&doc, &ctx), "<h3>Title</h3>\n");
    }

    #[test]
    fn bold_and_italic() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![
                    Inline::Bold(vec![Inline::Text("bold".into())]),
                    Inline::Text(" and ".into()),
                    Inline::Italic(vec![Inline::Text("italic".into())]),
                ]),
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(
            render_to_html(&doc, &ctx),
            "<p><strong>bold</strong> and <em>italic</em></p>\n"
        );
    }

    #[test]
    fn html_escaping() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![Inline::Text("<script>".into())]),
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(render_to_html(&doc, &ctx), "<p>&lt;script&gt;</p>\n");
    }

    #[test]
    fn image_no_alt() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Image {
                    alt: vec![],
                    url: "test.png".into(),
                },
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(render_to_html(&doc, &ctx), "<img src=\"test.png\" alt=\"\" />");
    }

    #[test]
    fn list_unordered() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::List {
                    items: vec![
                        vec![Inline::Text("a".into())],
                        vec![Inline::Text("b".into())],
                    ],
                    ordered: false,
                },
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        assert_eq!(
            render_to_html(&doc, &ctx),
            "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n"
        );
    }

    #[test]
    fn explicit_chunk_renders_inner_blocks() {
        let doc = Document {
            chunks: vec![Chunk::Explicit {
                name: "box".into(),
                blocks: vec![
                    Block::Paragraph(vec![Inline::Text("inside".into())]),
                    Block::HorizontalRule,
                ],
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("<p>inside</p>"));
        assert!(html.contains("<hr>"));
    }

    #[test]
    fn reference_unresolved_with_empty_registry() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![
                    Inline::Text("see ".into()),
                    Inline::Reference(RefExpr::Named("missing".into())),
                ]),
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("unresolved ref"));
    }

    #[test]
    fn transclusion_unresolved_with_empty_registry() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![Inline::Transclusion(RefExpr::Named("missing".into()))]),
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("unresolved transclusion"));
    }

    #[test]
    fn directive_renders_as_comment() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Directive {
                    name: "foo".into(),
                    body: Some("arg1".into()),
                },
            }],
        };
        let reg = empty_registry();
        let ctx = make_ctx("test.stuff", 0, &reg);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("<!-- @foo(arg1) -->"));
    }

    }
