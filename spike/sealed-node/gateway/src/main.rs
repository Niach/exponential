//! sealed-gateway: the only exit of a sealed agent VM.
//!
//! One HTTP/1.1 request per connection: the gateway parses the request head,
//! refuses anything that is not an origin-form request to an allowlisted
//! path, strips every credential the client sent, injects the subscription
//! OAuth bearer it alone holds, and streams the upstream response back
//! byte-for-byte (SSE included). The upstream is fixed per route; the client
//! can never name a host.
//!
//! Listeners: `tcp:<addr>` everywhere, `vsock:<port>` on Linux, and
//! `vsock-pool:<base>:<n>` on Hermit (one connection per port, see listen.rs).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Instant;

mod listen;

#[cfg(target_os = "hermit")]
use hermit as _;

const OAUTH_BETA: &str = "oauth-2025-04-20";

/// Upstreams the gateway may dial. Only the host is configurable per route,
/// never by the client.
struct Route {
    prefix: &'static str,
    host: &'static str,
}

/// Path allowlist (origin-form only). A headless subscription turn
/// (`claude -p`, tools included) only ever calls `/v1/messages` (and its
/// `/count_tokens` child); everything else, `/api/oauth/*` profile calls
/// included, is refused and logged.
const ROUTES: &[Route] = &[Route { prefix: "/v1/messages", host: "api.anthropic.com" }];

/// Hosts the client may put in `Host:`; anything else is a request to reach
/// a different origin through us and is refused.
const LOCAL_HOSTS: &[&str] = &["127.0.0.1", "localhost", "gateway", "api.anthropic.com"];

/// Headers the client never gets to forward.
const STRIP: &[&str] = &[
    "authorization",
    "x-api-key",
    "proxy-authorization",
    "cookie",
    "host",
    "connection",
    "keep-alive",
    "proxy-connection",
    "te",
    "upgrade",
    "transfer-encoding",
];

struct Ctx {
    token: String,
    tls: Arc<rustls::ClientConfig>,
    verbose: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let listen_spec = std::env::var("GATEWAY_LISTEN")
        .ok()
        .or_else(|| args.get(1).cloned())
        .unwrap_or_else(|| "tcp:127.0.0.1:8787".into());
    let verbose = std::env::var("GATEWAY_VERBOSE").is_ok_and(|v| v == "1");
    let token = load_token(args.get(2).map(String::as_str));
    eprintln!(
        "[gateway] boot ok: listen={listen_spec} token_len={} routes={}",
        token.len(),
        ROUTES.len()
    );

    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    #[cfg(not(target_os = "hermit"))]
    let provider = rustls::crypto::ring::default_provider();
    #[cfg(target_os = "hermit")]
    let provider = rustls_rustcrypto::provider();
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
    .with_safe_default_protocol_versions()
    .expect("tls versions")
    .with_root_certificates(roots)
    .with_no_client_auth();
    let ctx = Arc::new(Ctx { token, tls: Arc::new(tls), verbose });

    if let Some(pool) = listen_spec.strip_prefix("vsock-pool:") {
        let (base, n) = pool.split_once(':').expect("vsock-pool:<base>:<n>");
        eprintln!("[gateway] listening on {listen_spec}");
        listen::vsock_pool(base.parse().unwrap(), n.parse().unwrap(), move |conn| {
            if let Err(e) = handle(&ctx, conn) {
                eprintln!("[gateway] conn error: {e}");
            }
        });
    }
    let listener = listen::Listener::bind(&listen_spec).expect("bind listener");
    eprintln!("[gateway] listening on {listen_spec}");
    loop {
        let conn = match listener.accept() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[gateway] accept: {e}");
                continue;
            }
        };
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Err(e) = handle(&ctx, conn) {
                eprintln!("[gateway] conn error: {e}");
            }
        });
    }
}

/// The token comes from the host over a one-shot vsock provisioning channel
/// (`vsock-once:<port>`, unikernel runs), a file (host runs) or the
/// environment; it is read once and never written anywhere.
fn load_token(source: Option<&str>) -> String {
    if let Some(port) = source.and_then(|s| s.strip_prefix("vsock-once:")) {
        eprintln!("[gateway] waiting for the token on vsock port {port}");
        let t = listen::vsock_receive_line(port.parse().expect("vsock-once:<port>"))
            .expect("token provisioning over vsock");
        return t.trim().to_string();
    }
    if let Ok(path) = std::env::var("GATEWAY_TOKEN_FILE") {
        let t = std::fs::read_to_string(&path).expect("read GATEWAY_TOKEN_FILE");
        return t.trim().to_string();
    }
    let t = std::env::var("GATEWAY_TOKEN").expect("GATEWAY_TOKEN or GATEWAY_TOKEN_FILE");
    std::env::remove_var("GATEWAY_TOKEN");
    t.trim().to_string()
}

struct Head {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
}

fn read_head<R: BufRead>(r: &mut R) -> io::Result<Option<(String, Vec<(String, String)>)>> {
    let mut first = String::new();
    if r.read_line(&mut first)? == 0 {
        return Ok(None);
    }
    let mut headers = Vec::new();
    let mut total = first.len();
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line)?;
        total += n;
        if n == 0 || total > 64 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad head"));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(Some((first.trim_end().to_string(), headers)))
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn refuse<W: Write>(w: &mut W, code: u16, why: &str) -> io::Result<()> {
    eprintln!("[gateway] REFUSED {code}: {why}");
    let body = format!("{{\"type\":\"error\",\"error\":{{\"type\":\"gateway_refused\",\"message\":\"{why}\"}}}}");
    write!(
        w,
        "HTTP/1.1 {code} Refused\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )?;
    w.flush()
}

fn handle(ctx: &Ctx, conn: listen::Conn) -> io::Result<()> {
    let started = Instant::now();
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut client = conn;
    let Some((line, headers)) = read_head(&mut reader)? else { return Ok(()) };
    let mut parts = line.split_whitespace();
    let head = Head {
        method: parts.next().unwrap_or_default().to_string(),
        target: parts.next().unwrap_or_default().to_string(),
        headers,
    };

    // 0. No control characters anywhere in the head: a bare CR inside a
    //    value would otherwise be forwarded and could split headers upstream.
    let bad = |t: &str| t.chars().any(|c| c.is_control() && c != '\t');
    if bad(&head.target) || head.headers.iter().any(|(k, v)| bad(k) || bad(v)) {
        return refuse(&mut client, 400, "control characters in request head");
    }
    // 1. Origin-form only: no CONNECT tunnels, no absolute URIs naming a host.
    if head.method.eq_ignore_ascii_case("CONNECT") {
        return refuse(&mut client, 405, "CONNECT is not allowed");
    }
    if !head.target.starts_with('/') {
        return refuse(&mut client, 403, "only origin-form request targets are allowed");
    }
    // 2. Host header must name the gateway itself (or the one upstream).
    let host = header(&head.headers, "host").unwrap_or("");
    let host_name = host.rsplit_once(':').map_or(host, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host });
    if !LOCAL_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host_name)) {
        return refuse(&mut client, 403, "host is not allowlisted");
    }
    // 3. Path allowlist picks the upstream.
    let path = head.target.split('?').next().unwrap_or("");
    let Some(route) = ROUTES.iter().find(|r| path.starts_with(r.prefix)) else {
        return refuse(&mut client, 403, "path is not allowlisted");
    };
    if header(&head.headers, "transfer-encoding").is_some() {
        return refuse(&mut client, 411, "chunked request bodies are not supported");
    }
    let body_len: usize = header(&head.headers, "content-length")
        .map(|v| v.parse().unwrap_or(usize::MAX))
        .unwrap_or(0);
    if body_len > 32 * 1024 * 1024 {
        return refuse(&mut client, 413, "body too large");
    }
    let mut body = vec![0u8; body_len];
    reader.read_exact(&mut body)?;

    // 4. Rewrite: drop every client credential, inject ours.
    let mut out = format!("{} {} HTTP/1.1\r\nhost: {}\r\n", head.method, head.target, route.host);
    let mut saw_beta = false;
    for (k, v) in &head.headers {
        let lk = k.to_ascii_lowercase();
        if STRIP.contains(&lk.as_str()) {
            continue;
        }
        if lk == "anthropic-beta" {
            saw_beta = true;
            if !v.split(',').any(|b| b.trim() == OAUTH_BETA) {
                out.push_str(&format!("{k}: {v},{OAUTH_BETA}\r\n"));
                continue;
            }
        }
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    if !saw_beta {
        out.push_str(&format!("anthropic-beta: {OAUTH_BETA}\r\n"));
    }
    out.push_str(&format!("authorization: Bearer {}\r\nconnection: close\r\n\r\n", ctx.token));

    if ctx.verbose {
        log_request(&head);
    }

    // 5. Upstream TLS (rustls) to the route's fixed host.
    let tcp = TcpStream::connect((route.host, 443))?;
    tcp.set_nodelay(true).ok();
    let name = rustls::pki_types::ServerName::try_from(route.host)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
        .to_owned();
    let conn = rustls::ClientConnection::new(ctx.tls.clone(), name)
        .map_err(|e| io::Error::other(e))?;
    let mut tls = rustls::StreamOwned::new(conn, tcp);
    tls.write_all(out.as_bytes())?;
    drop(out);
    tls.write_all(&body)?;
    tls.flush()?;

    // 6. Response head: force connection: close so the client never reuses
    //    a socket we are about to drop; then stream the body unchanged.
    let mut up = BufReader::new(tls);
    let Some((status, rheaders)) = read_head(&mut up)? else {
        return refuse(&mut client, 502, "upstream closed");
    };
    let mut rh = format!("{status}\r\n");
    for (k, v) in &rheaders {
        if k.eq_ignore_ascii_case("connection") || k.eq_ignore_ascii_case("keep-alive") {
            continue;
        }
        rh.push_str(&format!("{k}: {v}\r\n"));
    }
    rh.push_str("connection: close\r\n\r\n");
    client.write_all(rh.as_bytes())?;
    client.flush()?;
    let mut bytes = 0u64;
    let mut first_byte_ms = None;
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = match up.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            // Servers often skip close_notify on `connection: close`.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        first_byte_ms.get_or_insert(started.elapsed().as_millis());
        bytes += n as u64;
        client.write_all(&buf[..n])?;
        client.flush()?;
    }
    eprintln!(
        "[gateway] {} {} -> {} | {} | body_in={} body_out={} ttfb_ms={} total_ms={}",
        head.method,
        path,
        route.host,
        status,
        body_len,
        bytes,
        first_byte_ms.unwrap_or(0),
        started.elapsed().as_millis()
    );
    Ok(())
}

/// Q1 evidence: every request header as the client sent it, credential
/// values redacted to their shape.
fn log_request(head: &Head) {
    eprintln!("[gateway] >>> {} {}", head.method, head.target);
    for (k, v) in &head.headers {
        let lk = k.to_ascii_lowercase();
        let shown = if matches!(lk.as_str(), "authorization" | "x-api-key" | "cookie" | "proxy-authorization") {
            format!("<redacted len={} prefix={:?}>", v.len(), v.chars().take(7).collect::<String>())
        } else {
            v.clone()
        };
        eprintln!("[gateway]     {k}: {shown}");
    }
}
