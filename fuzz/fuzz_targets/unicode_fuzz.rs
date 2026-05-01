#![no_main]
use libfuzzer_sys::fuzz_target;
use sbd::markup::lexer::Token;
use logos::Logos;

fuzz_target!(|data: &[u8]| {
    if !std::str::from_utf8(data).is_ok() {
        return;
    }

    let input = String::from_utf8_lossy(data).into_owned();

    if input.chars().count() > 10000 {
        return;
    }

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut lexer = Token::lexer(&input);
        while let Some(_) = lexer.next() {}
    }));

    if res.is_err() {
        panic!("Unicode lexer panicked on: {:?}", input);
    }
});