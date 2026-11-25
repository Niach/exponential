//! VAPP-271: the Photobook sample prepares for export: a core-catalog
//! package declaring exactly photos, files.write, print and share, offline.
//! Until the pinned signer signs it, only `--allow-unsigned` takes it.

use std::path::{Path, PathBuf};
use std::process::Command;

use eengine_bundle::prepare;
use serde_json::json;

const FILE: &[u8] = include_bytes!("../../../../../samples/apps/photobook.app.json");

/// Round 9 §14.1: the committed sample stays a signed format 5 file until the
/// pinned signer re-signs it (`a2ui10-migrate.ts --apply --files`).
fn awaits_the_signer() -> bool {
    let waits = serde_json::from_slice::<serde_json::Value>(FILE).unwrap()["package"]["formatVersion"] == 5;
    if waits {
        eprintln!("skipped: samples/apps/photobook.app.json is format 5 until the pinned signer re-signs it");
    }
    waits
}

#[test]
fn the_photobook_prepares() {
    if awaits_the_signer() {
        return;
    }
    let toml = json!({"id": "at.exponential.photobook", "name": "Photobook", "version": "1.0.0"});
    let signed = serde_json::from_slice::<serde_json::Value>(FILE).unwrap().get("signature").is_some();
    if !signed {
        assert_eq!(prepare(FILE, &toml, false).err().map(|r| r.code), Some("unsigned"));
    }
    let prepared = prepare(FILE, &toml, !signed).expect("the Photobook prepares");
    assert_eq!(prepared.scopes, ["files.write", "photos.read", "print.pdf", "share.send"]);
    assert_eq!(prepared.package["catalogId"], "https://ui.exponential.at/catalogs/core/v1");
}

const ID: &str = "at.exponential.samples.photobook";

/// "Photobook builds": the signed sample with its committed icon and
/// bundle.toml (`samples/apps/photobook.*`), WITHOUT `--allow-unsigned`, on
/// stub kits unless `EENGINE_BUNDLE_RUNNERS=<dir>` holds real ones
/// (`check-photobook-bundle.yml`, `check-eengine-runner-windows.yml`).
/// `EENGINE_BUNDLE_OUT=<dir>` keeps the outputs (`<dir>/<target>/`). The
/// AppImage needs `mksquashfs`, the setup.exe `makensis`: without them the
/// refusal names the tool, unless `EENGINE_BUNDLE_REQUIRE` lists the target.
struct Bundle {
    tmp: tempfile::TempDir,
    runners: PathBuf,
    out: PathBuf,
}

impl Bundle {
    fn new() -> Self {
        assert!(serde_json::from_slice::<serde_json::Value>(FILE).unwrap().get("signature").is_some(), "the committed Photobook is signed");
        let tmp = tempfile::tempdir().unwrap();
        let runners = std::env::var_os("EENGINE_BUNDLE_RUNNERS").map(PathBuf::from).unwrap_or_else(|| stub_kits(tmp.path()));
        let out = std::env::var_os("EENGINE_BUNDLE_OUT").map(PathBuf::from).unwrap_or_else(|| tmp.path().join("dist"));
        Bundle { tmp, runners, out }
    }

    fn run(&self, target: &str) -> anyhow::Result<Vec<(String, eengine_bundle::targets::Output)>> {
        let samples = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../samples/apps");
        eengine_bundle::run(&eengine_bundle::Options {
            package: format!("{samples}/photobook.app.json").into(),
            icon: format!("{samples}/photobook.icon.svg").into(),
            config: format!("{samples}/photobook.bundle.toml").into(),
            targets: vec![target.into()],
            runners: Some(self.runners.clone()),
            out: self.out.join(target),
            allow_unsigned: false,
            cache: self.tmp.path().join("cache"),
        })
    }

    /// The target ran, or (its tool missing and not required) refused naming the tool.
    fn tool_ran(&self, target: &str, tool: &str, env: &str, result: anyhow::Result<Vec<(String, eengine_bundle::targets::Output)>>) -> bool {
        let installed = std::env::var_os(env).is_some() || Command::new(tool).arg("-version").output().is_ok();
        let required = std::env::var("EENGINE_BUNDLE_REQUIRE").map(|v| v.split(',').any(|t| t.trim() == target)).unwrap_or(false);
        if installed || required {
            result.unwrap_or_else(|e| panic!("{target}: {e:#}"));
            return true;
        }
        let err = format!("{:#}", result.err().expect("no tool: refused"));
        assert!(err.contains(tool), "{err}");
        eprintln!("skipped the {target} package that needs {tool}");
        false
    }
}

#[test]
fn the_signed_photobook_bundles_for_linux_and_web() {
    if awaits_the_signer() {
        return;
    }
    let b = Bundle::new();
    let linux = b.run("linux");
    let deb = std::fs::read(b.out.join(format!("linux/{ID}_1.0.0_amd64.deb"))).expect("the .deb");
    assert!(deb.starts_with(b"!<arch>\n"));
    if b.tool_ran("linux", "mksquashfs", "MKSQUASHFS", linux) {
        let appimage = std::fs::read(b.out.join("linux/Photobook-1.0.0-x86_64.AppImage")).expect("the AppImage");
        assert!(appimage.len() > deb.len() / 2);
    }

    b.run("web").expect("the web target");
    let web = b.out.join("web/Photobook-1.0.0-web");
    assert_eq!(std::fs::read(web.join("app/package.app.json")).unwrap(), FILE, "byte for byte");
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(web.join("app/runner.json")).unwrap()).unwrap();
    assert_eq!((config["id"].as_str(), config["network"].as_str()), (Some(ID), Some("off")));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(web.join("manifest.webmanifest")).unwrap()).unwrap();
    assert_eq!(manifest["name"], "Photobook");
    assert!(web.join("icons/icon-512.png").is_file() && web.join("NOTICES.txt").is_file());
}

#[test]
fn the_signed_photobook_bundles_for_windows() {
    if awaits_the_signer() {
        return;
    }
    let b = Bundle::new();
    let result = b.run("windows");
    if b.tool_ran("windows", "makensis", "MAKENSIS", result) {
        let setup = std::fs::read(b.out.join("windows/Photobook-1.0.0-setup.exe")).expect("the setup.exe");
        assert!(setup.starts_with(b"MZ"));
    }
}

fn stub_kits(tmp: &Path) -> PathBuf {
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
    let web = tmp.join("kits/web");
    std::fs::create_dir_all(web.join("app")).unwrap();
    std::fs::write(web.join("index.html"), "<html><head><title>App</title></head><body></body></html>").unwrap();
    std::fs::write(web.join("sw.js"), "// Generated\nconst CACHE = \"exp-runner-kit\"\nconst FILES = [\"./\"]\n").unwrap();
    std::fs::write(web.join("glyphs.json"), r#"{"icons": {"image": [["rect", {"width": "18"}]]}, "alias": {"ui-photos": "image"}}"#).unwrap();
    std::fs::write(web.join("NOTICES.txt"), "stub notices\n").unwrap();
    tmp.join("kits")
}
