//! `--target web` (bundle.json `outputs.web`, `permissions.web`, `signing.web`):
//! the web runner's static export (`apps/runner/web`, `bun run build`) becomes
//! `{name}-{version}-web/`, a PWA directory any static host serves.
//!
//! The kit is the built runner with `NOTICES.txt` beside `index.html`. Over a
//! copy of it: `app/` replaced, `manifest.webmanifest` + the icons + the
//! favicon written, `glyphs.json` pruned to the icons the package and the
//! runner's shell can draw, and `sw.js`'s precache list and cache name
//! recomputed over the final files (the build generated them over the kit's).
//! Nothing to sign: a static directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context as _, Result};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use super::{Job, Output};
use crate::manifest::web_manifest;

/// The runner's glyph file (apps/runner/web/src/glyphs.ts `GLYPHS_FILE`).
pub const GLYPHS_FILE: &str = "glyphs.json";
const SERVICE_WORKER: &str = "sw.js";
const FAVICON_SIZES: [u32; 3] = [16, 32, 48];

pub fn bundle(job: &Job) -> Result<Output> {
    let pattern = crate::contract::strings("/outputs/web").into_iter().next().context("bundle.json outputs.web")?;
    let dir = job.out.join(job.output_name(pattern.trim_end_matches('/'), ""));
    if !job.kit.join("index.html").is_file() || !job.kit.join(SERVICE_WORKER).is_file() {
        bail!("{} is not a web runner kit (no index.html or {SERVICE_WORKER}; scripts/runner-kits.sh web)", job.kit.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&job.kit, &dir)?;
    std::fs::copy(job.notices(), dir.join(crate::kit::notices_file())).context("copy NOTICES.txt")?;

    let app = dir.join("app");
    let _ = std::fs::remove_dir_all(&app);
    job.write_app_dir(&app)?;

    let config = &job.prepared.config;
    std::fs::write(dir.join("manifest.webmanifest"), format!("{}\n", serde_json::to_string_pretty(&web_manifest(config))?))?;
    std::fs::create_dir_all(dir.join("icons"))?;
    for (_, px) in crate::icons::sizes("web") {
        std::fs::write(dir.join(format!("icons/icon-{px}.png")), job.icon.png(px)?)?;
    }
    let frames = FAVICON_SIZES.iter().map(|&px| Ok((px, job.icon.png(px)?))).collect::<Result<Vec<_>>>()?;
    std::fs::write(dir.join("favicon.ico"), ico(&frames))?;

    let index = std::fs::read_to_string(dir.join("index.html"))?;
    std::fs::write(dir.join("index.html"), with_title(&index, &config.name))?;

    let glyphs_path = dir.join(GLYPHS_FILE);
    let kit_glyphs: Value = match std::fs::read(&glyphs_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("the kit's glyphs.json")?,
        Err(_) => serde_json::json!({"icons": {}, "alias": {}}),
    };
    let names = icon_candidates(&job.prepared.package);
    std::fs::write(&glyphs_path, serde_json::to_vec(&prune_glyphs(&kit_glyphs, &names))?)?;

    let sw = std::fs::read_to_string(dir.join(SERVICE_WORKER))?;
    let files = export_files(&dir)?;
    std::fs::write(dir.join(SERVICE_WORKER), rewrite_service_worker(&sw, &cache_name(&dir, &files)?, &files)?)?;

    Ok(Output { files: vec![dir], notes: Vec::new() })
}

/// Every string the package and the runner's shell (runner.json, consent.json)
/// carry: an icon is a string prop (`icon`, a tab's icon, a data-bound value),
/// so whatever names a glyph among them may be drawn. Over-inclusion costs a
/// few bytes; a miss paints genUI's missing-icon fallback.
pub fn icon_candidates(package: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    strings(package, &mut out);
    for shell in [eengine::contract::RUNNER_JSON, eengine::contract::CONSENT_JSON] {
        strings(&serde_json::from_str(shell).expect("the eengine fixtures parse"), &mut out);
    }
    out
}

fn strings(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => {
            out.insert(s.clone());
        }
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// The kit's `glyphs.json` (`{icons: {glyph: node}, alias: {name: glyph}}`)
/// cut to `names`: a name's glyph is `alias[name]` or the name itself.
pub fn prune_glyphs(kit: &Value, names: &BTreeSet<String>) -> Value {
    let icons = kit["icons"].as_object().cloned().unwrap_or_default();
    let alias = kit["alias"].as_object().cloned().unwrap_or_default();
    let (mut kept_icons, mut kept_alias) = (Map::new(), Map::new());
    for name in names {
        let aliased = alias.get(name).and_then(Value::as_str);
        let glyph = aliased.unwrap_or(name);
        if let Some(node) = icons.get(glyph) {
            kept_icons.insert(glyph.to_string(), node.clone());
            if let Some(to) = aliased {
                kept_alias.insert(name.clone(), Value::String(to.to_string()));
            }
        }
    }
    serde_json::json!({"icons": kept_icons, "alias": kept_alias})
}

/// The export's files, scope-relative (`./a/b`), sorted, without `sw.js`.
pub fn export_files(dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for path in walk(dir)? {
        let rel = path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
        if rel != SERVICE_WORKER {
            files.push(rel);
        }
    }
    files.sort();
    Ok(files)
}

/// `exp-runner-<16 hex>`: sha256 over every file's path and bytes, so a new
/// export (a new package version) is a new cache and the old one goes.
pub fn cache_name(dir: &Path, files: &[String]) -> Result<String> {
    let mut hash = Sha256::new();
    for f in files {
        hash.update(f.as_bytes());
        hash.update(std::fs::read(dir.join(f))?);
    }
    let hex: String = hash.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect();
    Ok(format!("exp-runner-{hex}"))
}

/// The kit's `sw.js` (apps/runner/web/src/sw.ts) with its `CACHE` and `FILES`
/// lines replaced: the precache list = `./` (the shell) + every file.
pub fn rewrite_service_worker(sw: &str, cache: &str, files: &[String]) -> Result<String> {
    let list: Vec<String> = std::iter::once("./".to_string()).chain(files.iter().map(|f| format!("./{f}"))).collect();
    let (mut cache_line, mut files_line) = (false, false);
    let lines: Vec<String> = sw
        .lines()
        .map(|l| {
            if l.starts_with("const CACHE = ") {
                cache_line = true;
                format!("const CACHE = {}", Value::String(cache.to_string()))
            } else if l.starts_with("const FILES = ") {
                files_line = true;
                format!("const FILES = {}", serde_json::to_string(&list).unwrap_or_default())
            } else {
                l.to_string()
            }
        })
        .collect();
    if !cache_line || !files_line {
        bail!("the kit's {SERVICE_WORKER} has no `const CACHE` / `const FILES` line (apps/runner/web/src/sw.ts)");
    }
    Ok(format!("{}\n", lines.join("\n")))
}

/// `index.html` with the app's name as its `<title>` (the runner also sets it at launch).
pub fn with_title(html: &str, name: &str) -> String {
    let (Some(start), Some(end)) = (html.find("<title>"), html.find("</title>")) else { return html.to_string() };
    let escaped = name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!("{}<title>{escaped}{}", &html[..start], &html[end..])
}

/// An ICO of PNG frames (`(px, png)`, px ≤ 256).
pub fn ico(frames: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * frames.len() as u32;
    for (px, png) in frames {
        let side = if *px >= 256 { 0 } else { *px as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in frames {
        out.extend_from_slice(png);
    }
    out
}

fn walk(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for path in walk(from)? {
        let dest = to.join(path.strip_prefix(from)?);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&path, &dest).with_context(|| format!("copy {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prune_keeps_only_named_glyphs_and_their_aliases() {
        let kit = json!({"icons": {"camera": [["circle", {"r": "3"}]], "image": [["rect", {}]], "x": [["path", {}]]}, "alias": {"ui-photos": "image", "edit": "x"}});
        let names: BTreeSet<String> = ["ui-photos", "camera", "Hello", "missing"].into_iter().map(String::from).collect();
        assert_eq!(prune_glyphs(&kit, &names), json!({"icons": {"camera": [["circle", {"r": "3"}]], "image": [["rect", {}]]}, "alias": {"ui-photos": "image"}}));
    }

    #[test]
    fn candidates_cover_the_package_and_the_shell() {
        let names = icon_candidates(&json!({"templates": {"home": {"root": {"component": "Button", "icon": "camera"}}}}));
        assert!(names.contains("camera"));
        // runner.json's shell draws these (the permissions entry, the failure screen).
        assert!(names.contains("ui-permission") && names.contains("ui-error"));
    }

    #[test]
    fn service_worker_lines_are_replaced() {
        let sw = "// gen\nconst CACHE = \"exp-runner-old\"\nconst FILES = [\"./\",\"./old.js\"]\nself.x()\n";
        let got = rewrite_service_worker(sw, "exp-runner-new", &["a.js".into(), "app/runner.json".into()]).unwrap();
        assert_eq!(got, "// gen\nconst CACHE = \"exp-runner-new\"\nconst FILES = [\"./\",\"./a.js\",\"./app/runner.json\"]\nself.x()\n");
        assert!(rewrite_service_worker("self.x()", "c", &[]).is_err());
    }

    #[test]
    fn cache_name_follows_every_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "1").unwrap();
        let files = export_files(dir.path()).unwrap();
        let before = cache_name(dir.path(), &files).unwrap();
        std::fs::write(dir.path().join("a"), "2").unwrap();
        let after = cache_name(dir.path(), &files).unwrap();
        assert!(before.starts_with("exp-runner-") && before.len() == "exp-runner-".len() + 16);
        assert_ne!(before, after);
    }

    #[test]
    fn title_is_escaped() {
        assert_eq!(with_title("<head><title>App</title></head>", "A & <B>"), "<head><title>A &amp; &lt;B&gt;</title></head>");
    }

    #[test]
    fn ico_header_and_directory() {
        let bytes = ico(&[(16, vec![1, 2]), (256, vec![3])]);
        assert_eq!(&bytes[..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(bytes[6], 16);
        assert_eq!(bytes[22], 0);
        assert_eq!(u32::from_le_bytes(bytes[18..22].try_into().unwrap()), 38);
        assert_eq!(&bytes[38..], &[1, 2, 3]);
    }
}
