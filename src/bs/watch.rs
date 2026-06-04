use std::ffi::OsStr;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{error, info};

use rustc_hash::FxHashMap;

use crate::bs::browser_utils::handle_client;
use crate::bs::live::{LiveReload, LockExt};
use crate::bs::{builder::Builder, config::Manifest};

type FileHash = Arc<Mutex<Option<FxHashMap<String, Vec<u8>>>>>;

struct BuildGuard(Arc<AtomicBool>);
impl Drop for BuildGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct WatchContext {
    config_path: String,
    running: Arc<AtomicBool>,
    prev_hashes: FileHash,
    live_reload: Arc<LiveReload>,
}

pub fn stuff_watcher(config_path: &str) -> anyhow::Result<()> {
    let manifest = Manifest::load(config_path)?;
    let src_dir = manifest.project.src_dir_path();
    let out_dir = manifest.project.out_dir_path();

    let live_reload = LiveReload::new();

    info!("starting initial build");
    let mut builder = Builder::new(manifest)?;
    builder.set_live_reload(true);
    builder.build()?;
    info!("finished initial build");

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    info!("oki dokey. running at {addr}");

    let out_dir_srv = out_dir.clone();
    let live_reload_srv = live_reload.clone();

    let server_handle = thread::spawn(move || {
        listener.incoming().for_each(|stream| match stream {
            Ok(stream) => {
                let out = out_dir_srv.clone();
                let lr = live_reload_srv.clone();
                thread::spawn(move || {
                    handle_client(stream, &out, &lr);
                });
            }
            Err(e) => error!("{e}"),
        });
    });

    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&src_dir, RecursiveMode::Recursive)?;
    info!("waching {} for changes", src_dir.display());

    let ctx = WatchContext {
        config_path: config_path.to_string(),
        running: Arc::new(AtomicBool::new(false)),
        prev_hashes: Arc::new(Mutex::new(None)),
        live_reload,
    };

    let debounce_dur = Duration::from_millis(200);
    let mut debounce_start: Option<Instant> = None;

    loop {
        let remaining = match debounce_start {
            Some(t) => {
                let elapsed = t.elapsed();
                if elapsed >= debounce_dur {
                    Duration::ZERO
                } else {
                    debounce_dur - elapsed
                }
            }
            None => debounce_dur,
        };

        match rx.recv_timeout(remaining) {
            Ok(Ok(event)) => {
                let is_stuff = event
                    .paths
                    .iter()
                    .any(|p| p.extension() == Some(OsStr::new("stuff")));
                if is_stuff && !ctx.running.load(Ordering::Acquire) {
                    debounce_start = Some(Instant::now());
                }
            }
            Ok(Err(e)) => error!("Watch error: {:?}", e),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                match debounce_start {
                    Some(t) if t.elapsed() >= debounce_dur => {
                        debounce_start = None;
                        if !ctx.running.swap(true, Ordering::AcqRel) {
                            spawn_build(&ctx);
                        }
                    }
                    _ => {}
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    drop(watcher);
    server_handle.join().unwrap();
    Ok(())
}

fn spawn_build(ctx: &WatchContext) {
    let cp = ctx.config_path.clone();
    let lr = ctx.live_reload.clone();
    let r = Arc::clone(&ctx.running);
    let ph = Arc::clone(&ctx.prev_hashes);

    thread::spawn(move || {
        let _guard = BuildGuard(Arc::clone(&r));

        info!("Change detected, rebuilding...");

        let m = match Manifest::load(&cp) {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to load config: {e}");
                return;
            }
        };

        let mut builder = match Builder::new(m) {
            Ok(b) => b,
            Err(e) => {
                error!("ono.. Failed to create builder: {e}");
                return;
            }
        };

        let hashes = ph.lock_unpoisoned().take().unwrap_or_default();
        if !hashes.is_empty() {
            builder.set_prev_content_hashes(hashes);
        }
        builder.set_live_reload(true);

        match builder.build() {
            Ok(()) => {
                info!("Rebuild complete");
                *ph.lock_unpoisoned() = builder.take_content_hashes();
                lr.reload_all();
            }
            Err(e) => error!("Build failed: {e}"),
        }
    });
}
