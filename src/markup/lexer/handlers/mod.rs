use crate::markup::lexer::sink::TokenSink;
use crate::markup::lexer::state::LexerState;

pub mod line;

pub trait LineLexer: Send + Sync {
    fn can_handle(&self, line: &str, state: &LexerState) -> bool;
    fn lex(&self, line: &str, state: &mut LexerState, sink: &mut dyn TokenSink);
    fn priority(&self) -> u8 {
        128
    }
}