//! #ERR-18: a panic's event survives a tunnel that does not take it. The
//! panic hook sends it in-process; it also waits on disk
//! (`crashes/<id>.panic`) until the tunnel answered 2xx for it, and the next
//! client sends what is left.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

fn http() -> reqwest::blocking::Client {
    exp_http::blocking().build().unwrap()
}

fn kept_panics(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "panic"))
                .map(|path| std::fs::read_to_string(path).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

fn panic_on_a_thread(message: &'static str) {
    let _ = std::thread::spawn(move || panic!("{message}")).join();
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_panic_the_tunnel_refused_is_sent_by_the_next_client() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // What the tunnel answers a POST with.
    let status = Arc::new(AtomicU16::new(200));
    let (posts, received) = mpsc::channel::<(u16, String)>();
    {
        let status = status.clone();
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
                // The config endpoint answers nothing usable: the cached
                // config (and the DSN override) stay.
                let code = if request_line.starts_with("GET") { 404 } else { status.load(Ordering::SeqCst) };
                let _ = write!(stream, "HTTP/1.1 {code} X\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}");
                if request_line.starts_with("POST") {
                    let _ = posts.send((code, String::from_utf8_lossy(&body).into_owned()));
                }
            }
        });
    }
    let posted = |needle: &str, code: u16| {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Ok((answered, body)) = received.recv_timeout(Duration::from_millis(200)) {
                if answered == code && body.contains(needle) {
                    return body;
                }
            }
        }
        panic!("no POST answered {code} carrying {needle:?}");
    };

    let data_dir = std::env::temp_dir().join(format!("obs-panic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let crashes = data_dir.join("crashes");
    // SAFETY: set before any thread of this test binary reads it.
    unsafe { std::env::set_var("EXP_SENTRY_DSN", "https://publickey@glitchtip.example/9") };
    observability::init(observability::Setup {
        product: observability::Product::Desktop,
        version: "1.2.3".into(),
        data_dir: data_dir.clone(),
        http,
        native_crashes: false,
    });
    observability::connect(&format!("http://127.0.0.1:{port}"), None);

    // The tunnel takes it: sent, nothing left on disk.
    panic_on_a_thread("taken right away");
    let body = posted("taken right away", 200);
    assert!(body.contains(r#""level":"fatal""#) && body.contains("exponential-desktop@1.2.3"), "{body}");
    assert!(kept_panics(&crashes).is_empty(), "a delivered panic is not kept");

    // The tunnel answers 503 (the 2026-11-12 crash): the event stays.
    status.store(503, Ordering::SeqCst);
    panic_on_a_thread("Encoding Error: Utf8Error");
    posted("Encoding Error", 503);
    let kept = kept_panics(&crashes);
    assert_eq!(kept.len(), 1, "the refused panic waits for the next launch");
    assert!(kept[0].contains("Encoding Error") && kept[0].contains("exponential-desktop@1.2.3"), "{}", kept[0]);

    // The next client (here: another server name for the same tunnel)
    // sends it with its own event id and deletes it once taken.
    status.store(200, Ordering::SeqCst);
    let event_id = serde_json::from_str::<serde_json::Value>(&kept[0]).unwrap()["event_id"].as_str().unwrap().to_string();
    observability::connect(&format!("http://localhost:{port}"), None);
    let body = posted("Encoding Error", 200);
    assert!(body.contains(&event_id.replace('-', "")) || body.contains(&event_id), "same event id: {body}");
    wait_for("the sent panic is deleted", || kept_panics(&crashes).is_empty());
    let _ = std::fs::remove_dir_all(&data_dir);
}
