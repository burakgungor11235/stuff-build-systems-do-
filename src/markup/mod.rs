pub mod assembler;
pub mod parser;

pub use assembler::to_html;
pub use parser::{parse, AstNode};

pub fn render(input: &str) -> String {
    let nodes: Vec<AstNode> = parse(input);
    
    // Log AST structure at trace level
    tracing::trace!(
        ast = %nodes.iter()
            .map(|n| n.to_str())
            .collect::<Vec<String>>()
            .join("")
    );
    
    to_html(&nodes)
}
