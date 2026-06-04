use std::io::Write;
use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};


pub const RELOAD_SCRIPT: &str = r#"
<script>
(new EventSource("/__reload")).addEventListener("reload",function(){location.reload()});
</script>"#;

/// Extension trait for `Mutex` that ignores poisoning.
///
/// The logical thing would have been using `parking_lot::Mutex` but that 
/// would require another dependency. 
pub trait LockExt<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct LiveReload {
    clients: Mutex<Vec<mpsc::Sender<String>>>,
}

impl LiveReload {
    pub fn new() -> Arc<Self> {
        Arc::new(LiveReload {
            clients: Mutex::new(Vec::new()),
        })
    }

    fn with_clients<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Vec<mpsc::Sender<String>>) -> R,
    {
        let mut clients = self.clients.lock_unpoisoned();
        f(&mut clients)
    }

    pub fn subscribe(&self) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel();
        self.with_clients(|clients| clients.push(tx));
        rx
    }

    pub fn reload_all(&self) {
        self.with_clients(|clients| {
            clients.retain(|tx| tx.send("reload".to_string()).is_ok());
        });
    }

    /// Returns true if the request was consumed.
    pub fn handle_request(&self, path: &str, stream: &mut TcpStream) -> bool {
        if path != "/__reload" {
            return false;
        }
        self.handle_connection(stream);
        true
    }

    pub fn handle_connection(&self, stream: &mut TcpStream) {
        const SSE_HEADER: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";

        let _ = stream.write_all(SSE_HEADER);
        let rx = self.subscribe();
        let mut last_heartbeat = Instant::now();

        loop {
            if last_heartbeat.elapsed() >= Duration::from_secs(15) {
                if stream.write_all(b": heartbeat\r\n\r\n").is_err() { break; }
                let _ = stream.flush();
                last_heartbeat = Instant::now();
            }

            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(_) => {
                    let msg = "event: reload\r\ndata: {}\r\n\r\n";
                    if stream.write_all(msg.as_bytes()).is_err() { break; }
                    let _ = stream.flush();
                    last_heartbeat = Instant::now();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}


