pub mod parser;
pub mod assembler;

pub use parser::{parse, AstNode};
pub use assembler::{to_html};


pub fn render(input: &str) -> String {
    let nodes : Vec<AstNode> = parse(input);
    print!("{}",(nodes.iter().map(|n| n.to_str()).collect::<Vec<String>>().join("")));
    to_html(&nodes)
}
