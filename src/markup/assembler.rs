use super::parser::AstNode;

pub struct Assembler;

impl Assembler {
    pub fn new() -> Self {
        Self
    }

    pub fn assemble(&self, nodes: &[AstNode]) -> String {
        let mut html = String::new();
        for node in nodes {
            html.push_str(&self.node_to_html(node));
        }
        html
    }

    fn node_to_html(&self, node: &AstNode) -> String {
        match node {
            AstNode::Text(s) => {
                self.escape_html(s)
            },
            AstNode::Bold(n) => {
                format!("<strong>{}</strong>", self.node_to_html(n))
            },
            AstNode::Italic(n) => {
                format!("<em>{}</em>", self.node_to_html(n))
            },
            AstNode::Strikethrough(n) => {
                format!("<del>{}</del>", self.node_to_html(n))
            },
            AstNode::Heading(level, n) => {
                format!("<h{}>{}</h{}>\n", level, self.node_to_html(n), level)
            }
            AstNode::Blockquote(nodes) => {
                let mut html = String::from("<blockquote>\n");
                for n in nodes {
                    html.push_str(&self.node_to_html(n));
                }
                html.push_str("</blockquote>\n");
                html
            }
            AstNode::Paragraph(nodes) => {
                if nodes.is_empty() {
                    String::new()
                } else {
                    let inner: String = nodes
                        .iter()
                        .map(|n| self.node_to_html(n))
                        .collect();
                    if inner.trim().is_empty() {
                        String::new()
                    } else {
                        format!("<p>{}</p>\n", inner)
                    }
                }
            }
        }
    }

    // hmm juicy injections mitigated :D 
    fn escape_html(&self, s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
}

pub fn to_html(nodes: &[AstNode]) -> String {

    Assembler::new().assemble(nodes)
}
