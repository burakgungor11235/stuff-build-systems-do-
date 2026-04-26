use crate::markup::lexer::handlers::LineLexer;
use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::span::{Span, StrSpan};
use crate::markup::lexer::state::LexerState;
use crate::markup::lexer::token::Token;
use crate::markup::lexer::ListKind;

pub struct ListHandler;

impl ListHandler {
    fn detect_list_kind(marker: &str) -> Option<ListKind> {
        let marker_stripped = marker.trim_end_matches('.').trim();
        
        // Unordered markers
        if marker_stripped == "-" { 
            return Some(ListKind::Unordered);
        }

        // Roman numerals - must have at least one valid roman char
        let roman_chars: String = marker_stripped.chars().filter(|c| c.is_ascii_lowercase()).collect();
        if !roman_chars.is_empty() {
            let is_roman = roman_chars.chars().all(|c| "ivxlcdm".contains(c));
            if is_roman && roman_chars.len() == marker_stripped.len() {
                return Some(ListKind::OrderedRoman);
            }
        }

        // Check for uppercase roman
        let roman_chars_upper: String = marker_stripped.chars().filter(|c| c.is_ascii_uppercase()).collect();
        if !roman_chars_upper.is_empty() {
            let is_roman = roman_chars_upper.chars().all(|c| "IVXLCDM".contains(c));
            if is_roman && roman_chars_upper.len() == marker_stripped.len() {
                return Some(ListKind::OrderedRoman);
            }
        }

        // Alpha (single letter a-z or A-Z)
        if marker_stripped.len() == 1 && marker_stripped.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
            return Some(ListKind::OrderedAlpha);
        }

        // Numeric - must be all digits
        if marker_stripped.chars().all(|c| c.is_ascii_digit()) {
            return Some(ListKind::OrderedNumeric);
        }

        // Invalid marker - not a recognized list type
        None
    }
}

impl LineLexer for ListHandler {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool {
        if !state.at_line_start {
            return false;
        }

        let trimmed = line.trim_start();

        // Check for common list markers - must be followed by space or end of line
        if trimmed.starts_with('-') || trimmed.starts_with('+') {
            let rest = &trimmed[1..];
            return rest.is_empty() || rest.starts_with(' ');
        }
        
        // For * we need to be more careful - it could be bold inline or list
        if let Some(rest) = trimmed.strip_prefix('*') {
            // List: followed by space or end of line
            // Not list: followed by alphanumeric (inline formatting)
            return rest.is_empty() || rest.starts_with(' ');
        }

        // Check for ordered markers: number., letter., roman.
        if let Some(dot_pos) = trimmed.find('.') {
            if dot_pos > 0 {
                let before_dot = &trimmed[..dot_pos];
                // Check if it's a number, single letter, or roman numeral
                if before_dot.chars().all(|c| c.is_ascii_digit()) {
                    return true; // numeric
                }
                if before_dot.len() == 1 && before_dot.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                    return true; // alpha
                }
                // Roman check - simple heuristic
                if before_dot.chars().all(|c| "ivxlcdmIVXLCDM".contains(c)) && !before_dot.is_empty() {
                    return true; // roman
                }
            }
        }

        false
    }

    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink) {
        let leading_spaces = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();

        // Extract marker
        let marker_end ;
        let mut marker = String::new();

        // Handle unordered markers
        if trimmed.starts_with('-') {
            marker = trimmed[..1].to_string();
            // Check for " - " style
            if trimmed.len() > 1 && trimmed.chars().nth(1) == Some(' ') {
                marker.push(' ');
            } 
        }
        // Handle ordered markers
        else if let Some(dot_pos) = trimmed.find('.') {
            marker_end = dot_pos + 1;
            marker = trimmed[..marker_end].to_string();
        }

        // Determine list kind
        let kind = match Self::detect_list_kind(&marker) {
            Some(k) => k,
            None => {
                // Invalid marker - emit as text instead
                let text = StrSpan::new(line);
                sink.emit(Token::Text(text));
                state.clear_line_start();
                return;
            }
        };
        
        // Track state for list continuation (clone for state)
        state.prev_list_indent = Some(leading_spaces);
        state.prev_list_kind = Some(kind.clone());

        // Build marker span
        let marker_span = StrSpan::new(&line[..leading_spaces + marker.len()]);
        let span = Span::new(0, leading_spaces + marker.len(), 1, 1);

        sink.emit(Token::ListItem {
            indent: leading_spaces,
            kind,
            marker: marker_span,
            span,
        });
        state.clear_line_start();

        // Emit content after marker as text
        let content_start = leading_spaces + marker.len();
        if content_start < line.len() {
            let content = StrSpan::new(&line[content_start..]);
            sink.emit(Token::Text(content));
        }
    }

    fn priority(&self) -> u8 {
        180 // After HR, heading, blockquote
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_list(input: &str) -> Vec<Token<'static>> {
        let handler = ListHandler;
        let state = LexerState::new();
        
        #[derive(Default)]
        struct TestSink { tokens: Vec<Token<'static>> }
        
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

    fn parse_multiline(input: &str) -> Vec<Token<'static>> {
        let handler = ListHandler;
        let mut state = LexerState::new();
        
        #[derive(Default)]
        struct TestSink { tokens: Vec<Token<'static>> }
        
        impl TokenSink for TestSink {
            fn emit(&mut self, token: Token<'_>) {
                let token_static = unsafe {
                    std::mem::transmute::<Token<'_>, Token<'static>>(token)
                };
                self.tokens.push(token_static);
            }
        }
        
        let mut sink = TestSink::default();
        for line in input.lines() {
            state.reset_line_state();
            handler.lex(line, &mut state, &mut sink);
        }
        sink.tokens
    }

    #[test]
    fn test_list_numeric() {
        let tokens = lex_list("1. First item");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::OrderedNumeric, .. }));
    }

    #[test]
    fn test_list_alpha() {
        let tokens = lex_list("a. Alpha item");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::OrderedAlpha, .. }));
    }

    #[test]
    fn test_list_roman() {
        let tokens = lex_list("i. Roman item");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::OrderedRoman, .. }));
    }

    #[test]
    fn test_list_unordered() {
        let tokens = lex_list("- Unordered item");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::Unordered, .. }));
    }
    #[test]
    fn test_list_numeric_leading_zero() {
        let tokens = lex_list("01. Leading zero");
        // Should this be allowed?
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_list_numeric_zero() {
        let tokens = lex_list("0. Zero item");
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_list_large_number() {
        let tokens = lex_list("999. Large number");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::OrderedNumeric, .. }));
    }

    #[test]
    fn test_list_alpha_uppercase() {
        let tokens = lex_list("A. Uppercase letter");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::OrderedAlpha, .. }));
    }

    #[test]
    fn test_list_alpha_double() {
        let tokens = lex_list("aa. Double letter");
        // "aa" - not single letter, what happens?
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_list_roman_uppercase() {
        let tokens = lex_list("II. Uppercase roman");
        assert!(matches!(tokens[0], Token::ListItem { kind: ListKind::OrderedRoman, .. }));
    }

    #[test]
    fn test_list_roman_complex() {
        let tokens = lex_list("MMMMM. Invalid roman");
        // What happens with non-roman chars?
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_list_no_space_after_marker() {
        let tokens = lex_list("1.No space");
        // Marker with no space
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_list_only_marker() {
        let tokens = lex_list("1. ");
        assert!(matches!(tokens[0], Token::ListItem { .. }));
    }

    #[test]
    fn test_list_leading_spaces() {
        let tokens = lex_list("   1. With leading spaces");
        // Should still be recognized as list
        assert!(!tokens.is_empty());
    }


    #[test]
    fn test_list_indent_numeric() {
        let tokens = lex_list("1. First");
        assert!(matches!(tokens[0], Token::ListItem { indent: 0, .. }));
    }

    #[test]
    fn test_list_indent_two_spaces() {
        let tokens = lex_list("  1. Two spaces");
        assert!(matches!(tokens[0], Token::ListItem { indent: 2, .. }));
    }

    #[test]
    fn test_list_indent_four_spaces() {
        let tokens = lex_list("    1. Four spaces");
        assert!(matches!(tokens[0], Token::ListItem { indent: 4, .. }));
    }

    #[test]
    fn test_list_indent_mixed() {
        let tokens = lex_list("       7. Seven spaces");
        assert!(matches!(tokens[0], Token::ListItem { indent: 7, .. }));
    }

    #[test]
    fn test_list_indent_with_tab() {
        let tokens = lex_list("\t1. Tab indent");
        // Tab counts as 1 char, "1." is a valid numeric marker
        assert!(matches!(tokens[0], Token::ListItem { indent: 1, .. }));
    }

    #[test]
    fn test_list_indent_unordered() {
        let tokens = lex_list("    - Nested unordered");
        assert!(matches!(tokens[0], Token::ListItem { indent: 4, .. }));
    }

    #[test]
    fn test_invalid_marker_emits_text() {
        let tokens = lex_list("          notalist. should be text");
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_invalid_marker_with_dot_emits_text() {
        let tokens = lex_list("     random. invalid marker");
        assert!(matches!(tokens[0], Token::Text(_)));
    }

    #[test]
    fn test_mixed_invalid_emits_text() {
        let tokens = lex_list("abc123. not a valid list");
        assert!(matches!(tokens[0], Token::Text(_)));
    }


    #[test]
    fn test_multiple_ordered_items() {
        let tokens = parse_multiline("1. First\n2. Second\n3. Third");
        assert_eq!(tokens.len(), 6); // 3 ListItem + 3 Text
    }

    #[test]
    fn test_multiple_unordered_items() {
        let tokens = parse_multiline("- Item A\n    - Item B\n- Item C");
        assert_eq!(tokens.len(), 6);
    }

    #[test]
    fn test_mixed_list_types() {
        let tokens = parse_multiline("- Unordered one\n1. Ordered two\n- Unordered three");
        // All should be recognized as lists
        assert!(tokens.iter().any(|t| matches!(t, Token::ListItem { kind: ListKind::Unordered, .. })));
        assert!(tokens.iter().any(|t| matches!(t, Token::ListItem { kind: ListKind::OrderedNumeric, .. })));
    }

    #[test]
    fn test_nested_indent_levels() {
        let tokens = parse_multiline("- Top level\n  - Nested one\n    - Nested two");
        // Nested should have greater indent
        let list_items: Vec<_> = tokens.iter().filter_map(|t| match t {
            Token::ListItem { indent, .. } => Some(indent),
            _ => None,
        }).collect();
        assert!(list_items[0] <= list_items[1]);
        assert!(list_items[1] <= list_items[2]);
    }

    #[test]
    fn test_different_alpha_markers() {
        let tokens = parse_multiline("a. Alpha one\nb. Alpha two\nc. Alpha three");
        assert_eq!(tokens.len(), 6);
    }

    #[test]
    fn test_different_roman_markers() {
        let tokens = parse_multiline("i. Roman one\nii. Roman two\niii. Roman three");
        assert_eq!(tokens.len(), 6);
    }

    #[test]
    fn test_mixed_alpha_roman() {
        let tokens = parse_multiline("a. Alpha\ni. Roman\nb. Alpha");
        assert!(tokens.iter().any(|t| matches!(t, Token::ListItem { kind: ListKind::OrderedAlpha, .. })));
        assert!(tokens.iter().any(|t| matches!(t, Token::ListItem { kind: ListKind::OrderedRoman, .. })));
    }
}
