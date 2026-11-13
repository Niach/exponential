//! The HTTP half shared by the `github` and `exponential` providers: the
//! workspace's blocking reqwest on rustls with the ring provider (installed
//! here when the host has not: ecode links no Exponential crate, so not
//! exp-http either), JSON in and out, every error redacted.

use std::time::Duration;

use serde_json::Value;

use crate::{Error, Result};

/// A client for one provider (`user_agent` names the caller to the host).
pub(crate) fn client(user_agent: &str) -> Result<reqwest::blocking::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Http { status: 0, detail: e.to_string() })
}

/// A client for object transfers (#EXP-1616: a pack to or from the bucket
/// over a presigned URL): the same stack, a timeout for a big repo on a
/// slow link.
#[cfg(all(feature = "exponential", feature = "repo"))]
pub(crate) fn transfer_client(user_agent: &str) -> Result<reqwest::blocking::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(60 * 60))
        .build()
        .map_err(|e| Error::Http { status: 0, detail: e.to_string() })
}

/// A JSON body (the workspace's reqwest has no `json` feature).
pub(crate) fn with_json(request: reqwest::blocking::RequestBuilder, body: &Value) -> reqwest::blocking::RequestBuilder {
    request.header("content-type", "application/json").body(body.to_string())
}

/// A response: its status and its JSON body (`Null` when empty or not JSON).
pub(crate) struct Response {
    pub status: u16,
    pub body: Value,
    /// Read by the `github` provider's polling only.
    #[cfg_attr(not(feature = "github"), allow(dead_code))]
    pub etag: Option<String>,
}

/// Sends a request built by `build` and reads the JSON answer. Network
/// errors become `Http { status: 0 }`; `secrets` are redacted from them.
pub(crate) fn send(request: reqwest::blocking::RequestBuilder, secrets: &[&str]) -> Result<Response> {
    let response = request.send().map_err(|e| Error::Http { status: 0, detail: ecode_core::redact(&e.to_string(), secrets) })?;
    let status = response.status().as_u16();
    let etag = response.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_string);
    let text = response.text().unwrap_or_default();
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok(Response { status, body, etag })
}

impl Response {
    /// The provider's message, for errors (redacted by the caller's secrets).
    pub fn message(&self) -> String {
        self.body.get("message").and_then(Value::as_str).map_or_else(|| self.body.to_string(), str::to_string)
    }

    /// `Err(Http)` unless 2xx.
    pub fn ok(self, secrets: &[&str]) -> Result<Value> {
        if (200..300).contains(&self.status) {
            Ok(self.body)
        } else {
            Err(Error::Http { status: self.status, detail: ecode_core::redact(&self.message(), secrets) })
        }
    }
}

/// A one-thread HTTP/1.1 server answering from a handler (the providers' tests).
#[cfg(test)]
pub(crate) mod mock {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use serde_json::Value;

    type Handler = dyn Fn(&str, &str, &Value) -> (u16, Value) + Send + Sync;

    pub struct MockServer {
        port: u16,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl MockServer {
        /// `handler(method, path with query, JSON body)` → `(status, body)`.
        pub fn start(handler: impl Fn(&str, &str, &Value) -> (u16, Value) + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let requests = Arc::new(Mutex::new(vec![]));
            let log = requests.clone();
            let handler: Box<Handler> = Box::new(handler);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut head = String::new();
                    let mut length = 0;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':') {
                            if name.eq_ignore_ascii_case("content-length") {
                                length = value.trim().parse().unwrap_or(0);
                            }
                        }
                        head.push_str(&line);
                    }
                    let mut body = vec![0; length];
                    let _ = reader.read_exact(&mut body);
                    let body_json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let mut parts = head.split_whitespace();
                    let (method, path) = (parts.next().unwrap_or_default().to_string(), parts.next().unwrap_or_default().to_string());
                    log.lock().unwrap().push(format!("{head}\n{}", String::from_utf8_lossy(&body)));
                    let (status, answer) = handler(&method, &path, &body_json);
                    let text = if answer.is_null() { String::new() } else { answer.to_string() };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                        text.len()
                    );
                    let _ = stream.flush();
                }
            });
            Self { port, requests }
        }

        pub fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        /// Every request so far: its head, a blank line, its body.
        pub fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }
}
