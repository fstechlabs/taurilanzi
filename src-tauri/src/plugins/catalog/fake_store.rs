//! A store on a local port for the tests: it serves the files it is given and counts requests.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>"#;

pub struct FakeStore {
    pub base: String,
    hits: Arc<AtomicUsize>,
}

impl FakeStore {
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// Serves the files `files` makes from the store's address: by the whole path asked for, query
/// included, or else by the path alone.
pub fn serve(files: impl FnOnce(&str) -> HashMap<String, Vec<u8>>) -> FakeStore {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = files(&base);
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            counter.fetch_add(1, Ordering::SeqCst);
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let target = line.split(' ').nth(1).unwrap_or_default().to_owned();
            let mut length = 0;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let path = target.split('?').next().unwrap_or_default();
            let (status, body) = match files.get(&target).or_else(|| files.get(path)) {
                Some(body) => ("200 OK", body.as_slice()),
                None => ("404 Not Found", &[][..]),
            };
            let mut stream = &stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    FakeStore { base, hits }
}
