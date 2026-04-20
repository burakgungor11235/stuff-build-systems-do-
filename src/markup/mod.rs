pub mod assembler;
pub mod parser;

pub use assembler::to_html;
pub use parser::{parse, AstNode};

pub fn render(input: &str) -> String {
    let nodes: Vec<AstNode> = parse(input);
    print!( // ma look I'm cool now 
        "{}",
        (nodes
            .iter()
            .map(|n| n.to_str())
            .collect::<Vec<String>>()
            .join(""))
    );
    let nodes: Vec<AstNode> = parse(input);
    to_html(&nodes)
}
