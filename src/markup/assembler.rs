use crate::markup::ast::*;

pub fn render_to_html(doc: &Document) -> String {
    let mut html = String::new();
    for chunk in &doc.chunks {
        html.push_str(&render_chunk(chunk));
    }
    html
}

fn render_chunk(chunk: &Chunk) -> String {
    match chunk {
        Chunk::Implicit { block, .. } => render_block(block),
        Chunk::Explicit { blocks, .. } => {
            let mut s = String::new();
            for b in blocks {
                s.push_str(&render_block(b));
            }
            s
        }
    }
}

fn render_block(block: &Block) -> String {
    match block {
        Block::Paragraph(inlines) => {
            format!("<p>{}</p>\n", render_inlines(inlines))
        }
        Block::Heading { level, content } => {
            let tag = format!("h{}", level);
            format!("<{tag}>{}</{tag}>\n", render_inlines(content))
        }
        Block::Blockquote { depth, content } => {
            let open = "<blockquote>".repeat(*depth as usize);
            let close = "</blockquote>".repeat(*depth as usize);
            format!("{}{}{}\n", open, render_inlines(content), close)
        }
        Block::HorizontalRule => "<hr>\n".to_string(),
        Block::Image { alt, url } => {
            let alt_text = render_inlines(alt);
            format!("<img src=\"{url}\" alt=\"{alt_text}\" />")
        }
        Block::Directive { .. } => {
            // For now, ignore or render as a placeholder
            String::new()
        }
        Block::List { items, ordered } => {
            let tag = if *ordered { "ol" } else { "ul" };
            let items_html: String = items
                .iter()
                .map(|item| format!("<li>{}</li>\n", render_inlines(item)))
                .collect();
            format!("<{tag}>\n{items_html}</{tag}>\n")
        }
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    inlines.iter().map(render_inline).collect()
}

fn render_inline(inline: &Inline) -> String {
    match inline {
        Inline::Text(s) => escape_html(s),
        Inline::Bold(inner) => format!("<strong>{}</strong>", render_inlines(inner)),
        Inline::Italic(inner) => format!("<em>{}</em>", render_inlines(inner)),
        Inline::Strikethrough(inner) => format!("<del>{}</del>", render_inlines(inner)),
        Inline::Reference(r) => format!("<!-- ref:{} -->", r), // placeholder
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty_document_renders_empty() {
        let doc = Document { chunks: vec![] };
        assert_eq!(render_to_html(&doc), "");
    }

    #[test]
    fn paragraph_with_text() {
        let doc = Document {
            chunks: vec![Chunk::Implicit {
                name: None,
                block: Block::Paragraph(vec![Inline::Text("Hello".into())]),
            }],
        };
        assert_eq!(render_to_html(&doc), "<p>Hello</p>\n");
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
        assert_eq!(render_to_html(&doc), "<h3>Title</h3>\n");
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
        assert_eq!(
            render_to_html(&doc),
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
        assert_eq!(render_to_html(&doc), "<p>&lt;script&gt;</p>\n");
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
        assert_eq!(render_to_html(&doc), "<img src=\"test.png\" alt=\"\" />");
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
        assert_eq!(
            render_to_html(&doc),
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
        let html = render_to_html(&doc);
        assert!(html.contains("<p>inside</p>"));
        assert!(html.contains("<hr>"));
    }
}
