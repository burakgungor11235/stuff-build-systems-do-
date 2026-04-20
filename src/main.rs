mod bs;
mod markup;

use anyhow::Result;
use bs::{builder::Builder, config::Manifest};
use std::env;
use std::mem::forget;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    let log_dir = env::var("SBD_LOG_DIR").ok();
    
    let log_level = env::var("SBD_LOG")
        .or_else(|_| env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info".to_string());

    let filter = EnvFilter::new(log_level);

    let base = tracing_subscriber::registry()
        .with(filter);

    match log_dir {
        Some(dir) => {
            let file_appender = tracing_appender::rolling::daily(&dir, "sbd.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            forget(guard);
            
            base.with(fmt::layer().with_writer(non_blocking).with_ansi(false))
                .init();
        }
        None => {
            base.with(fmt::layer().pretty())
                .init();
        }
    }

    info!("SBD starting up");

    let manifest = Manifest::load("test_envs/basic/stuff.toml")?;
    let mut builder = Builder::new(manifest)?;
    builder.build()?;

    Ok(())
}