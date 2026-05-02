#![no_main]
use libfuzzer_sys::fuzz_target;
use sbd::markup::lexer::Token;
use logos::Logos;
// dummy simple round trip testing
fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);

    let mut lexer = Token::lexer(&input);
    let mut reconstructed = String::new();

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while let Some(_) = lexer.next() {
            reconstructed.push_str(lexer.slice());
        }
    }));

    if res.is_err() {
        panic!("Lexer panicked on input: {:?}", input);
    }

    if input != reconstructed {
        panic!(
            "Round-trip failed!\nInput: {:?}\nOutput: {:?}",
            input, reconstructed
        );
    }
});
