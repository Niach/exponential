//! `EXP_DATA_DIR` isolates the CLI end to end: `whoami` reads, and `login`
//! writes, the override's `accounts.json` + token files — never the default
//! data dir (here a decoy under a fake HOME/XDG_DATA_HOME, signed in as a
//! different person), and two override dirs never see each other.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use api::accounts::AuthStore;
use api::login::AuthUser;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("exp-cli-datadir-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(root.join("home")).unwrap();
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn xdg_data(&self) -> PathBuf {
        self.home().join(".local/share")
    }

    /// Where the CLI resolves its data dir WITHOUT an override (the dir the
    /// desktop app shares), under the fake HOME.
    fn default_data_dir(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.home().join("Library/Application Support/at.exponential")
        } else {
            self.xdg_data().join("exponential")
        }
    }

    /// A fresh data dir with auto-update off (no release check from a test).
    fn data_dir(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "{\"cliAutoUpdate\": false}\n").unwrap();
        dir
    }

    fn run(&self, data_dir: Option<&Path>, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_exponential"));
        command
            .args(args)
            .env("HOME", self.home())
            .env("XDG_DATA_HOME", self.xdg_data())
            .env("XDG_CONFIG_HOME", self.home().join(".config"))
            .env("XDG_CACHE_HOME", self.home().join(".cache"))
            .env("EXP_DIAGNOSTICS", "0")
            .env("NO_PROXY", "127.0.0.1,localhost")
            .stdin(std::process::Stdio::null());
        for key in [
            "EXP_DATA_DIR",
            "EXP_TOKEN",
            "EXP_ACCOUNT",
            "EXP_INSTANCE",
            "EXP_INSTALL_TOKEN",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env_remove(key);
        }
        if let Some(dir) = data_dir {
            command.env("EXP_DATA_DIR", dir);
        }
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command.output().expect("run the exponential binary")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn user(id: &str, email: &str) -> AuthUser {
    AuthUser {
        id: id.to_string(),
        email: email.to_string(),
        name: None,
        is_admin: None,
        onboarding_completed_at: None,
    }
}

fn sign_in(data_dir: &Path, instance: &str, id: &str, email: &str) {
    std::fs::create_dir_all(data_dir).unwrap();
    AuthStore::load(data_dir.to_path_buf())
        .sign_in(instance, &format!("token-{id}"), &user(id, email))
        .unwrap();
}

fn emails(data_dir: &Path) -> Vec<String> {
    AuthStore::load(data_dir.to_path_buf())
        .signed_in_accounts()
        .into_iter()
        .map(|account| account.email)
        .collect()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn two_data_dirs_never_see_each_others_accounts() {
    let sandbox = Sandbox::new("stores");
    let a = sandbox.data_dir("a");
    let b = sandbox.data_dir("b");
    sign_in(&a, "https://a.example", "user-a", "a@example.test");
    sign_in(&b, "https://b.example", "user-b", "b@example.test");
    assert_eq!(emails(&a), vec!["a@example.test".to_string()]);
    assert_eq!(emails(&b), vec!["b@example.test".to_string()]);
    AuthStore::load(a.clone()).sign_out(&api::accounts::account_id_for("https://a.example", "user-a"));
    assert!(emails(&a).is_empty());
    assert_eq!(emails(&b), vec!["b@example.test".to_string()]);
}

#[test]
fn whoami_reads_the_exp_data_dir_account_only() {
    let sandbox = Sandbox::new("whoami");
    sign_in(&sandbox.default_data_dir(), "https://app.example", "real", "real@example.test");
    let a = sandbox.data_dir("a");
    let b = sandbox.data_dir("b");
    let empty = sandbox.data_dir("empty");
    sign_in(&a, "https://a.example", "user-a", "a@example.test");
    sign_in(&b, "https://b.example", "user-b", "b@example.test");

    // No override: the default dir (proves the decoy is where the CLI looks).
    let default = sandbox.run(None, &["whoami"], &[]);
    assert!(stdout(&default).contains("real@example.test on https://app.example"), "{default:?}");

    let out_a = sandbox.run(Some(&a), &["whoami"], &[]);
    assert!(out_a.status.success(), "{out_a:?}");
    assert_eq!(stdout(&out_a).trim(), "a@example.test on https://a.example");

    let out_b = sandbox.run(Some(&b), &["whoami"], &[]);
    assert_eq!(stdout(&out_b).trim(), "b@example.test on https://b.example");

    let out_empty = sandbox.run(Some(&empty), &["whoami"], &[]);
    assert!(!out_empty.status.success(), "{out_empty:?}");
    assert!(stdout(&out_empty).contains("Not signed in"), "{out_empty:?}");
    assert!(!stdout(&out_empty).contains("real@example.test"), "{out_empty:?}");
}

/// A one-thread HTTP/1.1 stand-in for the instance: `get-session` resolves
/// any bearer to `login@example.test`; everything else is a 404 (the
/// personal-key mint after login is best-effort).
fn fake_instance() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let (status, payload) = if request_line.contains("/api/auth/get-session") {
                (
                    "200 OK",
                    r#"{"user":{"id":"login-user","email":"login@example.test"},"session":{"id":"s"}}"#,
                )
            } else {
                ("404 Not Found", "{}")
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn login_writes_into_exp_data_dir_and_never_the_default_dir() {
    let sandbox = Sandbox::new("login");
    sign_in(&sandbox.default_data_dir(), "https://app.example", "real", "real@example.test");
    let a = sandbox.data_dir("a");
    let instance = fake_instance();

    let login = sandbox.run(Some(&a), &["login", "--instance", &instance], &[("EXP_TOKEN", "test-token")]);
    assert!(login.status.success(), "{login:?}");
    assert!(stdout(&login).contains("Signed in as login@example.test"), "{login:?}");

    assert_eq!(emails(&a), vec!["login@example.test".to_string()]);
    assert_eq!(emails(&sandbox.default_data_dir()), vec!["real@example.test".to_string()]);

    let whoami = sandbox.run(Some(&a), &["whoami"], &[]);
    assert_eq!(stdout(&whoami).trim(), format!("login@example.test on {instance}"));
}

#[test]
fn daemon_install_refuses_an_overridden_data_dir() {
    let sandbox = Sandbox::new("service");
    let a = sandbox.data_dir("a");
    sign_in(&a, "https://a.example", "user-a", "a@example.test");
    let install = sandbox.run(Some(&a), &["daemon", "install"], &[]);
    assert!(!install.status.success(), "{install:?}");
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(stderr.contains("EXP_DATA_DIR"), "{install:?}");
    assert!(!sandbox.home().join(".config/systemd").exists());
    assert!(!sandbox.home().join("Library/LaunchAgents").exists());
}

#[test]
fn help_lists_exp_data_dir() {
    let sandbox = Sandbox::new("help");
    let help = sandbox.run(None, &["--help"], &[]);
    assert!(stdout(&help).contains("EXP_DATA_DIR"), "{help:?}");
}
