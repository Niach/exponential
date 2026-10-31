//! NEW-58 live check: the real capturer + store + endpoint, end to end.
//! `cargo run -p epilot-screen --example live -- <data dir> [seconds]`: captures for
//! a few seconds, then calls `get_screenshots` over the loopback endpoint as
//! the voice agent's run would, prints the text lines and saves the images.

use std::io::{Read as _, Write as _};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("data dir"));
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    epilot_screen::set_status_observer(|status| println!("status: {status:?}"));
    epilot_screen::start(dir.clone()).expect("start");
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    let grant = epilot_screen::grant("live-run").expect("grant");
    println!("query at {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "get_screenshots", "arguments": {} } }).to_string();
    let url = grant.url.trim_start_matches("http://");
    let (host, path) = url.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((url, "/".into()));
    let mut stream = std::net::TcpStream::connect(host).expect("connect");
    write!(stream, "POST {path} HTTP/1.0\r\nHost: {host}\r\nAuthorization: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", emcp::ecore::bearer_header(&grant.token), body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let json = response.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    let value: serde_json::Value = serde_json::from_str(json).expect("json");
    let mut images = 0;
    for item in value["result"]["content"].as_array().cloned().unwrap_or_default() {
        match item["type"].as_str() {
            Some("text") => println!("text: {}", item["text"]),
            Some("image") => {
                use base64_decode::decode;
                let data = item["data"].as_str().unwrap();
                let bytes = decode(data);
                let path = dir.join(format!("shot-{images}.webp"));
                std::fs::write(&path, &bytes).unwrap();
                let webp = &bytes[..bytes.len().min(16)];
                let riff = webp.starts_with(b"RIFF") && webp.get(8..12) == Some(b"WEBP");
                println!("image {images}: {} {} ({} base64 chars, RIFF/WEBP {riff})", path.display(), item["mimeType"], data.len());
                images += 1;
            }
            _ => {}
        }
    }
    println!("frames on disk: {}", walk(&dir.join("screen")));
    epilot_screen::stop(false);
}

fn walk(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir).map(|entries| entries.flatten().map(|e| if e.path().is_dir() { walk(&e.path()) } else { 1 }).sum()).unwrap_or(0)
}

mod base64_decode {
    pub fn decode(text: &str) -> Vec<u8> {
        let table = |c: u8| match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            _ => 63,
        };
        let clean: Vec<u8> = text.bytes().filter(|c| *c != b'=' && !c.is_ascii_whitespace()).collect();
        let mut out = Vec::with_capacity(clean.len() * 3 / 4);
        for chunk in clean.chunks(4) {
            let v: Vec<u32> = chunk.iter().map(|c| u32::from(table(*c))).collect();
            let n = v.iter().enumerate().fold(0u32, |acc, (i, x)| acc | (x << (18 - 6 * i)));
            out.push((n >> 16) as u8);
            if v.len() > 2 { out.push((n >> 8) as u8); }
            if v.len() > 3 { out.push(n as u8); }
        }
        out
    }
}
