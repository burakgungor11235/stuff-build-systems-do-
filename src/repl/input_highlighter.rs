use logos::Logos;
use rustyline::completion::Completer;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Helper;
use std::borrow::Cow;

use crate::markup::lexer::Token;
use crate::repl::highlight::*;

pub fn highlight_source(source: &str) -> String {
    let mut out = String::new();
    let mut lex = Token::lexer(source);
    while let Some(tok) = lex.next() {
        let slice = lex.slice();
        match tok {
            Ok(token) => out.push_str(&colour_token_slice(&token, slice)),
            Err(()) => out.push_str(&bold(C_RED).paint(slice).to_string()), // lexer error highlight red
        }
    }
    out
}

fn colour_token_slice(token: &Token, slice: &str) -> String {
    match token {
        // --- structure (blue) ---
        Token::Directive(_) => bold(C_BLUE).paint(slice).to_string(),
        Token::SimpleDirective(_) => bold(C_BLUE).paint(slice).to_string(),
        Token::Heading(_) => bold(C_BLUE).paint(slice).to_string(),
        Token::ExplicitChunkStart => bold(C_BLUE).paint(slice).to_string(),
        Token::ExplicitChunkEnd(_) => bold(C_BLUE).paint(slice).to_string(),
        Token::ImplicitChunk(_) => bold(C_BLUE).paint(slice).to_string(),
        Token::HorizontalRule => bold(C_BLUE).paint(slice).to_string(),

        // --- inline formatting delimiters (teal) ---
        Token::Star
        | Token::Underscore
        | Token::Tilde
        | Token::Caret
        | Token::LBrace
        | Token::RBrace
        | Token::LParen
        | Token::RParen
        | Token::Slash
        | Token::Plus
        | Token::Minus
        | Token::Dot
        | Token::Bang
        | Token::LBracket
        | Token::RBracket
        | Token::Pipe => norm(C_TEAL).paint(slice).to_string(),

        // --- block markers (teal) ---
        Token::BlockquotePrefix => bold(C_TEAL).paint(slice).to_string(),
        Token::ImageStart => bold(C_TEAL).paint(slice).to_string(),

        // --- references & escapes (purple) ---
        Token::Reference(_) => bold(C_PURPLE).paint(slice).to_string(),
        Token::Escape(_) => norm(C_PURPLE).paint(slice).to_string(),

        // --- numbers (green) ---
        Token::Digits(_) => norm(C_GREEN).paint(slice).to_string(),

        // --- whitespace & newlines (grey) ---
        Token::Whitespace(_) => norm(C_GREY)
            .paint(slice.replace(' ', "·").replace('\t', "␣"))
            .to_string(),
        Token::Newline => norm(C_GREY).paint("¶\n".to_string()).to_string(), // show pilcrow + actual newline

        // --- comments (grey) ---
        Token::Comment | Token::IncompleteComment => norm(C_GREY).paint(slice).to_string(),

        // --- text (white) ---
        Token::Text(_) => norm(C_WHITE).paint(slice).to_string(),
    }
}
pub struct InputHighlighter;

impl Completer for InputHighlighter {
    type Candidate = String;
}

impl Hinter for InputHighlighter {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Validator for InputHighlighter {}

impl Highlighter for InputHighlighter {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &self,
        prompt: &'p str,
        _is_password: bool,
    ) -> Cow<'b, str> {
        Cow::Owned(bold(C_BLUE).paint(prompt).to_string())
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(highlight_source(line))
    }
}

impl Helper for InputHighlighter {}
