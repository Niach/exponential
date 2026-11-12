//! #LOGS-2: the whole path, end to end: a cached-off config, the
//! `EXP_SENTRY_DSN` override, an error → `POST <server>/api/monitoring`
//! with the DSN in the envelope header, scrubbed, the user id only.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

fn http() -> reqwest::blocking::Client {
    exp_http::blocking().build().unwrap()
}

#[test]
fn an_error_goes_through_the_tunnel_scrubbed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let (posts, received) = mpsc::channel::<(String, String)>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = stream;
            // The config endpoint: this server ships nothing.
            let answer = if request_line.starts_with("GET") { r#"{"enabled":false}"# } else { "{}" };
            let _ = write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}", answer.len());
            let _ = posts.send((request_line, String::from_utf8_lossy(&body).into_owned()));
        }
    });

    let data_dir = std::env::temp_dir().join(format!("obs-tunnel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    // #ERR-12: a native crash the last launch (another release) left behind.
    let crashes = data_dir.join("crashes");
    std::fs::create_dir_all(&crashes).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(
        crashes.join("6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11.json"),
        format!(
            r#"{{"id":"6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11","release":"exponential-desktop@1.2.2","component":"desktop","timestamp":{now},"os":"Linux","os_version":null,"arch":"x86_64","kind":"SIGSEGV","reason":"SIGSEGV / SEGV_MAPERR","address":0,"thread_id":7,"thread_name":null,"frames":[{{"address":4096,"module":0,"trust":"context"}}],"modules":[{{"code_file":"/usr/bin/exp-desktop","base":0,"size":65536,"debug_id":"b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf","code_id":null,"debug_file":null,"main":true}}]}}"#
        ),
    )
    .unwrap();
    // SAFETY: set before any thread of this test binary reads it.
    unsafe { std::env::set_var("EXP_SENTRY_DSN", "https://publickey@glitchtip.example/9") };
    observability::init(observability::Setup {
        product: observability::Product::Desktop,
        version: "1.2.3".into(),
        data_dir: data_dir.clone(),
        http,
        native_crashes: false,
    });
    observability::connect(&server, Some("user-42"));
    sentry::Hub::main().capture_message("failed with Authorization: Bearer s3cr3t", sentry::Level::Error);
    observability::flush();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut envelope = None;
    let mut crash = None;
    while std::time::Instant::now() < deadline && (envelope.is_none() || crash.is_none()) {
        let Ok((line, body)) = received.recv_timeout(Duration::from_secs(1)) else { continue };
        if line.starts_with("POST /api/monitoring ") {
            if body.contains("SIGSEGV") {
                crash = Some(body);
            } else if body.contains("Authorization") {
                envelope = Some(body);
            }
        }
    }
    let crash = crash.expect("the pending native crash reached the tunnel");
    assert!(crash.contains("exponential-desktop@1.2.2") && crash.contains(r#""level":"fatal""#), "{crash}");
    assert!(crash.contains(r#""instruction_addr":"0x1000""#) && crash.contains("b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf"), "{crash}");
    // Deleted once the client flushed it.
    let gone = std::time::Instant::now() + Duration::from_secs(10);
    while std::fs::read_dir(&crashes).unwrap().next().is_some() && std::time::Instant::now() < gone {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(std::fs::read_dir(&crashes).unwrap().next().is_none(), "sent once, then deleted");
    let envelope = envelope.expect("an envelope reached the tunnel");
    assert!(envelope.lines().next().unwrap().contains("glitchtip.example/9"), "{envelope}");
    assert!(envelope.contains("Bearer [Filtered]") && !envelope.contains("s3cr3t"), "{envelope}");
    assert!(envelope.contains(r#""id":"user-42""#), "{envelope}");
    assert!(envelope.contains("exponential-desktop@1.2.3"), "{envelope}");
    assert!(!envelope.contains("server_name"), "{envelope}");
    let _ = std::fs::remove_dir_all(&data_dir);
}
