use tracing::warn;

use crate::markup::ast::*;
use crate::markup::semantic::{ChunkGraph, RenderState};

pub struct RenderContext<'a> {
    pub current_file: &'a str,
    pub current_chunk_index: usize,
    pub graph: &'a ChunkGraph,
    pub render_state: &'a RenderState,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        current_file: &'a str,
        current_chunk_index: usize,
        graph: &'a ChunkGraph,
        render_state: &'a RenderState,
    ) -> Self {
        Self {
            current_file,
            current_chunk_index,
            graph,
            render_state,
        }
    }

    pub fn for_chunk(
        rel_path: &'a str,
        chunk_idx: usize,
        graph: &'a ChunkGraph,
        state: &'a RenderState,
    ) -> Self {
        Self::new(rel_path, chunk_idx, graph, state)
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
    chunk.blocks().map(|b| render_block(b, ctx)).collect()
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
            let alt_text = escape_attr(inlines_to_plain_text(alt).trim());
            let url = url.trim();
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
    let mut result = String::with_capacity(inlines.len() * 32);
    for inline in inlines {
        result.push_str(&render_inline(inline, ctx));
    }
    result
}

fn render_inline(inline: &Inline, ctx: &RenderContext) -> String {
    match inline {
        Inline::Text(s) => escape_html(s),
        Inline::Bold(inner) => format!("<strong>{}</strong>", render_inlines(inner, ctx)),
        Inline::Italic(inner) => format!("<em>{}</em>", render_inlines(inner, ctx)),
        Inline::Strikethrough(inner) => format!("<del>{}</del>", render_inlines(inner, ctx)),

        Inline::Reference(expr) => {
            match ctx
                .graph
                .resolve_ref(expr, ctx.current_file, ctx.current_chunk_index)
            {
                Some(target) => {
                    let anchor = target.anchor_id();
                    let title = target
                        .first_inline_text
                        .map(|id| ctx.graph.string(id).to_string())
                        .or_else(|| {
                            ctx.render_state
                                .get_inlines(target.id)
                                .map(inlines_to_plain_text)
                        })
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| format!("#{}", anchor));
                    format!("<a href=\"#{anchor}\">{}</a>", escape_html(&title))
                }
                None => {
                    warn!("unresolved ref: {:?}", expr);
                    format!("<!-- unresolved ref: {:?} -->", expr)
                }
            }
        }

        Inline::Transclusion(expr) => {
            let targets =
                ctx.graph
                    .resolve_transclusion(expr, ctx.current_file, ctx.current_chunk_index);
            if targets.is_empty() {
                warn!("unresolved transclusion: {:?}", expr);
                format!("<!-- unresolved transclusion: {:?} -->", expr)
            } else {
                let mut html = String::new();

                targets.into_iter().for_each(|target| {
                    if let Some(chunk_html) = ctx.render_state.get(target.id) {
                        html.push_str(strip_block_tags(chunk_html));
                    }
                });

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
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(c),
        }
    }
    result
}

fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(c),
        }
    }
    result
}

// still not sure if this is a duct tape
fn strip_block_tags(mut html: &str) -> &str {
    html = html.trim();
    let open_tags = [
        "<p>",
        "<h1>",
        "<h2>",
        "<h3>",
        "<h4>",
        "<h5>",
        "<h6>",
        "<blockquote>",
        "<ul>",
        "<ol>",
    ];
    let close_tags = [
        "</p>",
        "</h1>",
        "</h2>",
        "</h3>",
        "</h4>",
        "</h5>",
        "</h6>",
        "</blockquote>",
        "</ul>",
        "</ol>",
    ];
    let self_closing = ["<hr>"];
    loop {
        let old_len = html.len();
        for tag in &open_tags {
            if html.starts_with(tag) {
                html = &html[tag.len()..];
                break;
            }
        }
        for tag in &self_closing {
            if html.starts_with(tag) {
                html = &html[tag.len()..];
                break;
            }
        }
        for tag in &close_tags {
            if html.ends_with(tag) {
                html = &html[..html.len() - tag.len()];
                break;
            }
        }
        if html.ends_with('\n') {
            html = &html[..html.len() - 1];
        }
        if html.is_empty() {
            break;
        }
        if html.len() == old_len {
            break;
        }
    }
    html
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::markup::semantic::{ChunkGraph, RenderState};

    fn empty_graph() -> ChunkGraph {
        ChunkGraph::default()
    }

    fn make_ctx<'a>(
        file: &'a str,
        idx: usize,
        graph: &'a ChunkGraph,
        render_state: &'a RenderState,
    ) -> RenderContext<'a> {
        RenderContext::new(file, idx, graph, render_state)
    }

    #[test]
    fn empty_document_renders_empty() {
        let doc = Document { chunks: vec![] };
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
        assert_eq!(
            render_to_html(&doc, &ctx),
            "<img src=\"test.png\" alt=\"\" />"
        );
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("<p>inside</p>"));
        assert!(html.contains("<hr>"));
    }

    #[test]
    fn reference_unresolved_with_empty_graph() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![
                    Inline::Text("see ".into()),
                    Inline::Reference(RefExpr::Named("missing".into())),
                ]),
            }],
        };
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("unresolved ref"));
    }

    #[test]
    fn transclusion_unresolved_with_empty_graph() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![Inline::Transclusion(RefExpr::Named(
                    "missing".into(),
                ))]),
            }],
        };
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
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
        let graph = empty_graph();
        let render_state = RenderState::default();
        let ctx = make_ctx("test.stuff", 0, &graph, &render_state);
        let html = render_to_html(&doc, &ctx);
        assert!(html.contains("<!-- @foo(arg1) -->"));
    }
}
