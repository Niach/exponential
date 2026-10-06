//! Manual probe of the linked cua driver, the way a host runs it:
//! `cargo run -p computer --features cua --example serve -- [TOOL [JSON_ARGS]]...`
//! calls each tool through a granted session and prints the result (images
//! elided), then serves the loopback MCP endpoint until Ctrl-C, printing its
//! URL and token for a hand-written MCP client. This same binary is what the
//! host re-runs as the private worker, so the first line of `main` is the
//! worker check every host carries.

use serde_json::{json, Value};

fn main() {
    if let Some(generation) = computer::worker::requested_generation() {
        computer::worker::run_and_exit(generation);
    }
    println!("readiness: {:?}", computer::readiness());
    println!("prepare: {:?}", computer::prepare());
    let grant = computer::grant("probe", "Probe").expect("grant");
    println!("url: {}\ntoken: {}", grant.url, grant.token);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut args = args.into_iter().peekable();
    while let Some(tool) = args.next() {
        let arguments = match args.peek() {
            Some(next) if next.starts_with('{') => args.next().unwrap(),
            _ => "{}".to_string(),
        };
        let reply = post(&grant, &tool, &arguments);
        println!("--- {tool} {arguments}\n{}", elide(reply));
    }
    println!("serving; Ctrl-C ends it");
    loop {
        std::thread::park();
    }
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
                let bytes = item["data"].as_str().map_or(0, str::len);
                item["data"] = json!(format!("<{bytes} base64 chars>"));
            }
        }
    }
    serde_json::to_string_pretty(&value).unwrap()
}
