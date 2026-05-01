#![no_main]
use libfuzzer_sys::fuzz_target;
use sbd::markup::lexer::Token;
use logos::Logos;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut lexer = Token::lexer(&input);
        while let Some(token) = lexer.next() {
            if let Ok(Token::Directive(d)) = token {
                assert!(d.name.len() <= 1000);
                assert!(d.body.len() <= 100000);
            }
        }
    }));

    if res.is_err() {
        panic!("Lexer panicked on nested input: {:?}", input);
    }
});