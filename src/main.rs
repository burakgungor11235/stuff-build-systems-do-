use std::fs::write;

mod markup;

fn main() {
    let input = r#"

#1 Test Document
> something
>> something something
> > something else

Some normal text with *bold*, _italic_, and ~strikethrough~.

>>> Triple blockquote!
>> Back to double
> And back to single

> Blockquote with *bold text* inside!
"#;

    let html = markup::render(input);
    println!("{}", html);
    println!("{input}");
    write("out.html", html).expect("lololol");
}
