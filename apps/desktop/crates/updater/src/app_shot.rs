//! #FEED-244: the device half of `exponential_apps {op: "screenshot"}`,
//! shared by the CLI daemon and the desktop app (both answer the
//! `app_screenshot` device command through [`run_command`]).
//!
//! The server resolves the screen into a bundle (eui-shot's format) and mints
//! a signed upload URL; the device paints the bundle with `exponential-shot`
//! (crates/eui-shot: its own binary, since gpui's headless renderer only
//! exists under `test-support`) and PUTs the PNG to that URL. The server
//! finalizes the attachment as the caller.
//!
//! Where `exponential-shot` comes from, first hit wins: `EXP_SHOT_BIN`; next
//! to the running executable (a dev build's target dir, a package that ships
//! it); the per-user tools cache; else the newest CLI release's
//! `exponential-shot-<triple>` asset, verified against its signed
//! SHA256SUMS like every update, into that cache.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _, Result};
use serde_json::{json, Value};

/// The device command kind (apps/web `APP_SCREENSHOT_COMMAND`).
pub const COMMAND_KIND: &str = "app_screenshot";
/// The heartbeat cap: this device answers [`COMMAND_KIND`]
/// (apps/web `APP_SCREENSHOT_CAP`).
pub const CAP: &str = "app-screenshot";
/// The binary's file name.
pub const BINARY: &str = if cfg!(windows) { "exponential-shot.exe" } else { "exponential-shot" };
/// How long one paint may take before it is killed.
const RENDER_TIMEOUT: Duration = Duration::from_secs(45);

/// The `cli-v*` release asset for this machine (build-cli.yml's matrix).
pub fn asset_name() -> String {
    format!("exponential-shot-{}", crate::cli_target_triple())
}

/// Whether a release carries a renderer for this OS/arch (build-cli.yml).
pub fn released_for_this_machine() -> bool {
    matches!(
        (std::env::consts::OS, std::env::consts::ARCH),
        ("linux", "x86_64") | ("linux", "aarch64") | ("macos", "aarch64")
    )
}

/// Whether this device reports [`CAP`]: a local renderer, or one it can fetch.
pub fn capable() -> bool {
    local_binary().is_some() || released_for_this_machine()
}

fn tools_dir() -> Option<PathBuf> {
    crate::staging_dir().and_then(|dir| dir.parent().map(|p| p.join("tools")))
}

/// A renderer already on disk ([`fetch_binary`]'s cache aside).
fn local_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EXP_SHOT_BIN").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let sibling = std::env::current_exe().ok()?.parent()?.join(BINARY);
    sibling.is_file().then_some(sibling)
}

/// The renderer to run: local, cached, else fetched from the newest CLI
/// release (verified).
pub fn binary() -> Result<PathBuf> {
    if let Some(path) = local_binary() {
        return Ok(path);
    }
    if !released_for_this_machine() {
        bail!(
            "this device ({} {}) has no app renderer: screenshots render on Linux x86_64/arm64 or Apple-silicon macOS",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    }
    let version = crate::feed::advertised_latest("cli")
        .context("find the app renderer's release")?
        .context("the instance advertises no CLI release to take the app renderer from")?;
    fetch_binary(&format!("cli-v{version}"))
}

/// The cached renderer of release `tag`, fetched and verified on first use.
fn fetch_binary(tag: &str) -> Result<PathBuf> {
    let dir = tools_dir().context("no per-user data directory for the app renderer")?;
    let path = dir.join(format!("{}-{tag}", BINARY));
    if path.is_file() {
        return Ok(path);
    }
    let asset = asset_name();
    let files = crate::feed::probe_release(tag, &asset)?
        .with_context(|| format!("release {tag} carries no {asset}"))?;
    let sums = crate::fetch_verified_sums(&files.sums_url, files.sig_url.as_deref())?;
    let staged = dir.join(format!("{asset}.download"));
    crate::download(&files.asset_url, &staged, |_, _| {})?;
    if let Err(e) = crate::verify_sha256(&staged, &sums, &asset) {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).context("make the app renderer executable")?;
    }
    std::fs::rename(&staged, &path).context("install the app renderer")?;
    // Older releases' renderers go.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{BINARY}-")) && entry.path() != path {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(path)
}

/// One `app_screenshot` command: `{bundle, uploadUrl}` (strings) → the PNG
/// painted and PUT; the result = `{width, height, bytes}` (JSON). `Err` = the
/// refusal the server answers the agent with.
pub fn run_command(payload: &Value) -> Result<String> {
    let bundle = payload.get("bundle").and_then(Value::as_str).context("the command has no bundle")?;
    let upload_url = payload.get("uploadUrl").and_then(Value::as_str).context("the command has no uploadUrl")?;
    let png = render(&binary()?, bundle)?;
    let (width, height) = png_size(&png).context("the renderer wrote no PNG")?;
    upload(upload_url, &png)?;
    Ok(json!({"width": width, "height": height, "bytes": png.len()}).to_string())
}

/// Run the renderer on `bundle` (its JSON text) → the PNG's bytes.
pub fn render(binary: &Path, bundle: &str) -> Result<Vec<u8>> {
    let dir = std::env::temp_dir().join(format!(
        "exp-shot-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).context("create the screenshot's temp dir")?;
    let result = (|| {
        let input = dir.join("bundle.json");
        let output = dir.join("shot.png");
        std::fs::write(&input, bundle).context("write the bundle")?;
        let mut child = Command::new(binary)
            .arg(&input)
            .arg(&output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("start {}", binary.display()))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().context("wait for the renderer")? {
                break status;
            }
            if started.elapsed() > RENDER_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                bail!("the renderer took longer than {}s", RENDER_TIMEOUT.as_secs());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            use std::io::Read as _;
            let _ = pipe.read_to_string(&mut stderr);
        }
        if !status.success() {
            bail!("{}", refusal(&stderr, status.code()));
        }
        std::fs::read(&output).context("read the rendered PNG")
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// The renderer's refusal: its last non-empty stderr line (a crash without
/// one names the exit).
pub fn refusal(stderr: &str, code: Option<i32>) -> String {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| match code {
            Some(code) => format!("the renderer failed (exit {code})"),
            None => "the renderer crashed".to_string(),
        })
}

/// A PNG's `(width, height)` from its IHDR.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let be = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    Some((be(16), be(20)))
}

/// PUT the PNG to the signed upload URL (the server's `/api/attachment-uploads/<token>`).
fn upload(url: &str, png: &[u8]) -> Result<()> {
    let client = exp_http::blocking().timeout(Duration::from_secs(60)).build().context("build the upload client")?;
    let response = client
        .put(url)
        .header("Content-Type", "image/png")
        .body(png.to_vec())
        .send()
        .context("upload the screenshot")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        bail!("the screenshot upload answered HTTP {status}: {}", body.chars().take(200).collect::<String>());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_asset_follows_the_cli_triple() {
        assert_eq!(asset_name(), format!("exponential-shot-{}", crate::cli_target_triple()));
    }

    #[test]
    fn a_refusal_is_the_last_stderr_line() {
        assert_eq!(refusal("warn: x\nno GPU renderer\n\n", Some(2)), "no GPU renderer");
        assert_eq!(refusal("", Some(101)), "the renderer failed (exit 101)");
        assert_eq!(refusal("  ", None), "the renderer crashed");
    }

    #[test]
    fn png_size_reads_the_ihdr() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(390u32.to_be_bytes());
        png.extend(844u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((390, 844)));
        assert_eq!(png_size(b"GIF89a"), None);
    }

    #[test]
    fn a_command_without_its_fields_is_refused() {
        assert!(run_command(&json!({"uploadUrl": "x"})).unwrap_err().to_string().contains("no bundle"));
        assert!(run_command(&json!({"bundle": "{}"})).unwrap_err().to_string().contains("no uploadUrl"));
    }

    #[cfg(unix)]
    #[test]
    fn the_renderer_runs_as_a_subprocess_and_its_refusal_comes_back() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("ok.sh");
        // A fake renderer: copies a PNG header to the output path.
        std::fs::write(&ok, "#!/bin/sh\nprintf '\\211PNG\\r\\n\\032\\n' > \"$2\"\n").unwrap();
        std::fs::set_permissions(&ok, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(&render(&ok, "{}").unwrap()[1..4], b"PNG");
        let bad = dir.path().join("bad.sh");
        std::fs::write(&bad, "#!/bin/sh\necho 'this device has no GPU renderer' >&2\nexit 2\n").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(render(&bad, "{}").unwrap_err().to_string(), "this device has no GPU renderer");
    }
}
