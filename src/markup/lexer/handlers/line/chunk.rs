/*!
Chunk syntax handler

Chunk syntax (chunk = named block of content):
i

| Syntax | Meaning | Token |
|--------|---------|-------|
| `:>name` | Explicit chunk, captures inline content | ChunkExplicit |
| `:> name` | Explicit chunk (space optional) | ChunkExplicit |
| `:>(name)` | Implicit chunk, captures next block | ChunkStartImplicit |
| `:>(name) rest` | Implicit chunk + rest as text | ChunkStartImplicit + Text |
| `:> (name)` | Text (space before parens) | Text |
| `:>` alone | Text (no name) | Text |
| `:<` | Explicit chunk start (multi-line) | ChunkStartExplicit |
| `>:(name)` | Chunk end | ChunkEnd |
| `>:name` | Chunk end (no parens) | ChunkEnd |

*/

use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::{Span, StrSpan};
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;

pub struct ChunkHandler;

impl ChunkHandler {
    /// Extract identifier from chunk syntax
    /// Handles: `:>name`, `:> name`, `>:name`, `>:name)`, etc.
    /// Returns (name, remaining_content)
    fn extract_identifier(content: &str) -> Option<(&str, &str)> {
        let trimmed = content.trim_start();
        let leading_spaces = content.len() - trimmed.len();
        
        let rest = if leading_spaces > 0 {
            trimmed
        } else {
            content
        };

        if rest.is_empty() {
            return None;
        }

        // Handle (name) format - strip parens
        if rest.starts_with('(') {
            let inner = &rest[1..];
            if let Some(end) = inner.find(')') {
                let name = &inner[..end];
                let remaining = &inner[end + 1..];
                return Some((name, remaining));
            }
            // Unclosed paren - treat as text
            return None;
        }

        // Handle name format - alphanumeric + underscore + hyphen
        let mut end = 0;
        for (i, ch) in rest.char_indices() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                end = i + ch.len_utf8();
            } else {
                break;
            }
        }

        if end > 0 {
            let name = &rest[..end];
            let remaining = &rest[end..];
            Some((name, remaining))
        } else {
            None
        }
    }
}

impl LineLexer for ChunkHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        if !state.at_line_start {
            return false;
        }

        // Match specific chunk prefixes only
        line.starts_with(":>") ||    // :>name or :>(name)
        line.starts_with(":<") ||    // :< for explicit multi-line
        line.starts_with(">:")       // >:name or >:(name)
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let line = line.trim_end();
        let span = Span::new(0, line.len(), 1, 1);

        // 1. :>(name) - Implicit chunk start (capture next block)
        if line.starts_with(":>(") {
            let rest = &line[3..]; // Skip ":>("
            if let Some(end) = rest.find(')') {
                let name = &rest[..end];
                let remaining = &rest[end + 1..];
                
                let name_span = StrSpan::new(name);
                sink.emit(Token::ChunkStartImplicit { name: name_span, span });
                
                if !remaining.trim().is_empty() {
                    sink.emit(Token::Text(StrSpan::new(remaining.trim())));
                }
                state.clear_line_start();
                return;
            }
            // Unclosed paren - fall through to text
        }

        // 2. :>name or :> name - Explicit chunk (single line)
        if line.starts_with(":>") {
            let rest = &line[2..]; // Skip ":>"
            
            if rest.is_empty() {
                // :> alone → text
                sink.emit(Token::Text(StrSpan::new(line)));
                state.clear_line_start();
                return;
            }

            // Check for space before parens - if so, it's text
            // :> (name) → Text (space before parens)
            // :>name → ChunkExplicit
            // :> name → ChunkExplicit (space is optional)
            let trimmed = rest.trim_start();
            let had_leading_space = trimmed.len() != rest.len();
            
            if had_leading_space && trimmed.starts_with('(') {
                // :> (something) → text
                sink.emit(Token::Text(StrSpan::new(line)));
                state.clear_line_start();
                return;
            }

            // :>name or :> name → ChunkExplicit
            if let Some((name, remaining)) = Self::extract_identifier(rest) {
                let name_span = StrSpan::new(name);
                sink.emit(Token::ChunkExplicit { name: name_span, span });
                
                if !remaining.is_empty() {
                    sink.emit(Token::Text(StrSpan::new(remaining)));
                }
                state.clear_line_start();
                return;
            }

            // Couldn't parse identifier - treat as text
            sink.emit(Token::Text(StrSpan::new(line)));
            state.clear_line_start();
            return;
        }

        // 3. :< - Explicit chunk start (multi-line)
        if line.starts_with(":<") {
            state.in_explicit_chunk = true;
            sink.emit(Token::ChunkStartExplicit);
            
            // Any content after :< is text
            let rest = &line[2..];
            if !rest.is_empty() {
                sink.emit(Token::Text(StrSpan::new(rest)));
            }
            state.clear_line_start();
            return;
        }

        // 4. >: - Chunk end (explicit chunk end)
        if line.starts_with(">:") {
            let rest = &line[2..]; // Skip ">:"
            
            // Parse name from rest (handles both "name" and "(name)" formats)
            let name = if rest.starts_with('(') {
                // >:(name) → extract "name"
                let inner = &rest[1..];
                inner.find(')').map(|end| &inner[..end]).unwrap_or(
                    &inner[..inner.len().saturating_sub(1)]
                )
            } else {
                // >:name → take identifier
                let mut end = 0;
                for (i, ch) in rest.char_indices() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                        end = i + ch.len_utf8();
                    } else {
                        break;
                    }
                }
                if end > 0 {
                    &rest[..end]
                } else {
                    rest
                }
            };

            if !name.is_empty() {
                let name_span = StrSpan::new(name);
                sink.emit(Token::ChunkEnd { name: name_span, span });
            }
            state.in_explicit_chunk = false;
            state.clear_line_start();
            return;
        }

        // Fallback (shouldn't reach here with proper can_handle)
        sink.emit(Token::Text(StrSpan::new(line)));
        state.clear_line_start();
    }

    fn priority(&self) -> u8 {
        200 // Above blockquote - chunk markers take precedence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_chunk(input: &str) -> Vec<Token<'static>> {
        let handler = ChunkHandler;
        let state = LexerState::new();
        
        #[derive(Default)]
        struct TestSink {
            tokens: Vec<Token<'static>>,
        }
        
        impl TokenSink for TestSink {
            fn emit(&mut self, token: Token<'_>) {
                let token_static = unsafe {
                    std::mem::transmute::<Token<'_>, Token<'static>>(token)
                };
                self.tokens.push(token_static);
            }
        }
        
        let mut sink = TestSink::default();
        handler.lex(input, &mut state.clone(), &mut sink);
        println!("{} : {:?}", input, sink.tokens);
        sink.tokens
    }

    // Implicit chunk: :>(name)
    #[test]
    fn test_implicit_chunk_simple() {
        let tokens = lex_chunk(":>(mychunk)");
        assert!(matches!(
            tokens[0],
            Token::ChunkStartImplicit { name, .. } if name.as_str() == "mychunk"
        ));
    }

    #[test]
    fn test_implicit_chunk_with_content() {
        let tokens = lex_chunk(":>(mychunk) some text");
        println!("Tokens: {:?}", tokens);
        assert!(matches!(
            tokens[0],
            Token::ChunkStartImplicit { name, .. } if name.as_str() == "mychunk"
        ));
        // Remaining content after ) should be text
        assert!(tokens.len() >= 2);
    }

    #[test]
    fn test_implicit_chunk_spaces_in_name() {
        let tokens = lex_chunk(":>(my chunk name)");
        assert!(matches!(
            tokens[0],
            Token::ChunkStartImplicit { name, .. } if name.as_str() == "my chunk name"
        ));
    }

    // Explicit chunk: :>name
    #[test]
    fn test_explicit_chunk_simple() {
        let tokens = lex_chunk(":>mychunk");
        assert!(matches!(
            tokens[0],
            Token::ChunkExplicit { name, .. } if name.as_str() == "mychunk"
        ));
    }

    #[test]
    fn test_explicit_chunk_with_space() {
        let tokens = lex_chunk(":> mychunk");
        assert!(matches!(
            tokens[0],
            Token::ChunkExplicit { name, .. } if name.as_str() == "mychunk"
        ));
    }

    #[test]
    fn test_explicit_chunk_with_content() {
        let tokens = lex_chunk(":>name some content");
        assert!(matches!(
            tokens[0],
            Token::ChunkExplicit { name, .. } if name.as_str() == "name"
        ));
        assert!(matches!(
            tokens[1],
            Token::Text(t) if t.as_str() == " some content"
        ));
    }

    //  should be text
    #[test]
    fn test_colon_gt_alone() {
        let tokens = lex_chunk(":>");
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_space_before_parens() {
        let tokens = lex_chunk(":> (name)");
        println!("Tokens for ':> (name)': {:?}", tokens);
        // :> (name) should be text because of space before parens
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    // Explicit start: :<
    #[test]
    fn test_explicit_start_simple() {
        let tokens = lex_chunk(":<");
        assert!(matches!(tokens[0], Token::ChunkStartExplicit));
    }

    #[test]
    fn test_explicit_start_with_content() {
        let tokens = lex_chunk(":<content");
        assert!(matches!(tokens[0], Token::ChunkStartExplicit));
        assert!(matches!(tokens[1], Token::Text(t) if t.as_str() == "content"));
    }

    // Chunk end: >:
    #[test]
    fn test_chunk_end_with_parens() {
        let tokens = lex_chunk(">:(myname)");
        assert!(matches!(
            tokens[0],
            Token::ChunkEnd { name, .. } if name.as_str() == "myname"
        ));
    }

    #[test]
    fn test_chunk_end_without_parens() {
        let tokens = lex_chunk(">:Heisenberg");
        assert!(matches!(
            tokens[0],
            Token::ChunkEnd { name, .. } if name.as_str() == "Heisenberg"
        ));
    }

    #[test]
    fn test_chunk_end_with_content() {
        let tokens = lex_chunk(">:myname extra");
        assert!(matches!(
            tokens[0],
            Token::ChunkEnd { name, .. } if name.as_str() == "myname"
        ));
    }

    #[test]
    fn test_chunk_end_unclosed_paren() {
        // >:(name without closing → name should be extracted anyway
        let tokens = lex_chunk(">:(incomplete");
        // The parser should handle this gracefully
        assert!(!tokens.is_empty());
    }
}
