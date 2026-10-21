//! Transport JSON-RPC du LSP : en-têtes `Content-Length` puis corps JSON.

use std::io::{self, BufRead, Write};
use std::sync::Mutex;

use serde_json::Value;

/// Lit un message ; `Ok(None)` en fin de flux.
pub fn read(r: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut len = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(v) = header.strip_prefix("Content-Length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let len = len.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Content-Length absent"))?;
    let mut body = vec![0; len];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub struct Writer(Mutex<Box<dyn Write + Send>>);

impl Writer {
    pub fn new(w: impl Write + Send + 'static) -> Self {
        Writer(Mutex::new(Box::new(w)))
    }

    pub fn send(&self, msg: &Value) {
        let body = msg.to_string();
        let mut w = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = write!(w, "Content-Length: {}\r\n\r\n{body}", body.len());
        let _ = w.flush();
    }
}
