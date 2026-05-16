mod highlight;
mod input_highlighter;

use crate::markup::{assembler, lexer::Token, parser};
use crate::bs::registry::ChunkRegistry;
use logos::Logos;
use rustyline::Editor;
use std::collections::HashMap;

pub fn run() -> anyhow::Result<()> {
    println!("SBD REPL aka SRD (stuff repl do) ");
    println!("Type :help for commands, :quit to exit\n");

    let mut rl = Editor::<input_highlighter::InputHighlighter, _>::new()?;
    rl.set_helper(Some(input_highlighter::InputHighlighter));
    rl.load_history(".sbd_history").ok();

    let mut mode = OutputMode::Html;
    let mut memory: HashMap<String, String> = HashMap::new();
    let mut collecting: Option<CollectingState> = None;
    let mut next_auto_num = 1;

    loop {
        let prompt = match &collecting {
            Some(c) => format!("{}> ", c.name),
            None => "> ".to_string(),
        };

        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let line_trimmed = line.trim();
                rl.add_history_entry(&line)?;

                match parse_command(line_trimmed) {
                    Command::Quit => {
                        println!("Goodbye!");
                        break;
                    }
                    Command::Help => show_help(),
                    Command::Mode(m) => {
                        mode = m;
                        println!("Output mode set to: {:?}", mode);
                    }
                    Command::Clear => {
                        print!("\x1B[2J\x1B[1H");
                        println!("SBD REPL - Markup Debugging Console");
                    }
                    Command::MemoryList => {
                        if memory.is_empty() {
                            println!("Memory is empty.");
                        } else {
                            println!("Memory entries:");
                            for (name, content) in &memory {
                                let preview = if content.len() > 40 {
                                    format!("{}...", &content[..40])
                                } else {
                                    content.clone()
                                };
                                println!("  {}: {}", name, preview);
                            }
                        }
                    }
                    Command::MemoryShow(name) => {
                        if let Some(content) = memory.get(&name) {
                            println!("{}", content);
                        } else {
                            println!("Memory entry '{}' not found.", name);
                        }
                    }
                    Command::MemoryDelete(name) => {
                        if memory.remove(&name).is_some() {
                            println!("Deleted '{}'.", name);
                        } else {
                            println!("Memory entry '{}' not found.", name);
                        }
                    }
                    Command::MemoryClear => {
                        memory.clear();
                        println!("Memory cleared.");
                    }
                    Command::Begin(name) => {
                        if collecting.is_some() {
                            println!("Already collecting. Use :end to finish.");
                        } else {
                            let entry_name = if name.is_empty() {
                                let auto_name = format!("{}", next_auto_num);
                                next_auto_num += 1;
                                auto_name
                            } else {
                                name
                            };

                            if memory.contains_key(&entry_name) {
                                println!("Memory '{}' exists. Overwrite? (y/n): ", entry_name);
                                let confirm = rl.readline("> ")?;
                                if confirm.trim().to_lowercase() == "y" {
                                    collecting = Some(CollectingState {
                                        name: entry_name,
                                        buffer: String::new(),
                                    });
                                    println!(
                                        "(collecting into '{}', type :end to finish)",
                                        collecting.as_ref().unwrap().name
                                    );
                                } else {
                                    println!("Aborted.");
                                }
                            } else {
                                collecting = Some(CollectingState {
                                    name: entry_name.clone(),
                                    buffer: String::new(),
                                });
                                println!("(collecting into '{}', type :end to finish)", entry_name);
                            }
                        }
                    }
                    Command::End => {
                        if let Some(c) = collecting.take() {
                            memory.insert(c.name.clone(), c.buffer);
                            println!("Saved to memory as '{}'.", c.name);
                        } else {
                            println!("Error: Not currently collecting. Use :beg to start.");
                        }
                    }
                    Command::ProcessTokens(name) => {
                        if let Some(content) = memory.get(&name) {
                            let tokens: Vec<Token> =
                                Token::lexer(content).filter_map(|t| t.ok()).collect();
                            println!("{}", highlight::highlight_tokens(&tokens));
                        } else {
                            println!("Memory entry '{}' not found.", name);
                        }
                    }
                    Command::ProcessHtml(name) => {
                        if let Some(content) = memory.get(&name) {
                            let doc = parser::parse(content);
                            let reg = ChunkRegistry::default();
                            let ctx = assembler::RenderContext::new(&name, 0, &reg);
                            let html = assembler::render_to_html(&doc, &ctx);
                            println!("{}", highlight::highlight_html(&html));
                        } else {
                            println!("Memory entry '{}' not found.", name);
                        }
                    }
                    Command::ProcessAst(name) => {
                        if let Some(content) = memory.get(&name) {
                            let doc = parser::parse(content);
                            println!("{}", highlight::highlight_ast(&doc));
                        } else {
                            println!("Memory entry '{}' not found.", name);
                        }
                    }
                    Command::Empty => {}
                    Command::Markup => {
                        if let Some(ref mut c) = collecting {
                            if !c.buffer.is_empty() {
                                c.buffer.push('\n');
                            }
                            c.buffer.push_str(&line);
                        } else {
                            let output = process_markup(line_trimmed, &mode);
                            if !output.is_empty() {
                                println!("{}", output);
                            }
                        }
                    }
                    Command::Unknown(cmd) => {
                        println!("Unknown command: :{}", cmd);
                    }
                }
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("\nGoodbye!");
                break;
            }
            Err(_) => {
                println!("Goodbye!");
                break;
            }
        }
    }

    rl.save_history(".sbd_history").ok();
    Ok(())
}

#[derive(Debug)]
enum OutputMode {
    Html,
    Tokens,
    Ast,
}

struct CollectingState {
    name: String,
    buffer: String,
}

enum Command {
    Quit,
    Help,
    Mode(OutputMode),
    Clear,
    MemoryList,
    MemoryShow(String),
    MemoryDelete(String),
    MemoryClear,
    Begin(String),
    End,
    ProcessTokens(String),
    ProcessHtml(String),
    ProcessAst(String),
    Empty,
    Markup,
    Unknown(String),
}

fn parse_command(line: &str) -> Command {
    if line.is_empty() {
        return Command::Empty;
    }
    if !line.starts_with(':') {
        return Command::Markup;
    }

    let rest = &line[1..]; // strip ':'
    let mut parts = rest.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap().to_lowercase();
    let args = parts.next().unwrap_or("").to_string();

    match cmd.as_str() {
        "quit" | "q" => Command::Quit,
        "help" | "h" | "?" => Command::Help,
        "clear" | "cls" => Command::Clear,
        "mem" if args.is_empty() => Command::MemoryList,
        "mem" => Command::MemoryShow(args),
        "del" | "d" if args.is_empty() => Command::MemoryClear,
        "del" | "d" => Command::MemoryDelete(args),
        "beg" | "b" if args.is_empty() => Command::Begin(String::new()),
        "beg" | "b" => Command::Begin(args),
        "end" => Command::End,
        // Mode-changing commands
        "html" if args.is_empty() => Command::Mode(OutputMode::Html),
        "html" => Command::ProcessHtml(args),
        "tokens" | "tok" if args.is_empty() => Command::Mode(OutputMode::Tokens),
        "tokens" | "tok" => Command::ProcessTokens(args),
        "ast" if args.is_empty() => Command::Mode(OutputMode::Ast),
        "ast" => Command::ProcessAst(args),
        _ => Command::Unknown(cmd),
    }
}

fn show_help() {
    println!("Commands:");
    println!("  :help, :h             - Show this help");
    println!("  :quit, :q             - Exit REPL");
    println!("  :clear, :cls          - Clear screen");
    println!();
    println!("Output modes:");
    println!("  :html                 - Show rendered HTML (default)");
    println!("  :tokens, :tok         - Show tokenized output");
    println!("  :ast                  - Show parsed AST");
    println!();
    println!("Memory (multi-line):");
    println!("  :beg, :b              - Start collecting (auto-numbered if no name)");
    println!("  :beg(name), :b(name)  - Start collecting into 'name'");
    println!("  :end                  - Finish collecting, save to memory");
    println!("  :tok(name)            - Tokenize memory entry 'name'");
    println!("  :html(name)           - Render memory entry as HTML");
    println!("  :ast(name)            - Show AST for memory entry");
    println!("  :mem                  - List all memory entries");
    println!("  :mem(name)            - Show content of memory entry");
    println!("  :del(name), :d(name)  - Delete memory entry");
    println!("  :del                  - Clear all memory");
    println!();
    println!("Direct input is processed immediately (not stored).");
}

fn process_markup(source: &str, mode: &OutputMode) -> String {
    let reg = ChunkRegistry::default();
    let ctx = assembler::RenderContext::new("inline", 0, &reg);
    match mode {
        OutputMode::Tokens => {
            let tokens: Vec<Token> = Token::lexer(source).filter_map(|t| t.ok()).collect();
            highlight::highlight_tokens(&tokens)
        }
        OutputMode::Ast => {
            let doc = parser::parse(source);
            highlight::highlight_ast(&doc)
        }
        OutputMode::Html => {
            let doc = parser::parse(source);
            let html = assembler::render_to_html(&doc, &ctx);
            highlight::highlight_html(&html)
        }
    }
}