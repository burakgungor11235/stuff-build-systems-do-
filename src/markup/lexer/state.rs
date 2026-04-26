use super::ListKind;

#[derive(Debug, Clone, Default)]
pub struct LexerState {
    pub at_line_start: bool,
    pub prev_list_indent: Option<usize>,
    pub prev_list_kind: Option<ListKind>,
    pub last_was_newline: bool,
    pub in_explicit_chunk: bool,
}

impl LexerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset_line_state(&mut self) {
        self.at_line_start = true;
    }

    pub fn clear_line_start(&mut self) {
        self.at_line_start = false;
    }
}