use std::fs::write;

mod markup;

fn main() {
    let input = r#"#1 Hello World

This is *bold* and _italic_ and ~strikethrough~ text.

> This is a blockquote
> with multiple lines

#2 Second Heading

More text here.
"#;

    let html = markup::render(input);
    println!("{}", html);
    write("out.html", html).expect("lololol");
}
