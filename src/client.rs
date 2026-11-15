//! How the `carbon` command talks to a running hub: plain HTTP to port 3000 on this machine, nothing more.
//! Small enough to do by hand, which keeps the command quick to start and the binary small.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use serde_json::Value;

use crate::engine::HUB_PORT;

pub const NOT_RUNNING: &str =
    "Carbon isn't running. Start it with `carbon serve`, or open the Carbon app.";

fn request(method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut stream =
        TcpStream::connect(("127.0.0.1", HUB_PORT)).map_err(|_| NOT_RUNNING.to_string())?;
    // Starting or stopping a server can take a few seconds while the port is cleared
    stream.set_read_timeout(Some(Duration::from_secs(60))).ok();
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost:{HUB_PORT}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    stream
        .read_to_string(&mut reply)
        .map_err(|e| e.to_string())?;
    let (head, text) = reply
        .split_once("\r\n\r\n")
        .ok_or("Carbon sent something that isn't an answer")?;
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    if status >= 400 {
        return Err(value["error"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("Carbon answered {status}: {text}")));
    }
    Ok(value)
}

pub fn get(path: &str) -> Result<Value, String> {
    request("GET", path, None)
}

pub fn post(path: &str, body: Value) -> Result<Value, String> {
    request("POST", path, Some(&body))
}

/// True when a hub is already answering on port 3000
pub fn running() -> bool {
    get("/api/state").is_ok()
}
