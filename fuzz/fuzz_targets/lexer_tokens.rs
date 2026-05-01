#![no_main]
use libfuzzer_sys::fuzz_target;
use sbd::markup::lexer::Token;
use logos::Logos;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let mut lexer = Token::lexer(&input);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => tokens.push(token),
            Err(_) => {}
        }
    }

    for token in &tokens {
        match token {
            Token::Directive(d) => {
                assert!(d.name.len() <= 1000, "Directive name too long");
                assert!(d.body.len() <= 10000, "Directive body too long");
            }
            Token::SimpleDirective(s) => {
                assert!(s.len() <= 1000, "SimpleDirective name too long");
            }
            Token::Reference(s) => {
                assert!(s.len() <= 1000, "Reference too long");
            }
            Token::Heading(lvl) => {
                assert!(*lvl >= 1 && *lvl <= 99, "Invalid heading level");
            }
            _ => {}
        }
    }
});