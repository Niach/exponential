//! #ERR-29: a panic reaches the tunnel whatever thread it happens on. A
//! thread's own sentry hub is a snapshot of the main hub, taken when the
//! thread first touches sentry; after a client rebuild (the "Send
//! diagnostics" switch, a new server config) that snapshot holds the closed
//! client, so the panics of every thread that touched sentry before it went
//! nowhere.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn http() -> reqwest::blocking::Client {
    exp_http::blocking().build().unwrap()
}

/// A tunnel that answers every POST 200 and hands its body over; the
/// config endpoint answers nothing usable (the DSN override stays).
fn tunnel() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let (posts, received) = mpsc::channel::<String>();
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
            let code = if request_line.starts_with("GET") { 404 } else { 200 };
            let mut stream = stream;
            let _ = write!(stream, "HTTP/1.1 {code} X\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}");
            if request_line.starts_with("POST") {
                let _ = posts.send(String::from_utf8_lossy(&body).into_owned());
            }
        }
    });
    (server, received)
}

fn posted(received: &mpsc::Receiver<String>, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(body) = received.recv_timeout(Duration::from_millis(200)) {
            if body.contains(needle) {
                return true;
            }
        }
    }
    false
}

#[test]
fn a_panic_reaches_the_tunnel_from_a_thread_older_than_the_client() {
    let (server, received) = tunnel();
    let data_dir = std::env::temp_dir().join(format!("obs-thread-hubs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    // SAFETY: set before any thread of this test binary reads it.
    unsafe { std::env::set_var("EXP_SENTRY_DSN", "https://publickey@glitchtip.example/9") };
    observability::init(observability::Setup {
        product: observability::Product::Desktop,
        version: "1.2.3".into(),
        data_dir: data_dir.clone(),
        http,
        native_crashes: false,
    });
    observability::connect(&server, None);

    // A worker that touched sentry under the first client (a tracing
    // event, a caught panic, `Hub::current`), then waits.
    let (go, wait) = mpsc::channel::<&'static str>();
    let (touched, ready) = mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let _ = sentry::Hub::current();
        touched.send(()).unwrap();
        let message = wait.recv().unwrap();
        panic!("{message}");
    });
    ready.recv().unwrap();

    // A fresh thread under the first client: reported (it was before too).
    let _ = std::thread::spawn(|| panic!("a fresh thread panicked")).join();
    assert!(posted(&received, "a fresh thread panicked"), "a fresh thread's panic reaches the tunnel");

    // The switch goes off and on again: the client is rebuilt.
    observability::set_diagnostics_enabled(false);
    observability::set_diagnostics_enabled(true);
    go.send("an older thread panicked").unwrap();
    let _ = worker.join();
    assert!(posted(&received, "an older thread panicked"), "the older thread's panic reaches the rebuilt client");
    let _ = std::fs::remove_dir_all(&data_dir);
}
