mod bs;
mod markup;
mod repl;

use anyhow::{Ok, Result};
use bs::clean::Cleaner;
use bs::status::Status;
use bs::{builder::Builder, config::Manifest};
use clap::{Parser, Subcommand};
use std::env;
use std::mem::forget;
use std::process::Command;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::bs::browser_utils::open_url;
use crate::bs::watch::stuff_watcher;

#[derive(Parser)]
#[command(name = "sbd")]
#[command(
    about = "stuff buildsystems do",
    long_about = "It does stuff that what normal build systems do"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        /// Configuration file (default: ./stuff.toml)
        #[arg(short, long, default_value = "./stuff.toml")]
        config: String,

        /// Enable watch mode for hot reload
        #[arg(short, long)]
        watch: bool,

        #[arg(short, long)]
        open: bool,
    },
    Clean {
        /// Configuration file (default: ./stuff.toml)
        #[arg(short, long, default_value = "./stuff.toml")]
        config: String,
    },
    Status {
        /// Configuration file (default: ./stuff.toml)
        #[arg(short, long, default_value = "./stuff.toml")]
        config: String,

        /**
        Increase verbosity -v, -vv
        (to be honest we need a better and unified system but right now it's ok)
        */
        #[arg(short, long, action = clap::ArgAction::Count)]
        verbose: u8,
    },
    /// Interactive REPL for debugging markup
    Repl,
}

fn setup_logging() -> Result<()> {
    let log_dir = env::var("SBD_LOG_DIR").ok();

    let log_level = env::var("SBD_LOG")
        .or_else(|_| env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());

    let filter = EnvFilter::new(log_level);

    let base = tracing_subscriber::registry().with(filter);

    match log_dir {
        Some(dir) => {
            let file_appender = tracing_appender::rolling::daily(&dir, "sbd.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            forget(guard);

            base.with(fmt::layer().with_writer(non_blocking).with_ansi(false))
                .init();
        }
        None => {
            base.with(fmt::layer().pretty()).init();
        }
    }

    info!("SBD starting up");
    Ok(())
}

fn main() -> Result<()> {
    setup_logging()?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            config,
            watch,
            open,
        } => {
            if watch {
                info!("watch mode started!");
                stuff_watcher(&config, open)
            } else {
                let manifest = Manifest::load(&config)?;
                let entry_out = if open {
                    let rel = manifest.project.entry_point_rel_out()?;
                    Some(manifest.project.out_dir_path().join(rel))
                } else {
                    None
                };
                let mut builder = Builder::new(manifest)?;
                builder.build()?;
                if let Some(path) = entry_out {
                    open_url(path.to_str().unwrap())?;
                }
                Ok(())
            }
        }
        Commands::Clean { config } => {
            let manifest = Manifest::load(&config)?;
            let cleaner = Cleaner::new(manifest);
            cleaner.clean()?;
            Ok(())
        }
        Commands::Status { config, verbose } => {
            let manifest = Manifest::load(&config)?;
            let status = Status::new(manifest);
            status.show(verbose)?;
            Ok(())
        }
        Commands::Repl => {
            repl::run()?;
            Ok(())
        }
    }
}
