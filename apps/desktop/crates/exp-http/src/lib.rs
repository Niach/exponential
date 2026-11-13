//! THE way a crate of this workspace builds a reqwest client.
//!
//! reqwest 0.13 on `rustls-no-provider` builds its TLS from the process's
//! default rustls crypto provider and PANICS at `build()` when none is
//! installed. The only panic-free default it has, the `rustls` feature,
//! bundles aws-lc-rs: a C build, and a second provider next to ring makes
//! rustls refuse to pick one for every other rustls user (steer's
//! tokio-tungstenite, iroh). So every client starts here, which installs ring
//! first, and the test below fails the build for a client built any other
//! way.
//!
//! The SDK crates stay free of this crate and keep one builder of their own,
//! allowlisted below: eui-gpui (`transport::http_client`) and emcp
//! (`runtime::http_client`).

/// The process's crypto provider: ring, unless the host installed one already.
pub fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// An async client builder, the crypto provider installed.
pub fn client() -> reqwest::ClientBuilder {
    install_crypto();
    reqwest::Client::builder()
}

/// A blocking client builder, the crypto provider installed.
pub fn blocking() -> reqwest::blocking::ClientBuilder {
    install_crypto();
    reqwest::blocking::Client::builder()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// The files allowed to build a reqwest client directly: this crate and
    /// the SDK crates' own builders.
    const ALLOWED: &[&str] = &["exp-http/src/lib.rs", "eui-gpui/src/transport.rs", "emcp/src/runtime.rs", "ecode/src/http.rs"];

    const CONSTRUCTORS: &[&str] = &[
        "Client::builder(",
        "Client::new(",
        "Client::default(",
        "ClientBuilder::new(",
        "ClientBuilder::default(",
    ];

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// The reqwest client constructions in `text`, as `line: source`.
    fn constructions(text: &str) -> Vec<String> {
        // A bare `Client::new()` is reqwest's only when the file imports it.
        let imports_client = text.lines().any(|l| l.trim_start().starts_with("use reqwest") && l.contains("Client"));
        let mut found = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            let get = code.contains("reqwest::get(") || code.contains("reqwest::blocking::get(");
            let built = CONSTRUCTORS.iter().any(|pattern| {
                code.match_indices(pattern).any(|(at, _)| {
                    let before = code[..at].chars().next_back();
                    let qualified = code[..at].ends_with("reqwest::") || code[..at].ends_with("blocking::");
                    let bare = !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':');
                    qualified || (bare && imports_client)
                })
            });
            if get || built {
                found.push(format!("{}: {}", n + 1, code));
            }
        }
        found
    }

    #[test]
    fn every_reqwest_client_is_built_through_exp_http() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the crates directory").to_path_buf();
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&crates).expect("a readable crates directory").flatten() {
            for sub in ["src", "tests", "benches", "examples"] {
                rust_files(&entry.path().join(sub), &mut files);
            }
        }
        let mut offenders = Vec::new();
        for file in files {
            let rel = file.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            if ALLOWED.contains(&rel.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&file).expect("a readable source file");
            offenders.extend(constructions(&text).into_iter().map(|hit| format!("{rel}:{hit}")));
        }
        assert!(
            offenders.is_empty(),
            "build reqwest clients through `exp_http::client()` / `exp_http::blocking()` \
             (reqwest 0.13 panics at build without the crypto provider they install):\n{}",
            offenders.join("\n")
        );
    }

    #[test]
    fn the_gate_sees_every_spelling() {
        let hits = constructions(
            "use reqwest::blocking::Client;\n\
             let a = Client::new();\n\
             let b = reqwest::Client::builder();\n\
             let c = reqwest::blocking::Client::new();\n\
             let d = reqwest::blocking::get(url);\n\
             let e = AuthClient::new();\n\
             // reqwest::Client::new() in a comment\n",
        );
        assert_eq!(hits.len(), 4, "{hits:?}");
    }

    #[test]
    fn a_client_builds_without_a_host_provider() {
        super::blocking().build().expect("a blocking client");
        super::client().build().expect("an async client");
    }
}
