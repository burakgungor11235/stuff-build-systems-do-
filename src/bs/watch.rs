use std::ffi::OsStr;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{error, info};

use crate::bs::{builder::Builder, config::Manifest};


// it watches stuff.
//
// Amazing I know.
pub fn stuff_watcher(config_path: &str) -> anyhow::Result<()> {
    let manifest = Manifest::load(config_path)?;
    let src_dir = manifest.project.src_dir_path();

    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&src_dir, RecursiveMode::Recursive)?;

    info!("Watching {} for changes...", src_dir.display());

    let config_owned = config_path.to_owned();
    let mut last_build = Instant::now();

    for res in rx {
        let event = match res {
            Ok(e) => e,
            Err(e) => {
                error!("Watch error: {:?}", e);
                continue;
            }
        };

        if !event
            .paths
            .iter()
            .any(|p| p.extension() == Some(OsStr::new("stuff")))
        {
            continue;
        }

        if last_build.elapsed() < Duration::from_millis(200) {
            continue;
        }
        last_build = Instant::now();

        info!("Change detected, rebuilding...");
        match Manifest::load(&config_owned) {
            Ok(m) => match Builder::new(m).and_then(|mut b| b.build()) {
                Ok(()) => info!("Rebuild complete"),
                Err(e) => error!("Build failed: {}", e),
            },
            Err(e) => error!("Failed to load config: {}", e),
        }
    }

    Ok(())
}
