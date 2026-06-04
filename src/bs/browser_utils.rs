use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::{Path, PathBuf},
};

use tracing::{debug, warn};

use crate::bs::live::LiveReload;

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(c) = chars.next() {
        match c {
            b'%' => {
                let hi = chars.next().and_then(hex_val).unwrap_or(0);
                let lo = chars.next().and_then(hex_val).unwrap_or(0);
                result.push((hi << 4 | lo) as char);
            }
            b'+' => result.push(' '),
            _ => result.push(c as char),
        }
    }
    result
}

fn sanitize_path(path: &str) -> Option<String> {
    if path.split('/').any(|seg| seg == "..") {
        return None;
    }
    Some(path.to_string())
}

fn mime_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn status_line(code: u16) -> String {
    match code {
        200 => "200 OK".to_string(),
        304 => "304 Not Modified".to_string(),
        404 => "404 Not Found".to_string(),
        500 => "500 Internal Server Error".to_string(),
        418 => "I'm a teapot".to_string(),
        _ => format!("soooo, {} HTTP code was returned :p", code),
    }
}

fn http_response(status: u16, body: &[u8], content_type: &str) -> Vec<u8> {
    let headers = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_line(status),
        content_type,
        body.len()
    );
    let mut resp = headers.into_bytes();
    resp.extend_from_slice(body);
    resp
}

fn serve_static_file(file_path: &PathBuf, stream: &mut TcpStream) {
    match fs::read(file_path) {
        Ok(data) => {
            let mime = mime_type(file_path.to_str().unwrap_or(""));
            let response = http_response(200, &data, mime);
            debug!("{:?}", data);
            let _ = stream.write_all(&response);
        }
        Err(e) => {
            warn!(path = %file_path.display(), error = %e, "Failed to serve static file");
            let _ = stream.write_all(&http_response(404, b"<h1>404 Not Found</h1>", "text/html"));
        }
    }
}

pub fn handle_client(mut stream: TcpStream, out_dir: &Path, live_r: &LiveReload) {
    let cloned = match stream.try_clone() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("<h1>500 Internal Server Error</h1><p>connection error : {e}</p>");
            let _ = stream.write_all(&http_response(500, msg.as_bytes(), "text/html"));
            return;
        }
    };
    let mut reader = BufReader::new(cloned);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "GET" {
        let _ = stream.write_all(&http_response(404, b"<h1>404 Not Found</h1>", "text/html"));
        return;
    }

    let raw_path = url_decode(parts[1]);
    let path = raw_path.split('?').next().unwrap_or(&raw_path);

    if live_r.handle_request(path, &mut stream) {
        return;
    }

    let file_path = if path == "/" {
        out_dir.join("index.html")
    } else {
        let clean = path.strip_prefix('/').unwrap_or(path);
        let clean = match sanitize_path(clean) {
            Some(p) => p,
            None => {
                let _ =
                    stream.write_all(&http_response(404, b"<h1>404 Not Found</h1>", "text/html"));
                return;
            }
        };
        out_dir.join(clean)
    };
    serve_static_file(&file_path, &mut stream);
}
