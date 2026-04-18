pub mod parser;
pub mod assembler;

pub use parser::{parse, AstNode};
pub use assembler::{to_html};


pub fn render(input: &str) -> String {
    let nodes : Vec<AstNode> = parse(input);
    for node in nodes.clone() {
        print!("{:?}", node);
    }
    to_html(&nodes)
}
