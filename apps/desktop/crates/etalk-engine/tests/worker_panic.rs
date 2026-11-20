//! #ERR-29: a panic that ends one of the speech worker's engine threads ends
//! the worker, and its host hears where it panicked and reports a crash by
//! panic; what the panic SAID never reaches a log line a report could
//! carry. This binary runs itself as the worker (`etalk::requested`), whose
//! engine thread panics.

use std::sync::Mutex;
use std::time::Duration;

fn main() {
    if etalk::requested() {
        etalk_engine::report_panics();
        let engine = std::thread::Builder::new()
            .name("voice-engine".into())
            .spawn(|| {
                let _ends = etalk_engine::EndsWorker;
                panic!("could not speak: Lunch at 12?");
            })
            .unwrap();
        let _ = engine.join();
        // Never reached: the guard ended the worker.
        std::process::exit(3);
    }
    if !cfg!(unix) {
        return;
    }
    etalk::log_capture::start();
    let dir = std::env::temp_dir().join(format!("etalk-worker-panic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (tx, events) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    let client = etalk::SpeechClient::new(std::env::current_exe().unwrap(), dir.clone(), move |event| {
        let _ = tx.lock().unwrap().send(event);
    });
    // A request brings the worker up.
    client.speak("m1", "Hello.");
    let event = events.recv_timeout(Duration::from_secs(60)).expect("the worker's end reaches the host");
    assert_eq!(event, etalk::Event::Exited { code: Some(etalk::PANIC_EXIT), signal: None }, "ended by the panic, its line not forwarded");
    // The stderr reader may still be logging the worker's last lines.
    let said = |records: &[(log::Level, String)]| records.iter().any(|(level, text)| *level == log::Level::Debug && text.contains("Lunch at 12?"));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !said(&etalk::log_capture::records()) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let records = etalk::log_capture::records();
    assert!(etalk::log_capture::error_logged_anywhere("talk: the speech worker crashed (panic)"), "{records:?}");
    assert!(
        records.iter().any(|(level, text)| *level == log::Level::Warn && text.contains("worker_panic.rs:") && text.ends_with("on thread voice-engine")),
        "where it panicked: {records:?}"
    );
    // What it said: a debug line (never a breadcrumb), nothing above.
    assert!(said(&records), "{records:?}");
    assert!(!records.iter().any(|(level, text)| *level <= log::Level::Info && text.contains("Lunch")), "{records:?}");
    client.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    println!("worker_panic: ok");
}
