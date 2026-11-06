//! `--target web` end to end: the album sample (unsigned) on a web runner kit.
//! The kit is a stand-in with the runner's generated files, or the real one
//! from `scripts/runner-kits.sh --out <dir> web` via `EENGINE_BUNDLE_WEB_KIT=<dir>/web`.

use std::path::{Path, PathBuf};

use eengine_bundle::{run, Options};
use serde_json::Value;

const ALBUM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../runner/web/samples/album.app.json");
const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="5" fill="#2563eb"/><circle cx="12" cy="12" r="5" fill="#fff"/></svg>"##;

fn stand_in_kit(dir: &Path) {
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<html><head><title>App</title></head><body></body></html>").unwrap();
    std::fs::write(dir.join("assets/index-x.js"), "console.log(1)").unwrap();
    std::fs::write(dir.join("app/package.app.json"), "{}").unwrap();
    std::fs::write(dir.join("sw.js"), "// Generated\nconst CACHE = \"exp-runner-kit\"\nconst FILES = [\"./\"]\nself.addEventListener(\"install\", () => {})\n").unwrap();
    let glyphs = serde_json::json!({
        "icons": {"image": [["rect", {"width": "18"}]], "circle-x": [["circle", {"r": "10"}]], "shield-check": [["path", {"d": "M1"}]], "camera": [["circle", {"r": "3"}]]},
        "alias": {"ui-photos": "image", "ui-error": "circle-x", "ui-permission": "shield-check", "edit": "camera"}
    });
    std::fs::write(dir.join("glyphs.json"), glyphs.to_string()).unwrap();
    std::fs::write(dir.join("NOTICES.txt"), "stub notices\n").unwrap();
}

#[test]
fn bundles_the_album_sample() {
    let tmp = tempfile::tempdir().unwrap();
    let runners = tmp.path().join("kits");
    let real = std::env::var_os("EENGINE_BUNDLE_WEB_KIT").map(PathBuf::from);
    match &real {
        Some(kit) => copy(kit, &runners.join("web")),
        None => stand_in_kit(&runners.join("web")),
    }
    std::fs::write(tmp.path().join("icon.svg"), ICON).unwrap();
    std::fs::write(tmp.path().join("bundle.toml"), "id = \"at.acme.album\"\nname = \"Album\"\nversion = \"1.2.0\"\n").unwrap();
    let out = tmp.path().join("dist");
    let done = run(&Options {
        package: ALBUM.into(),
        icon: tmp.path().join("icon.svg"),
        config: tmp.path().join("bundle.toml"),
        targets: vec!["web".into()],
        runners: Some(runners),
        out: out.clone(),
        allow_unsigned: true,
        cache: tmp.path().join("cache"),
    })
    .unwrap();
    let dir = out.join("Album-1.2.0-web");
    assert_eq!(done[0].1.files, vec![dir.clone()]);
    assert!(!out.join(".work").exists());

    // app/ = the package byte for byte + the prepared config.
    assert_eq!(std::fs::read(dir.join("app/package.app.json")).unwrap(), std::fs::read(ALBUM).unwrap());
    let config: Value = serde_json::from_slice(&std::fs::read(dir.join("app/runner.json")).unwrap()).unwrap();
    assert_eq!((config["id"].as_str(), config["version"].as_str()), (Some("at.acme.album"), Some("1.2.0")));

    let manifest: Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.webmanifest")).unwrap()).unwrap();
    assert_eq!(manifest["name"], "Album");
    for px in [192u32, 512] {
        let png = image::load_from_memory(&std::fs::read(dir.join(format!("icons/icon-{px}.png"))).unwrap()).unwrap();
        assert_eq!((png.width(), png.height()), (px, px));
    }
    assert_eq!(&std::fs::read(dir.join("favicon.ico")).unwrap()[..6], &[0, 0, 1, 0, 3, 0]);
    assert!(std::fs::read_to_string(dir.join("index.html")).unwrap().contains("<title>Album</title>"));
    assert!(dir.join("NOTICES.txt").is_file());

    // glyphs.json: what the album and the shell draw, nothing else.
    let glyphs: Value = serde_json::from_slice(&std::fs::read(dir.join("glyphs.json")).unwrap()).unwrap();
    let icons = glyphs["icons"].as_object().unwrap();
    // The album draws `ui-photos`, the shell `ui-error` and `ui-permission`.
    for name in ["ui-photos", "ui-error", "ui-permission"] {
        let glyph = glyphs["alias"][name].as_str().unwrap_or_else(|| panic!("{name}: {glyphs}"));
        assert!(icons.contains_key(glyph), "{name}: {glyphs}");
    }
    if real.is_none() {
        assert!(!icons.contains_key("camera"), "{glyphs}");
    }

    // sw.js precaches every final file under a cache named by their hash.
    let sw = std::fs::read_to_string(dir.join("sw.js")).unwrap();
    let files_line = sw.lines().find(|l| l.starts_with("const FILES = ")).unwrap();
    let files: Vec<String> = serde_json::from_str(files_line.trim_start_matches("const FILES = ")).unwrap();
    let expected = eengine_bundle::targets::web::export_files(&dir).unwrap();
    assert_eq!(files[0], "./");
    assert_eq!(files[1..].to_vec(), expected.iter().map(|f| format!("./{f}")).collect::<Vec<_>>());
    for f in ["./NOTICES.txt", "./glyphs.json", "./favicon.ico", "./app/runner.json", "./manifest.webmanifest"] {
        assert!(files.iter().any(|x| x == f), "{f}");
    }
    let cache = eengine_bundle::targets::web::cache_name(&dir, &expected).unwrap();
    assert!(sw.contains(&format!("const CACHE = \"{cache}\"")));
    assert!(!sw.contains("exp-runner-kit"));
}

fn copy(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let dest = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &dest);
        } else {
            std::fs::create_dir_all(to).unwrap();
            std::fs::copy(&path, &dest).unwrap();
        }
    }
}
