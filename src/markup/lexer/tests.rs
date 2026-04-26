use crate::markup::lexer::{Lexer, Token};

#[test]
fn test_full_document() {
    let input = r#"# Heading

*bold* and _italic_

> Blockquote

1. List item

---

![img|url]
"#;
    let tokens = Lexer::new(input).tokenize();
    println!("{} : {:?}", input, tokens);
    
    assert!(tokens.iter().any(|t| matches!(t, Token::Heading { .. })));
    assert!(tokens.iter().any(|t| matches!(t, Token::BoldStart | Token::BoldEnd)));
    assert!(tokens.iter().any(|t| matches!(t, Token::Blockquote { .. })));
    assert!(tokens.iter().any(|t| matches!(t, Token::ListItem { .. })));
    assert!(tokens.iter().any(|t| matches!(t, Token::HorizontalRule { .. })));
    assert!(tokens.iter().any(|t| matches!(t, Token::Image { .. })));
}

#[test]
fn test_poc_syntax() {
    let input = include_str!("../../../poc_all_syntax.stuff");
    let tokens = Lexer::new(input).tokenize();
    println!("poc_all_syntax.stuff : {} tokens", tokens.len());
}