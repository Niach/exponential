//! Manual probe of the computer-use backend, the way a host runs it:
//! `cargo run -p computer --example serve -- [TOOL [JSON_ARGS]]...` calls
//! each tool through a granted run and prints the result (images elided;
//! `EXP_SHOT=/path.png` also writes the last screenshot there), then serves
//! the loopback MCP endpoint until Ctrl-C, printing its URL and token for a
//! hand-written MCP client. The lock engages on the first call and the
//! person's Esc aborts, as in a run: watch the log for both.
//!
//! The test matrix (EXP-1273) on a machine, in this order:
//! `screenshot`, `list_windows`, `left_click '{"x":..,"y":..}'` into a
//! text field, `type '{"text":"hello"}'`, `key '{"combo":"ctrl+a"}'`,
//! `scroll`, `drag`; then one long `wait '{"seconds":10}'` while pressing
//! keys and moving the mouse (nothing lands), then Esc (the wait answers
//! the abort and the input is yours again).

use serde_json::{json, Value};

fn main() {
    env_logger_lite();
    println!("readiness: {:?}", computer::readiness());
    println!("prepare: {:?}", computer::prepare());
    computer::set_presence_observer(|label| match label {
        Some(label) => println!("[presence] Computer use in progress · {label} · Esc to abort"),
        None => println!("[presence] released"),
    });
    let grant = computer::grant("probe", "Probe").expect("grant");
    println!("url: {}\ntoken: {}", grant.url, grant.token);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut args = args.into_iter().peekable();
    while let Some(tool) = args.next() {
        let arguments = match args.peek() {
            Some(next) if next.starts_with('{') => args.next().unwrap(),
            _ => "{}".to_string(),
        };
        let started = std::time::Instant::now();
        let reply = post(&grant, &tool, &arguments);
        println!("--- {tool} {arguments} ({} ms)\n{}", started.elapsed().as_millis(), elide(reply));
    }
    println!("serving; Ctrl-C ends it");
    loop {
        std::thread::park();
    }
}

/// `RUST_LOG`-free: everything the crate logs goes to stderr.
fn env_logger_lite() {
    struct Stderr;
    impl log::Log for Stderr {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
        fn flush(&self) {}
    }
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Debug));
}

fn post(grant: &computer::Grant, tool: &str, arguments: &str) -> Value {
    use std::io::{Read, Write};
    let addr = grant.url.trim_start_matches("http://").trim_end_matches("/mcp");
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": serde_json::from_str::<Value>(arguments).expect("JSON args") } }).to_string();
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        grant.token,
        body.len()
    )
    .unwrap();
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).unwrap();
    let split = reply.windows(4).position(|w| w == b"\r\n\r\n").expect("headers") + 4;
    let (head, body) = reply.split_at(split);
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();
    let body = if head.contains("transfer-encoding: chunked") { dechunk(body) } else { body.to_vec() };
    serde_json::from_slice(&body).unwrap()
}

/// A large reply comes chunked (tiny_http's choice); join the chunks.
fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n").unwrap();
        let size = usize::from_str_radix(std::str::from_utf8(&body[..line_end]).unwrap().trim(), 16).unwrap();
        body = &body[line_end + 2..];
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

fn elide(mut value: Value) -> String {
    if let Some(content) = value.pointer_mut("/result/content").and_then(Value::as_array_mut) {
        for item in content {
            if item["type"] == "image" {
                let data = item["data"].as_str().unwrap_or_default();
                if let Ok(path) = std::env::var("EXP_SHOT") {
                    if let Some(bytes) = decode(data) {
                        let _ = std::fs::write(&path, bytes);
                        eprintln!("wrote {path}");
                    }
                }
                item["data"] = json!(format!("<{} base64 chars>", data.len()));
            }
        }
    }
    serde_json::to_string_pretty(&value).unwrap()
}

fn decode(data: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for byte in data.bytes() {
        if byte == b'=' {
            break;
        }
        let value = TABLE.iter().position(|&t| t == byte)? as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}
