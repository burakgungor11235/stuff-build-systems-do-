#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = sbd::markup::parse(&input);
    }));

    if res.is_err() {
        panic!("Parser panicked on input: {:?}", input);
    }
});