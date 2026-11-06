//! `--target linux` and `--target windows` bundle the web runner's sample
//! (`apps/runner/web/samples/album.app.json`, `--allow-unsigned`).
//!
//! The kits are stubs (a runner that is a few bytes, a stub AppImage runtime)
//! unless `EENGINE_BUNDLE_RUNNERS=<dir>` names real ones (`scripts/runner-kits.sh`).
//! The AppImage needs `mksquashfs` and the installer `makensis`: without them the
//! test checks the refusal names the tool, unless `EENGINE_BUNDLE_REQUIRE` lists
//! the target (CI sets it where the tools are installed). `EENGINE_BUNDLE_OUT=<dir>`
//! keeps the outputs there (`<dir>/<target>/`) for the steps after.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../runner/web/samples/album.app.json");
const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#09090b"/><rect x="14" y="18" width="36" height="28" rx="4" fill="none" stroke="#fafafa" stroke-width="4"/><circle cx="26" cy="29" r="4" fill="#fafafa"/><path d="M18 44l10-10 8 8 6-6 8 8z" fill="#fafafa"/></svg>"##;
const TOML: &str = "id = \"at.acme.album\"\nname = \"Album\"\nversion = \"1.0.0\"\n";

fn required(target: &str) -> bool {
    std::env::var("EENGINE_BUNDLE_REQUIRE").map(|v| v.split(',').any(|t| t.trim() == target)).unwrap_or(false)
}

fn on_path(tool: &str, env: &str) -> bool {
    std::env::var_os(env).is_some() || Command::new(tool).arg("-version").output().is_ok() || Command::new(tool).arg("--help").output().is_ok()
}

/// The kits dir: the real one, else stubs in `tmp`.
fn kits(tmp: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("EENGINE_BUNDLE_RUNNERS") {
        return PathBuf::from(dir);
    }
    let linux = tmp.join("kits/linux");
    std::fs::create_dir_all(linux.join("x86_64")).unwrap();
    std::fs::write(linux.join("x86_64/exponential-runner"), b"\x7fELF stub runner").unwrap();
    std::fs::write(linux.join("x86_64/appimage-runtime"), b"stub runtime\0").unwrap();
    std::fs::write(linux.join("NOTICES.txt"), b"stub notices\n").unwrap();
    std::fs::write(linux.join("appimage-runtime.NOTICES.txt"), b"stub runtime notices\n").unwrap();
    let windows = tmp.join("kits/windows");
    std::fs::create_dir_all(&windows).unwrap();
    std::fs::write(windows.join("exponential-runner.exe"), b"MZ stub runner").unwrap();
    std::fs::write(windows.join("NOTICES.txt"), b"stub notices\n").unwrap();
    tmp.join("kits")
}

fn run(tmp: &Path, target: &str) -> anyhow::Result<Vec<(String, eengine_bundle::targets::Output)>> {
    std::fs::write(tmp.join("icon.svg"), ICON).unwrap();
    std::fs::write(tmp.join("bundle.toml"), TOML).unwrap();
    eengine_bundle::run(&eengine_bundle::Options {
        package: PathBuf::from(SAMPLE),
        icon: tmp.join("icon.svg"),
        config: tmp.join("bundle.toml"),
        targets: vec![target.into()],
        runners: Some(kits(tmp)),
        out: out_dir(tmp, target),
        allow_unsigned: true,
        cache: tmp.join("cache"),
    })
}

fn out_dir(tmp: &Path, target: &str) -> PathBuf {
    std::env::var_os("EENGINE_BUNDLE_OUT").map(|d| PathBuf::from(d).join(target)).unwrap_or_else(|| tmp.join("dist"))
}

/// The members of an `ar` archive.
fn ar_members(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert!(bytes.starts_with(b"!<arch>\n"));
    let mut at = 8;
    let mut out = Vec::new();
    while at + 60 <= bytes.len() {
        let h = &bytes[at..at + 60];
        let name = String::from_utf8_lossy(&h[0..16]).trim().trim_end_matches('/').to_string();
        let size: usize = String::from_utf8_lossy(&h[48..58]).trim().parse().unwrap();
        out.push((name, bytes[at + 60..at + 60 + size].to_vec()));
        at += 60 + size + size % 2;
    }
    out
}

fn untar(gz: &[u8]) -> Vec<(String, u32, Vec<u8>)> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz));
    archive.entries().unwrap().map(|e| {
        let mut e = e.unwrap();
        let mut data = Vec::new();
        e.read_to_end(&mut data).unwrap();
        let path = String::from_utf8_lossy(&e.path_bytes()).into_owned();
        (path, e.header().mode().unwrap(), data)
    }).collect()
}

#[test]
fn linux_writes_the_deb_and_the_appimage() {
    let tmp = tempfile::tempdir().unwrap();
    let result = run(tmp.path(), "linux");
    let deb_file = out_dir(tmp.path(), "linux").join("at.acme.album_1.0.0_amd64.deb");
    let deb = std::fs::read(&deb_file).expect("the .deb is written first");
    let members = ar_members(&deb);
    assert_eq!(members.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["debian-binary", "control.tar.gz", "data.tar.gz"]);
    assert_eq!(members[0].1, b"2.0\n");
    let control = untar(&members[1].1);
    let control = String::from_utf8(control.iter().find(|e| e.0 == "./control").unwrap().2.clone()).unwrap();
    for line in ["Package: at.acme.album", "Version: 1.0.0", "Architecture: amd64", "Maintainer: Album"] {
        assert!(control.lines().any(|l| l == line), "{line} in\n{control}");
    }
    let data = untar(&members[2].1);
    let paths: Vec<&str> = data.iter().map(|e| e.0.as_str()).filter(|p| !p.ends_with('/')).collect();
    let mut want = vec![
        "./usr/bin/at.acme.album",
        "./usr/share/applications/at.acme.album.desktop",
        "./usr/share/at.acme.album/app/package.app.json",
        "./usr/share/at.acme.album/app/runner.json",
        "./usr/share/doc/at.acme.album/NOTICES.txt",
    ];
    for n in [32, 48, 64, 128, 256, 512] {
        want.push(Box::leak(format!("./usr/share/icons/hicolor/{n}x{n}/apps/at.acme.album.png").into_boxed_str()));
    }
    let mut got = paths.clone();
    got.sort();
    want.sort();
    assert_eq!(got, want);
    let file = |p: &str| data.iter().find(|e| e.0 == p).unwrap();
    assert_eq!(file("./usr/bin/at.acme.album").1, 0o755);
    assert_eq!(file("./usr/share/at.acme.album/app/package.app.json").2, std::fs::read(SAMPLE).unwrap(), "byte for byte");
    let config: serde_json::Value = serde_json::from_slice(&file("./usr/share/at.acme.album/app/runner.json").2).unwrap();
    assert_eq!(config["id"], "at.acme.album");
    let desktop = String::from_utf8(file("./usr/share/applications/at.acme.album.desktop").2.clone()).unwrap();
    assert!(desktop.contains("Exec=at.acme.album %u\n") && desktop.contains("MimeType=x-scheme-handler/at.acme.album;\n"), "{desktop}");
    if Command::new("dpkg-deb").arg("--version").output().is_ok() {
        let info = Command::new("dpkg-deb").arg("-I").arg(&deb_file).output().unwrap();
        assert!(info.status.success(), "dpkg-deb -I: {}", String::from_utf8_lossy(&info.stderr));
        let list = Command::new("dpkg-deb").arg("-c").arg(&deb_file).output().unwrap();
        assert!(String::from_utf8_lossy(&list.stdout).contains("root/root") && list.status.success());
    }

    let appimage = out_dir(tmp.path(), "linux").join("Album-1.0.0-x86_64.AppImage");
    if !on_path("mksquashfs", "MKSQUASHFS") && !required("linux") {
        let err = format!("{:#}", result.err().expect("no mksquashfs: refused"));
        assert!(err.contains("mksquashfs"), "{err}");
        eprintln!("skipped the AppImage: no mksquashfs");
        return;
    }
    let outputs = result.unwrap();
    assert_eq!(outputs[0].1.files, vec![deb_file.clone(), appimage.clone()]);
    let bytes = std::fs::read(&appimage).unwrap();
    let runtime = std::fs::read(kits(tmp.path()).join("linux/x86_64/appimage-runtime")).unwrap();
    assert!(bytes.starts_with(&runtime), "runtime first");
    assert_eq!(&bytes[runtime.len()..runtime.len() + 4], b"hsqs", "then the squashfs");
    if Command::new("unsquashfs").arg("-help").output().is_ok() {
        let list = Command::new("unsquashfs").args(["-l", "-o"]).arg(runtime.len().to_string()).arg(&appimage).output().unwrap();
        let list = String::from_utf8_lossy(&list.stdout);
        for p in ["squashfs-root/AppRun", "squashfs-root/at.acme.album", "squashfs-root/at.acme.album.desktop", "squashfs-root/at.acme.album.png", "squashfs-root/.DirIcon", "squashfs-root/NOTICES.txt", "squashfs-root/app/package.app.json", "squashfs-root/app/runner.json"] {
            assert!(list.lines().any(|l| l == p), "{p} in\n{list}");
        }
    }
}

#[test]
fn windows_writes_the_installer() {
    let tmp = tempfile::tempdir().unwrap();
    let result = run(tmp.path(), "windows");
    if !on_path("makensis", "MAKENSIS") && !required("windows") {
        let err = format!("{:#}", result.err().expect("no makensis: refused"));
        assert!(err.contains("makensis"), "{err}");
        eprintln!("skipped the installer: no makensis");
        return;
    }
    let outputs = result.unwrap();
    let setup = out_dir(tmp.path(), "windows").join("Album-1.0.0-setup.exe");
    assert_eq!(outputs[0].1.files.iter().map(|f| f.canonicalize().unwrap()).collect::<Vec<_>>(), vec![setup.canonicalize().unwrap()]);
    let bytes = std::fs::read(&setup).unwrap();
    assert_eq!(&bytes[..2], b"MZ");
    assert!(outputs[0].1.notes.iter().any(|n| n.contains("unsigned")));
    let seven = std::env::var("SEVENZIP").unwrap_or_else(|_| "7z".into());
    if let Ok(list) = Command::new(&seven).arg("l").arg(&setup).output() {
        let list = String::from_utf8_lossy(&list.stdout).replace('\\', "/");
        for f in ["Album.exe", "app.ico", "NOTICES.txt", "app/package.app.json", "app/runner.json"] {
            assert!(list.lines().any(|l| l.ends_with(&format!(" {f}"))), "{f} in\n{list}");
        }
    }
}

/// `[signing.windows]`: a throwaway self-signed pfx (openssl) signs the app's
/// `.exe` and the installer. The kit's runner is a real PE: an unsigned
/// installer from a first run.
#[test]
fn windows_signs_with_the_authors_pfx() {
    let tools = on_path("makensis", "MAKENSIS") && on_path("osslsigncode", "OSSLSIGNCODE") && Command::new("openssl").arg("version").output().is_ok();
    if !tools || std::env::var_os("EENGINE_BUNDLE_RUNNERS").is_some() {
        eprintln!("skipped: needs makensis, osslsigncode and openssl, and the stub kits");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let first = run(tmp.path(), "windows").unwrap();
    let pe = first[0].1.files[0].clone();
    let t = tmp.path().join("signed");
    std::fs::create_dir_all(&t).unwrap();
    kits(&t);
    std::fs::copy(&pe, t.join("kits/windows/exponential-runner.exe")).unwrap();
    let ok = Command::new("openssl").current_dir(&t).args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "key.pem", "-out", "cert.pem", "-days", "1", "-subj", "/CN=Acme Test Signing"]).output().unwrap().status.success()
        && Command::new("openssl").current_dir(&t).args(["pkcs12", "-export", "-inkey", "key.pem", "-in", "cert.pem", "-out", "sign.pfx", "-passout", "pass:hunter2"]).output().unwrap().status.success();
    assert!(ok, "openssl made the pfx");
    std::env::set_var("EENGINE_BUNDLE_TEST_PFX_PASSWORD", "hunter2");
    let toml = format!("{TOML}[signing.windows]\npfx = \"{}\"\npassword_env = \"EENGINE_BUNDLE_TEST_PFX_PASSWORD\"\n", t.join("sign.pfx").display());
    std::fs::write(t.join("icon.svg"), ICON).unwrap();
    std::fs::write(t.join("bundle.toml"), toml).unwrap();
    let out = eengine_bundle::run(&eengine_bundle::Options {
        package: PathBuf::from(SAMPLE),
        icon: t.join("icon.svg"),
        config: t.join("bundle.toml"),
        targets: vec!["windows".into()],
        runners: Some(t.join("kits")),
        out: t.join("dist"),
        allow_unsigned: true,
        cache: t.join("cache"),
    })
    .unwrap();
    assert!(out[0].1.notes.iter().all(|n| !n.contains("unsigned")));
    let osslsigncode = std::env::var("OSSLSIGNCODE").unwrap_or_else(|_| "osslsigncode".into());
    let verify = Command::new(&osslsigncode).args(["verify", "-in"]).arg(&out[0].1.files[0]).output().unwrap();
    let text = String::from_utf8_lossy(&verify.stdout);
    assert!(text.contains("Acme Test Signing"), "the installer carries the author's signature:\n{text}");
    assert!(!t.join("dist/.work").exists(), "the password file went with the scratch dir");
}
