/*
mod bs;
mod markup;

use anyhow::Result;
use bs::clean::Cleaner;
use bs::status::Status;
use bs::{builder::Builder, config::Manifest};
use clap::{Parser, Subcommand};
use std::env;
use std::mem::forget;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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
        Commands::Build { config, watch } => {
            if watch {
                // TODO: Implement watch mode
                // TODO: Implement cache error detection (files deleted in cache improperly)
                Err(anyhow::anyhow!("Watch mode not yet implemented"))
            } else {
                let manifest = Manifest::load(&config)?;
                let mut builder = Builder::new(manifest)?;
                builder.build()?;
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
    }
}
*/

fn main() {
    todo!();
}
