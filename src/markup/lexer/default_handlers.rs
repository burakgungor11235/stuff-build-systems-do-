use crate::markup::lexer::handlers::line::blockquote::BlockquoteHandler;
use crate::markup::lexer::handlers::line::chunk::ChunkHandler;
use crate::markup::lexer::handlers::line::comment::CommentHandler;
use crate::markup::lexer::handlers::line::directive::DirectiveHandler;
use crate::markup::lexer::handlers::line::heading::HeadingHandler;
use crate::markup::lexer::handlers::line::horizontal_rule::HorizontalRuleHandler;
use crate::markup::lexer::handlers::line::list::ListHandler;
use crate::markup::lexer::handlers::line::paragraph::ParagraphHandler;
use crate::markup::lexer::handlers::LineLexer;

pub fn default_line_handlers() -> Vec<Box<dyn LineLexer>> {
    vec![
        Box::new(HorizontalRuleHandler),
        Box::new(CommentHandler),
        Box::new(HeadingHandler),
        Box::new(ChunkHandler),
        Box::new(BlockquoteHandler),
        Box::new(ListHandler),
        Box::new(DirectiveHandler),
        Box::new(ParagraphHandler),
    ]
}
