

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // structural blocks 
    Heading {
        level: u8,
    },
    Blockquote {
        level: u8,
    },
    HorizontalRule,

    ListItem {
        indent: usize,
        kind: ListKind,
        marker: String,
    },

    ParagraphBreak,

    // inline 
    Text(String),

    BoldStart,
    BoldEnd,
    ItalicStart,
    ItalicEnd,
    StrikeStart,
    StrikeEnd,

    Image {
        alt: String,
        url: String,
    },

    // chunks 
    ChunkStartImplicit {
        name: String,
    },
    ChunkStartExplicit,
    ChunkEnd {
        name: String,
    },

    // directives 
    Identifier(String),


    // references 
    ChunkRef(String),
    // comment 
    Comment(String),

    // control
    // technically Chunks are a subclass of this but eeeh whos counting (not me)
    Newline,
    EOF,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListKind {
    OrderedNumeric,
    OrderedAlpha,
    OrderedRoman,
    Unordered,
    Loose, // maybe in the future
}

pub struct Lexer<'a> {
    input: &'a str,
    chars: std::str::Chars<'a>,
    current: Option<char>,
    pos: usize,
    in_bold: bool,
    in_italic: bool,
    in_strike: bool,
    at_start: bool,
    prev_list_kind: Option<(usize, ListKind)>,
    last_was_newline: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut l = Lexer {
            input,
            chars: input.chars(),
            current: None,
            pos: 0,
            in_bold: false,
            in_italic: false,
            in_strike: false,
            at_start: true,
            prev_list_kind: None,
            last_was_newline: false,
        };
        l.bump();
        l
    }

    fn bump(&mut self) {
        // Track if previous was newline before advancing
        let prev_was_newline = self.current == Some('\n');
        
        self.current = self.chars.next();
        self.pos += 1;
        
        // Check if we just consumed a newline
        if self.pos > 0 {
            if let Some(c) = self.input.chars().nth(self.pos - 1) {
                if c == '\n' {
                    self.at_start = true;
                }
            }
        }
        
        // Update last_was_newline
        self.last_was_newline = prev_was_newline;
    }

    pub fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while let Some(c) = self.current {
            let token = match c {
                '\n' => {
                    self.bump();
                    // Check if this is a paragraph break (two consecutive newlines)
                    if self.last_was_newline {
                        Token::ParagraphBreak
                    } else {
                        Token::Newline
                    }
                }

                '@' => self.lex_directive(),
                '&' => self.lex_reference(),
                '#' => self.lex_heading(),
                '>' => self.lex_blockquote(),

                '-' => {
                    // Check if it's a horizontal rule first
                    if self.peek_hr() {
                        self.consume_line();
                        Token::HorizontalRule
                    } else {
                        // Try to lex as list item (works for both ordered and unordered)
                        if let Some(list_item) = self.lex_list_item() {
                            list_item
                        } else {
                            self.lex_text()
                        }
                    }
                }

                ':' => self.lex_chunk_or_text(),

                '*' | '_' | '~' => self.lex_inline_marker(),

                '/' if self.peek_comment() => self.lex_comment(),

                _ => {
                    // Check for image syntax ![Alt text | URL]
                    if self.at_start && self.current == Some('!') {
                        if let Some(img) = self.try_image() {
                            img
                        } else if self.at_start {
                            // Check for list marker
                            if let Some(list_item) = self.lex_list_item() {
                                list_item
                            } else {
                                self.lex_text()
                            }
                        } else {
                            self.lex_text()
                        }
                    } else if self.at_start {
                        // Check if we're at the start of a line and have a list marker
                        if let Some(list_item) = self.lex_list_item() {
                            list_item
                        } else {
                            self.lex_text()
                        }
                    } else {
                        self.lex_text()
                    }
                }
            };

            tokens.push(token);
        }

        tokens.push(Token::EOF);

        tokens
    }

    fn lex_directive(&mut self) -> Token {
        self.bump(); // '@'
        let name = self.lex_identifier();
        Token::Identifier(name)
    }

    ///  &chunk
    fn lex_reference(&mut self) -> Token {
        self.bump(); // '&'
        let name = self.lex_identifier();
        Token::ChunkRef(name)
    }
    fn lex_heading(&mut self) -> Token {
        let mut level = 0;

        while self.current == Some('#') {
            level += 1;
            self.bump();
        }

        Token::Heading { level }
    }
    fn lex_blockquote(&mut self) -> Token {
        let start_pos = self.pos;
        let mut level = 0;

        while self.current == Some('>') {
            level += 1;
            self.bump();
        }

        // Check for chunk end: >:(name) or >:name
        if self.current == Some(':') {
            let after_colon_pos = self.pos;
            self.bump(); // consume ':'
            
            if self.current == Some('(') {
                // >:(name)
                self.bump(); // consume '('
                let name = self.lex_identifier();
                if self.current == Some(')') {
                    self.bump(); // consume ')'
                }
                self.at_start = false;
                return Token::ChunkEnd { name };
            } else if let Some(c) = self.current {
                if c.is_ascii_alphabetic() {
                    // >:name
                    // Reset position to after the colon, then lex the identifier
                    self.pos = after_colon_pos;
                    self.chars = self.input[self.pos..].chars();
                    self.current = self.chars.next();
                    let name = self.lex_identifier();
                    self.at_start = false;
                    return Token::ChunkEnd { name };
                }
            }
            
            // Not chunk end, reset to start
            self.pos = start_pos;
            self.chars = self.input[self.pos..].chars();
            self.current = self.chars.next();
        }

        Token::Blockquote { level }
    }

    fn lex_chunk_or_text(&mut self) -> Token {
        self.bump(); // ':'

        match self.current {
            Some('>') => {
                self.bump();
                let name = self.lex_identifier();
                Token::ChunkStartImplicit { name }
            }
            Some('<') => {
                self.bump();
                Token::ChunkStartExplicit
            }
            _ => self.lex_text(),
        }
    }

    fn lex_inline_marker(&mut self) -> Token {
        match self.current {
            Some('*') => {
                self.bump();
                if self.in_bold {
                    self.in_bold = false;
                    Token::BoldEnd
                } else {
                    self.in_bold = true;
                    Token::BoldStart
                }
            }
            Some('_') => {
                self.bump();
                if self.in_italic {
                    self.in_italic = false;
                    Token::ItalicEnd
                } else {
                    self.in_italic = true;
                    Token::ItalicStart
                }
            }
            Some('~') => {
                self.bump();
                if self.in_strike {
                    self.in_strike = false;
                    Token::StrikeEnd
                } else {
                    self.in_strike = true;
                    Token::StrikeStart
                }
            }
            _ => self.lex_text(),
        }
    }

    fn lex_comment(&mut self) -> Token {
        let mut buf = String::new();
        
        // Check for /* or */ style comment
        if self.current == Some('*') {
            self.bump(); // consume '*'
            
            // Collect until */
            let mut found_end = false;
            while let Some(c) = self.current {
                if c == '*' {
                    self.bump();
                    if self.current == Some('/') {
                        self.bump(); // consume '/'
                        found_end = true;
                        break;
                    }
                } else if c == '\n' {
                    break;
                } else {
                    buf.push(c);
                    self.bump();
                }
            }
            
            if found_end {
                return Token::Comment(buf);
            }
            
            // If no end found, return what we have
            return Token::Comment(buf);
        }
        
        // Fallback: treat everything until newline as comment
        while let Some(c) = self.current {
            if c == '\n' {
                break;
            }
            buf.push(c);
            self.bump();
        }
        
        Token::Comment(buf)
    }

    fn peek_comment(&self) -> bool {
        // Check if we're at start of line and have comment start
        if !self.at_start {
            return false;
        }
        // Check for /* style or */ style comment start
        let rest = &self.input[self.pos..];
        rest.starts_with("/*") || rest.starts_with("*/")
    }

    fn lex_text(&mut self) -> Token {
        let mut buf = String::new();

        while let Some(c) = self.current {
            if matches!(c, '\n' | '@' | '&' | '#' | ':' | '*' | '_' | '~') {
                break;
            }
            buf.push(c);
            self.bump();
        }

        Token::Text(buf)
    }

    fn lex_identifier(&mut self) -> String {
        let mut buf = String::new();

        while let Some(c) = self.current {
            if c.is_alphanumeric() || c == '_' {
                buf.push(c);
                self.bump();
            } else {
                break;
            }
        }

        buf
    }

    fn peek_hr(&self) -> bool {
        // simplistic HR detection: "---"
        self.input[self.pos..].starts_with("---")
    }

     fn consume_line(&mut self) {
         while let Some(c) = self.current {
             self.bump();
             if c == '\n' {
                 break;
             }
         }
     }

     /// Check if we're at the start of a line (after a newline or at the beginning)
     fn is_at_line_start(&self) -> bool {
         // At the very beginning of input
         if self.pos == 0 {
             return true;
         }
         
         // Check if the previous character was a newline
         if self.pos > 0 {
             let prev_char = self.input.chars().nth(self.pos - 1);
             return prev_char == Some('\n');
         }
         
false
     }

/// Try to parse a list item marker
/// Returns Some(ListItem) if successful, None otherwise
fn lex_list_item(&mut self) -> Option<Token> {
    let start_pos = self.pos;
    
    // Skip leading whitespace (indentation)
    let mut indent = 0;
    while let Some(c) = self.current {
        if c == ' ' || c == '\t' {
            if c == ' ' {
                indent += 1;
            } else if c == '\t' {
                indent += 4;
            }
            self.bump();
        } else {
            break;
        }
    }
    
    // Check for unordered list (- followed by space)
    if self.current == Some('-') {
        self.bump(); // consume '-'
        
        if self.current == Some(' ') {
            self.bump(); // consume the space too
            self.at_start = false;
            let marker = format!("- ");
            return Some(Token::ListItem {
                indent,
                kind: ListKind::Unordered,
                marker,
            });
        } else {
            // Not a list item, reset
            self.pos = start_pos;
            self.chars = self.input[self.pos..].chars();
            self.current = self.chars.next();
        }
    }
    
    // Check for ordered numeric list (1. 2. 3. etc.)
    /* btw
    1.das 
    3.asd 
    2.dasda 
        is ordered. I am not your father, you should be responsible for your own choices
    */
    if let Some(c) = self.current {
        if c.is_ascii_digit() {
            // Consume all digits
            let mut num_str = String::new();
            while let Some(c) = self.current {
                if c.is_ascii_digit() {
                    num_str.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            
            // Check for dot separator
            if self.current == Some('.') {
                self.bump(); // consume '.'
                
                // Check for space after dot
                if self.current == Some(' ') {
                    self.bump(); // consume space
                    self.at_start = false;
                    let marker = format!("{}.", num_str);
                    return Some(Token::ListItem {
                        indent,
                        kind: ListKind::OrderedNumeric,
                        marker,
                    });
                }
            }
        }
    }
    
    // Check for ordered alpha/roman list
    if let Some(c) = self.current {
        if c.is_ascii_alphabetic() {
            let letter = c.to_string();
            let letter_upper = letter.to_uppercase();
            
            self.bump(); // consume the letter
            
            // Check for dot separator
            if self.current == Some('.') {
                self.bump(); // consume '.'
                
                // Check for space after dot
                if self.current == Some(' ') {
                    self.bump(); // consume space
                    self.at_start = false;
                    
                    // Roman numeral characters: i, v, x, l, c, d, m (and their uppercase)
                    let roman_chars = ["I", "V", "X", "L", "C", "D", "M"];
                    let is_roman_char = roman_chars.contains(&letter_upper.as_str());
                    
                    // Determine list type based on context
                    let kind = if is_roman_char {
                        // At same indent as previous list?
                        if let Some((prev_indent, prev_kind)) = &self.prev_list_kind {
                            if *prev_indent == indent {
                                // Same indent - preserve the previous type
                                if matches!(prev_kind, ListKind::OrderedAlpha) {
                                    ListKind::OrderedAlpha
                                } else {
                                    ListKind::OrderedRoman
                                }
                            } else if *prev_indent > indent {
                                // Going back up in indent - check what came before
                                // For simplicity, default to Alpha at the original level
                                ListKind::OrderedAlpha
                            } else {
                                ListKind::OrderedRoman
                            }
                        } else {
                            // First item or after different list type - default to Roman for roman chars
                            ListKind::OrderedRoman
                        }
                    } else {
                        // Non-roman char is always Alpha
                        ListKind::OrderedAlpha
                    };
                    
                    // Update tracking for next item
                    self.prev_list_kind = Some((indent, kind.clone()));
                    
                    let marker = format!("{}.", letter);
                    return Some(Token::ListItem {
                        indent,
                        kind,
                        marker,
                    });
                }
            }
        }
    }
    
    // Reset position if we didn't find a valid list item
    self.pos = start_pos;
    self.chars = self.input[self.pos..].chars();
    self.current = self.chars.next();
    
None
    }
    
    /// Try to lex an image: ![Alt text | URL]
    fn try_image(&mut self) -> Option<Token> {
        let start_pos = self.pos;
        self.bump(); // consume '!'
        
        if self.current != Some('[') {
            // Not an image, reset
            self.pos = start_pos;
            self.chars = self.input[self.pos..].chars();
            self.current = self.chars.next();
            return None;
        }
        
        self.bump(); // consume '['
        
        // Collect alt text until | or ]
        let mut alt_text = String::new();
        while let Some(c) = self.current {
            if c == '|' {
                break;
            }
            if c == ']' {
                break;
            }
            alt_text.push(c);
            self.bump();
        }
        
        if self.current != Some('|') {
            // Not valid image syntax, reset
            self.pos = start_pos;
            self.chars = self.input[self.pos..].chars();
            self.current = self.chars.next();
            return None;
        }
        
        self.bump(); // consume '|'
        
        // Collect URL
        let mut url = String::new();
        while let Some(c) = self.current {
            if c == ']' {
                break;
            }
            url.push(c);
            self.bump();
        }
        
        if self.current != Some(']') {
            // Not valid image syntax, reset
            self.pos = start_pos;
            self.chars = self.input[self.pos..].chars();
            self.current = self.chars.next();
            return None;
        }
        
        self.bump(); // consume ']'
        
        // Mark that we're no longer at start of line
        self.at_start = false;
        
        Some(Token::Image {
            alt: alt_text,
            url,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::markup::lexer::{Lexer, Token, ListKind};

    #[test]
    fn lex_list_item_test() {
        // Test basic lists (numeric, unordered)
        let input = "1. First item\n- Second item\n3. Third";
        let l = Lexer::new(input);
        let tokens: Vec<Token> = l.tokenize();
        println!("\n=== Basic list item test ===");
        for tok in &tokens {
            println!("{:?}", tok);
        }
        
        let ordered = tokens.iter().filter(|t| matches!(t, Token::ListItem { kind: ListKind::OrderedNumeric, .. }));
        let unordered = tokens.iter().filter(|t| matches!(t, Token::ListItem { kind: ListKind::Unordered, .. }));
        println!("\nOrdered: {}", ordered.count());
        println!("Unordered: {}", unordered.count());
    }
    
    #[test]
    fn lex_alpha_roman_test() {
        // Test alpha and roman list detection
        let input = "1. First\n   a. Alpha 1\n   b. Alpha 2\n      i. Roman 1\n      ii. Roman 2\n   c. Alpha 3\n      v. Roman (should be roman)";
        let l = Lexer::new(input);
        let tokens: Vec<Token> = l.tokenize();
        println!("\n=== Alpha/Roman list test ===");
        for tok in &tokens {
            println!("{:?}", tok);
        }
        
        // Verify correct types
        let alpha = tokens.iter().filter(|t| matches!(t, Token::ListItem { kind: ListKind::OrderedAlpha, .. }));
        let roman = tokens.iter().filter(|t| matches!(t, Token::ListItem { kind: ListKind::OrderedRoman, .. }));
        println!("\nAlpha: {}", alpha.count());
        println!("Roman: {}", roman.count());
    }
    
    #[test]
    fn lex_roman_after_alpha_test() {
        // Test that i. after a. (same indent) becomes Alpha
        let input = "1. First\n   a. Alpha 1\n   b. Alpha 2\n   i. After alpha becomes alpha";
        let l = Lexer::new(input);
        let tokens: Vec<Token> = l.tokenize();
        println!("\n=== Roman after Alpha test ===");
        for tok in &tokens {
            println!("{:?}", tok);
        }
    }
    
    #[test]
    fn lex_example() {
        let example = r#"
#1 Existing Parser Syntax Examples

*This is bold text*
_This is italic text_
~This is strikethrough text~

#2 Heading Level 2 Example

#3 Heading Level 3 Example

> Blockquote level 1
>> Blockquote level 2 nested
>>> Blockquote level 3 deepest

This is paragraph one.

This is paragraph two, separated by a double newline.

---
#2 lists 
1. Ordered list item 1
   a. Lower alpha subitem a
   b. Lower alpha subitem b
      i. Lower roman subitem i
      ivv. Custom roman subitem ivv
something. Loose list item (no standard prefix)
3. Out of order ordered item (skipped 2)
2. Unordered numeric item (labeled unordered)
2. You can start unordered



#2 Custom Images
![Alt text for local image | ./assets/logo.png]
![Alt text for remote image | https://example.com/banner.jpg]

#2 Chunkers
Implicit chunk (takes paragraph): 
capture the current block (paragraph or list item) as chunk 


This is the paragraph content for the intro chunk 
:>(intro)

Explicit chunk: :<
This is explicit chunk content inside delimiters 
>:(explicit_chunk)

*/'

explicit_chunk is -> '
This is explicit chunk content inside delimiters 

'/*

#2 Comment Syntax
*/'Comment'/*


#2 directives 

@name(param1, param2, ...)

@name 

@name(
	param1, 
	param2, 
	...
)

directives can not have side effects inside the build system.
directives do not return a value, they are pure functions.
in the future directives will be pluggable 


@name(&chunk_name, "some other string", 123, true)
"#;
        let l = Lexer::new(example);

        println!("asda");

        let tokens = l.tokenize();
        for tok in tokens {
            println!("{:?}", tok);
        }
    }
}
