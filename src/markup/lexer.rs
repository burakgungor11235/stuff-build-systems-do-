use logos::{Lexer, Logos};

#[derive(Debug, PartialEq, Clone)]
pub struct DirectiveData {
    pub name: String,
    pub body: String,
}

/// Callback for `@name(` directives.
/// If the parentheses never close, the remainder of the input is consumed as the body.
pub fn lex_directive_payload(lex: &mut Lexer<Token>) -> Option<DirectiveData> {
    let slice = lex.slice(); // e.g., "@foo("
    let name = slice[1..slice.len() - 1].to_string();
    let remainder = lex.remainder();
    let mut depth = 1;
    let mut end_index = 0;

    for (i, b) in remainder.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            end_index = i;
            break;
        }
    }

    if depth == 0 {
        let body = remainder[..end_index].to_string();
        lex.bump(end_index + 1); // consume body + closing ')'
        Some(DirectiveData { name, body })
    } else {
        // Unclosed, consume **everything** that remains
        let body = remainder.to_string();
        lex.bump(remainder.len());
        Some(DirectiveData { name, body })
    }
}

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    #[regex(r"\\.", |lex| lex.slice().to_string())]
    Escape(String),

    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*\(", lex_directive_payload, priority = 4)]
    Directive(DirectiveData),

    #[regex(r"@[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice()[1..].to_string(), priority = 3)]
    SimpleDirective(String),

    #[regex(r"&([a-zA-Z_][a-zA-Z0-9_]*|[+-][0-9]+|[0-9]+)", |lex| lex.slice()[1..].to_string(), priority = 4)]
    Reference(String),

    #[token(":<")]
    ExplicitChunkStart,

    #[regex(r">:\([^)]+\)", |lex| lex.slice()[3..lex.slice().len()-1].to_string())]
    ExplicitChunkEnd(String),

    #[regex(r":>\([^)]+\)", |lex| lex.slice()[3..lex.slice().len()-1].to_string())]
    ImplicitChunk(String),

    #[regex(r"#[1-9][0-9]*", |lex| lex.slice()[1..].parse::<u32>().ok(), priority = 3)]
    Heading(u32),

    #[token("---")]
    HorizontalRule,

    #[regex(r">+")]
    BlockquotePrefix,

    #[token("![", priority = 5)]
    ImageStart,

    #[regex(r"[0-9]+", |lex| lex.slice().to_string(), priority = 2)]
    Digits(String),

    #[regex(r"[ \t]+", |lex| lex.slice().to_string())]
    Whitespace(String),

    #[token("\n")]
    Newline,

    #[token("*")]
    Star,
    #[token("_")]
    Underscore,
    #[token("~")]
    Tilde,
    #[token("^")]
    Caret,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("/")]
    Slash,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token(".")]
    Dot,
    #[token("!")]
    Bang,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("|")]
    Pipe,

    /// A complete, balanced comment: `/' ... '/`
    #[regex(r"/'([^']|'[^/])*'/", priority = 2)]
    Comment,

    /// An unclosed `/'` marker. The rest of the line is NOT consumed.
    #[token("/'", priority = 1)]
    IncompleteComment,

    /// Matches any single character not caught by the patterns above
    #[regex(
        r"[^ \t\n@*_\~\[\]|<>^\{\}\(\)/.\+!\-]+",
        |lex| lex.slice().to_string(),
        priority = 1
    )]
    Text(String),
}

impl Token {
   pub fn inline_as_str(&self) -> &str{
        match self {
            Token::Star => "*",
            Token::Underscore => "_",
            Token::Tilde => "~",
            _ => unreachable!(),
        }
    }
}
