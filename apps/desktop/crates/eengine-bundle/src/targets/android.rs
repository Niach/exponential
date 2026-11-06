//! `--target android` (bundle.json `outputs.android`, `permissions.android`, `signing.android`).
//!
//! Nothing is compiled: the kit (`scripts/runner-kits/android.sh`) is the
//! template's release build taken apart, and this relinks it per app with the
//! Android SDK's build-tools. `aapt2 link` turns the template's compiled
//! resources + an overlay (the label, the launcher icons) into the binary
//! manifest and resource table, on the template's own resource ids
//! (`--stable-ids`) so the prebuilt dex keeps pointing at the right ones; the
//! manifest is the template's merged manifest with the config id in place of
//! the template id (applicationId, the deep-link scheme, the provider
//! authorities) and the package's `<uses-permission>` lines between the
//! `E7:PERMISSIONS` markers. The .apk = that + the kit's dex, native libs and
//! Java resources + `assets/app/` + `assets/NOTICES.txt`, zipaligned and
//! signed with apksigner; the .aab = the same module in proto format, built by
//! bundletool and signed with jarsigner.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::{Job, Output};
use crate::contract::strings;
use crate::manifest::{android_permissions, xml_escape};

/// The kit's files (scripts/runner-kits/android.sh).
pub const KIT_FILES: [&str; 5] = ["kit.json", "AndroidManifest.xml", "resources.zip", "resource-ids.txt", "payload.zip"];

/// The env vars the passwords reach apksigner / jarsigner through (never argv).
const STORE_PASS_ENV: &str = "EENGINE_BUNDLE_STORE_PASS";
const KEY_PASS_ENV: &str = "EENGINE_BUNDLE_KEY_PASS";

pub fn bundle(job: &Job) -> Result<Output> {
    let config = &job.prepared.config;
    check_application_id(&config.id)?;
    for f in KIT_FILES {
        if !job.kit.join(f).is_file() {
            bail!("the android runner kit at {} has no {f} (rebuild it with scripts/runner-kits.sh)", job.kit.display());
        }
    }
    let kit: serde_json::Value = serde_json::from_slice(&std::fs::read(job.kit.join("kit.json"))?).context("the kit's kit.json")?;
    let template_id = kit["templateId"].as_str().context("kit.json has no templateId")?;
    let compile_sdk = kit["compileSdk"].as_u64().context("kit.json has no compileSdk")?;
    let tools = Tools::locate(compile_sdk)?;
    let mut output = Output::default();

    // The manifest and the overlay: the label, the launcher icons.
    let template = std::fs::read_to_string(job.kit.join("AndroidManifest.xml"))?;
    let manifest = patch_manifest(&template, template_id, &config.id, &android_permissions(&job.prepared.scopes, config))?;
    let manifest_path = job.work.join("AndroidManifest.xml");
    std::fs::write(&manifest_path, manifest)?;
    let res = job.work.join("res");
    std::fs::create_dir_all(res.join("values"))?;
    std::fs::write(res.join("values/strings.xml"), label_xml(&config.name))?;
    let mut icons = vec![(String::from("mipmap"), largest_icon())];
    icons.extend(crate::icons::sizes("android").into_iter().map(|(density, px)| (format!("mipmap-{density}"), px)));
    for (dir, px) in icons {
        std::fs::create_dir_all(res.join(&dir))?;
        std::fs::write(res.join(&dir).join("ic_launcher.png"), job.icon.png(px)?)?;
    }
    let overlay = job.work.join("overlay.zip");
    tools.run(Command::new(&tools.aapt2).arg("compile").arg("--dir").arg(&res).arg("-o").arg(&overlay), "aapt2 compile")?;

    // aapt2 link, binary (the .apk) and proto (the .aab).
    // The kit's ids are on the template's resource package; the table is on the config id now.
    let ids = job.work.join("resource-ids.txt");
    std::fs::write(&ids, rename_ids(&std::fs::read_to_string(job.kit.join("resource-ids.txt"))?, template_id, &config.id))?;
    let version_code = version_code(&config.version)?;
    let link = |out: &Path, proto: bool| -> Result<()> {
        let mut cmd = Command::new(&tools.aapt2);
        cmd.arg("link").arg("-o").arg(out).arg("-I").arg(&tools.android_jar).arg("--manifest").arg(&manifest_path);
        cmd.arg("--stable-ids").arg(&ids).arg("--auto-add-overlay");
        cmd.args(["--version-code", &version_code.to_string(), "--version-name", &config.version, "--replace-version"]);
        if proto {
            cmd.arg("--proto-format");
        }
        cmd.arg(job.kit.join("resources.zip")).arg("-R").arg(&overlay);
        tools.run(&mut cmd, "aapt2 link")
    };
    let linked = job.work.join("linked.apk");
    let linked_proto = job.work.join("linked-proto.zip");
    link(&linked, false)?;
    link(&linked_proto, true)?;

    // assets/app/ (runner.json `bundle`) + the notices.
    let app = job.work.join("app");
    job.write_app_dir(&app)?;
    let mut assets = Vec::new();
    for file in [eengine::runner::bundle_package_file(), eengine::runner::bundle_config_file()] {
        assets.push((format!("assets/app/{file}"), std::fs::read(app.join(file))?));
    }
    assets.push((format!("assets/{}", crate::kit::notices_file()), std::fs::read(job.notices())?));

    let signer = Signer::from_config(job.signing("android"), &job.work, &tools)?;
    if let Signer::Debug { .. } = signer {
        output.notes.push(String::from("android: no [signing.android] keystore, signed with a throwaway debug key: the .apk and .aab are for testing only (Play refuses them)"));
    }
    let names = strings("/outputs/android");
    let [apk_name, aab_name] = [&names[0], &names[1]].map(|p| job.output_name(p, ""));

    // The .apk: zipalign (16 KiB pages for the .so), then apksigner.
    let unaligned = job.work.join("unaligned.apk");
    write_zip(&unaligned, |w, seen| {
        copy_raw(w, seen, &linked, |name| Some(name.to_string()))?;
        copy_raw(w, seen, &job.kit.join("payload.zip"), |name| Some(name.to_string()))?;
        add_files(w, seen, &assets)
    })?;
    let aligned = job.work.join("aligned.apk");
    let mut align = Command::new(&tools.zipalign);
    align.args(if tools.page_align_16k { vec!["-f", "-P", "16", "4"] } else { vec!["-f", "-p", "4"] }).arg(&unaligned).arg(&aligned);
    tools.run(&mut align, "zipalign")?;
    let apk = job.out.join(&apk_name);
    let mut sign = Command::new(&tools.apksigner);
    // v1-v3 (no .idsig beside it: v4 is only for incremental adb installs).
    sign.arg("sign").args(signer.apksigner_args()).args(["--v4-signing-enabled", "false"]).arg("--out").arg(&apk).arg(&aligned);
    tools.run(signer.env(&mut sign), "apksigner sign")?;
    output.files.push(apk);

    // The .aab: the base module (bundletool's layout), then jarsigner.
    let base = job.work.join("base.zip");
    write_zip(&base, |w, seen| {
        copy_raw(w, seen, &linked_proto, |name| Some(proto_module_path(name)))?;
        copy_raw(w, seen, &job.kit.join("payload.zip"), |name| Some(module_path(name)))?;
        add_files(w, seen, &assets)
    })?;
    let bundle_config = job.work.join("BundleConfig.json");
    std::fs::write(&bundle_config, r#"{"optimizations": {"uncompressNativeLibraries": {"enabled": true}}}"#)?;
    let aab = job.out.join(&aab_name);
    let _ = std::fs::remove_file(&aab);
    let mut build = tools.bundletool()?;
    build.arg("build-bundle").arg(format!("--modules={}", base.display())).arg(format!("--config={}", bundle_config.display())).arg(format!("--output={}", aab.display()));
    tools.run(&mut build, "bundletool build-bundle")?;
    let mut jarsign = Command::new(&tools.jarsigner);
    jarsign.args(signer.jarsigner_args()).arg(&aab).arg(signer.alias());
    tools.run(signer.env(&mut jarsign), "jarsigner")?;
    output.files.push(aab);
    Ok(output)
}

/// The largest launcher icon (the default-density `mipmap/` entry, replacing the template's placeholder).
fn largest_icon() -> u32 {
    crate::icons::sizes("android").into_iter().map(|(_, px)| px).max().unwrap_or(192)
}

/// An Android applicationId: two or more segments of `[A-Za-z][A-Za-z0-9_]*`.
pub fn check_application_id(id: &str) -> Result<()> {
    let segments: Vec<&str> = id.split('.').collect();
    let ok = segments.len() >= 2
        && segments.iter().all(|s| s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    if !ok {
        bail!("bundle.toml `id` `{id}` is not an Android application id (two or more dot-separated segments, each a letter then letters, digits or `_`)");
    }
    Ok(())
}

/// `versionCode` from the semver `version`: major × 1 000 000 + minor × 1 000 + patch (a pre-release suffix is ignored).
pub fn version_code(version: &str) -> Result<u32> {
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<u32> = core.split('.').map(str::parse).collect::<Result<_, _>>().with_context(|| format!("version `{version}` is not major.minor.patch"))?;
    let [major, minor, patch] = parts[..] else { bail!("version `{version}` is not major.minor.patch") };
    if minor > 999 || patch > 999 || major > 2099 {
        bail!("version `{version}` does not fit an Android versionCode (minor and patch up to 999, major up to 2099)");
    }
    Ok((major * 1_000_000 + minor * 1_000 + patch).max(1))
}

/// The kit's merged manifest for one app: the config id in place of the template id
/// (package, scheme, authorities, the receiver permission) and the permissions between the markers.
pub fn patch_manifest(template: &str, template_id: &str, id: &str, permissions: &[String]) -> Result<String> {
    let markers = strings("/permissions/android/markers");
    let (open, close) = (&markers[0], &markers[1]);
    let (Some(start), Some(end)) = (template.find(open.as_str()), template.find(close.as_str())) else {
        bail!("the kit's manifest has no {open} … {close} markers");
    };
    let start = start + open.len();
    if end < start {
        bail!("the kit's manifest has its E7:PERMISSIONS markers out of order");
    }
    let indent: String = template[..start - open.len()].rsplit('\n').next().unwrap_or_default().chars().take_while(|c| c.is_whitespace()).collect();
    let mut between = String::from("\n");
    for line in permissions {
        between.push_str(&format!("{indent}{line}\n"));
    }
    between.push_str(&indent);
    let patched = format!("{}{between}{}", &template[..start], &template[end..]);
    if template_id.is_empty() || !patched.contains(&format!("package=\"{template_id}\"")) {
        bail!("the kit's manifest is not on its template id `{template_id}`");
    }
    Ok(patched.replace(template_id, id))
}

/// The kit's `--stable-ids` lines (`<package>:<type>/<name> = 0x…`) moved onto the app's package.
pub fn rename_ids(ids: &str, template_id: &str, id: &str) -> String {
    let prefix = format!("{template_id}:");
    ids.lines().map(|l| l.strip_prefix(&prefix).map(|rest| format!("{id}:{rest}")).unwrap_or_else(|| l.to_string()) + "\n").collect()
}

/// `res/values/strings.xml`: `runner_label` = the app name, escaped for aapt2.
pub fn label_xml(name: &str) -> String {
    let mut s = String::new();
    for (i, c) in name.chars().enumerate() {
        match c {
            '\\' => s.push_str("\\\\"),
            '\'' => s.push_str("\\'"),
            '"' => s.push_str("\\\""),
            '\n' => s.push_str("\\n"),
            '@' | '?' if i == 0 => {
                s.push('\\');
                s.push(c);
            }
            c => s.push(c),
        }
    }
    format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"runner_label\">{}</string>\n</resources>\n", xml_escape(&s).replace("&apos;", "'").replace("&quot;", "\""))
}

/// An APK entry of the kit's payload → its place in the bundle's base module.
pub fn module_path(entry: &str) -> String {
    let dex = !entry.contains('/') && entry.starts_with("classes") && entry.ends_with(".dex");
    if dex {
        format!("dex/{entry}")
    } else if entry.starts_with("lib/") || entry.starts_with("assets/") {
        entry.to_string()
    } else {
        format!("root/{entry}")
    }
}

/// An entry of aapt2's proto link → its place in the base module.
pub fn proto_module_path(entry: &str) -> String {
    if entry == "AndroidManifest.xml" { format!("manifest/{entry}") } else { entry.to_string() }
}

fn write_zip(path: &Path, fill: impl FnOnce(&mut ZipWriter<std::fs::File>, &mut HashSet<String>) -> Result<()>) -> Result<()> {
    let mut w = ZipWriter::new(std::fs::File::create(path)?);
    let mut seen = HashSet::new();
    fill(&mut w, &mut seen)?;
    w.finish()?;
    Ok(())
}

fn claim(seen: &mut HashSet<String>, name: &str) -> Result<()> {
    if !seen.insert(name.to_string()) {
        bail!("`{name}` is in the android kit twice");
    }
    Ok(())
}

/// Copy every file entry of `from` as is (its compression kept: the .so and resources.arsc stay stored).
fn copy_raw(w: &mut ZipWriter<std::fs::File>, seen: &mut HashSet<String>, from: &Path, rename: impl Fn(&str) -> Option<String>) -> Result<()> {
    let mut archive = ZipArchive::new(std::fs::File::open(from)?).with_context(|| format!("read {}", from.display()))?;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = rename(&entry.name()?) else { continue };
        claim(seen, &name)?;
        w.raw_copy_file_rename(entry, name)?;
    }
    Ok(())
}

fn add_files(w: &mut ZipWriter<std::fs::File>, seen: &mut HashSet<String>, files: &[(String, Vec<u8>)]) -> Result<()> {
    for (name, bytes) in files {
        claim(seen, name)?;
        w.start_file(name.as_str(), SimpleFileOptions::default().compression_method(CompressionMethod::Deflated))?;
        w.write_all(bytes)?;
    }
    Ok(())
}

/// The Android SDK build-tools + a JDK + bundletool: the author's prerequisites.
struct Tools {
    aapt2: PathBuf,
    zipalign: PathBuf,
    apksigner: PathBuf,
    /// build-tools 35+ zipalign takes `-P 16` (16 KiB pages for the .so).
    page_align_16k: bool,
    android_jar: PathBuf,
    java: PathBuf,
    keytool: PathBuf,
    jarsigner: PathBuf,
    /// The JDK's bin dir, put first on the tools' PATH (apksigner runs `java`).
    jdk_bin: Option<PathBuf>,
}

impl Tools {
    fn locate(compile_sdk: u64) -> Result<Self> {
        let sdk = ["ANDROID_HOME", "ANDROID_SDK_ROOT"].iter().find_map(|v| std::env::var_os(v)).map(PathBuf::from).context("--target android needs the Android SDK: set ANDROID_HOME (sdkmanager \"build-tools;35.0.0\" \"platforms;android-36\")")?;
        let mut versions: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(sdk.join("build-tools"))
            .with_context(|| format!("no build-tools under {}", sdk.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| ["aapt2", "zipalign", "apksigner"].iter().all(|t| p.join(t).is_file()))
            .map(|p| (p.file_name().and_then(|n| n.to_str()).unwrap_or_default().split(['.', '-']).filter_map(|n| n.parse().ok()).collect(), p))
            .collect();
        versions.sort();
        let Some((version, bt)) = versions.pop() else { bail!("no build-tools with aapt2, zipalign and apksigner under {}/build-tools", sdk.display()) };
        let android_jar = sdk.join(format!("platforms/android-{compile_sdk}/android.jar"));
        if !android_jar.is_file() {
            bail!("--target android needs {} (sdkmanager \"platforms;android-{compile_sdk}\")", android_jar.display());
        }
        let jdk_bin = std::env::var_os("JAVA_HOME").map(|h| PathBuf::from(h).join("bin")).filter(|b| b.join("java").is_file());
        let jdk_tool = |name: &str| -> Result<PathBuf> {
            jdk_bin.as_ref().map(|b| b.join(name)).filter(|p| p.is_file()).or_else(|| on_path(name)).with_context(|| format!("--target android needs a JDK ({name}): set JAVA_HOME"))
        };
        Ok(Self {
            aapt2: bt.join("aapt2"),
            zipalign: bt.join("zipalign"),
            apksigner: bt.join("apksigner"),
            page_align_16k: version.first().is_some_and(|m| *m >= 35),
            android_jar,
            java: jdk_tool("java")?,
            keytool: jdk_tool("keytool")?,
            jarsigner: jdk_tool("jarsigner")?,
            jdk_bin,
        })
    }

    /// bundletool: `BUNDLETOOL` (the jar, or an executable), else `bundletool` on PATH.
    fn bundletool(&self) -> Result<Command> {
        let found = std::env::var_os("BUNDLETOOL").map(PathBuf::from).or_else(|| on_path("bundletool"));
        let Some(tool) = found else {
            bail!("the .aab needs bundletool: set BUNDLETOOL to bundletool-all-<version>.jar (github.com/google/bundletool/releases)");
        };
        Ok(if tool.extension().is_some_and(|e| e == "jar") {
            let mut c = Command::new(&self.java);
            c.arg("-jar").arg(tool);
            c
        } else {
            Command::new(tool)
        })
    }

    fn run(&self, cmd: &mut Command, what: &str) -> Result<()> {
        if let Some(bin) = &self.jdk_bin {
            let mut path = OsString::from(bin);
            if let Some(p) = std::env::var_os("PATH") {
                path.push(":");
                path.push(p);
            }
            cmd.env("PATH", path);
        }
        let out = cmd.output().with_context(|| format!("run {what}"))?;
        if !out.status.success() {
            bail!("{what} failed:\n{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
}

/// `[signing.android]` (the author's keystore), else a throwaway debug key.
#[derive(Debug, PartialEq)]
pub enum Signer {
    Author { keystore: PathBuf, alias: String, store_password: String, key_password: String },
    Debug { keystore: PathBuf },
}

const DEBUG_ALIAS: &str = "androiddebugkey";
const DEBUG_PASSWORD: &str = "android";

impl Signer {
    /// `keystore` (a path, relative = to the working directory), `alias`, `store-password`, `key-password`
    /// (default = the store password); `EXPONENTIAL_ANDROID_STORE_PASSWORD` / `EXPONENTIAL_ANDROID_KEY_PASSWORD` override the file.
    pub fn read(table: Option<&serde_json::Map<String, serde_json::Value>>, env: impl Fn(&str) -> Option<String>) -> Result<Option<Self>> {
        let get = |k: &str| table.and_then(|t| t.get(k)).and_then(serde_json::Value::as_str).map(String::from);
        let Some(keystore) = get("keystore") else { return Ok(None) };
        let alias = get("alias").context("[signing.android] has a keystore but no alias")?;
        let store_password = env("EXPONENTIAL_ANDROID_STORE_PASSWORD").or_else(|| get("store-password")).context("[signing.android]: no store-password (or EXPONENTIAL_ANDROID_STORE_PASSWORD)")?;
        let key_password = env("EXPONENTIAL_ANDROID_KEY_PASSWORD").or_else(|| get("key-password")).unwrap_or_else(|| store_password.clone());
        Ok(Some(Self::Author { keystore: PathBuf::from(keystore), alias, store_password, key_password }))
    }

    fn from_config(table: Option<&serde_json::Map<String, serde_json::Value>>, work: &Path, tools: &Tools) -> Result<Self> {
        if let Some(author) = Self::read(table, |k| std::env::var(k).ok())? {
            if let Self::Author { keystore, .. } = &author {
                if !keystore.is_file() {
                    bail!("[signing.android] keystore {} does not exist", keystore.display());
                }
            }
            return Ok(author);
        }
        let keystore = work.join("debug.p12");
        let mut gen = Command::new(&tools.keytool);
        gen.args(["-genkeypair", "-noprompt", "-storetype", "PKCS12", "-keystore"]).arg(&keystore);
        gen.args(["-storepass", DEBUG_PASSWORD, "-keypass", DEBUG_PASSWORD, "-alias", DEBUG_ALIAS, "-keyalg", "RSA", "-keysize", "2048", "-validity", "10000", "-dname", "CN=Android Debug,O=Android,C=US"]);
        tools.run(&mut gen, "keytool -genkeypair (the debug key)")?;
        Ok(Self::Debug { keystore })
    }

    fn keystore(&self) -> &Path {
        match self {
            Self::Author { keystore, .. } | Self::Debug { keystore } => keystore,
        }
    }

    fn alias(&self) -> &str {
        match self {
            Self::Author { alias, .. } => alias,
            Self::Debug { .. } => DEBUG_ALIAS,
        }
    }

    fn env<'c>(&self, cmd: &'c mut Command) -> &'c mut Command {
        let (store, key) = match self {
            Self::Author { store_password, key_password, .. } => (store_password.as_str(), key_password.as_str()),
            Self::Debug { .. } => (DEBUG_PASSWORD, DEBUG_PASSWORD),
        };
        cmd.env(STORE_PASS_ENV, store).env(KEY_PASS_ENV, key)
    }

    fn apksigner_args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["--ks".into(), self.keystore().into(), "--ks-key-alias".into(), self.alias().into()];
        a.extend(["--ks-pass", &format!("env:{STORE_PASS_ENV}"), "--key-pass", &format!("env:{KEY_PASS_ENV}")].map(OsString::from));
        a
    }

    fn jarsigner_args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["-keystore".into(), self.keystore().into()];
        a.extend(["-storepass:env", STORE_PASS_ENV, "-keypass:env", KEY_PASS_ENV].map(OsString::from));
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_codes() {
        assert_eq!(version_code("1.0.0").unwrap(), 1_000_000);
        assert_eq!(version_code("2.13.7").unwrap(), 2_013_007);
        assert_eq!(version_code("1.2.3-beta.1").unwrap(), 1_002_003);
        assert_eq!(version_code("0.0.0").unwrap(), 1);
        assert!(version_code("1.2").is_err());
        assert!(version_code("1.1000.0").is_err());
        assert!(version_code("x.y.z").is_err());
    }

    #[test]
    fn application_ids() {
        assert!(check_application_id("at.acme.photobook").is_ok());
        assert!(check_application_id("a.b_2").is_ok());
        for bad in ["photobook", "at.acme.", "at.1acme", "at.acme-book", ""] {
            assert!(check_application_id(bad).is_err(), "{bad}");
        }
    }

    const TEMPLATE: &str = "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\" package=\"at.exponential.runner.template\">\n    <!-- E7:PERMISSIONS -->\n    <!-- /E7:PERMISSIONS -->\n    <provider android:authorities=\"at.exponential.runner.template.share\" />\n    <data android:scheme=\"at.exponential.runner.template\" />\n</manifest>\n";

    #[test]
    fn the_manifest_gets_the_id_and_only_the_declared_permissions() {
        let lines = vec![String::from("<uses-permission android:name=\"android.permission.INTERNET\" />")];
        let out = patch_manifest(TEMPLATE, "at.exponential.runner.template", "at.acme.photobook", &lines).unwrap();
        assert!(out.contains("package=\"at.acme.photobook\""));
        assert!(out.contains("android:authorities=\"at.acme.photobook.share\""));
        assert!(out.contains("android:scheme=\"at.acme.photobook\""));
        assert!(!out.contains("template"));
        assert!(out.contains("    <!-- E7:PERMISSIONS -->\n    <uses-permission android:name=\"android.permission.INTERNET\" />\n    <!-- /E7:PERMISSIONS -->"));
        let none = patch_manifest(TEMPLATE, "at.exponential.runner.template", "at.acme.photobook", &[]).unwrap();
        assert!(!none.contains("uses-permission"));
        // Re-patching what is between the markers replaces it.
        let again = patch_manifest(&out.replace("at.acme.photobook", "at.exponential.runner.template"), "at.exponential.runner.template", "at.acme.photobook", &[]).unwrap();
        assert_eq!(again, none);
    }

    #[test]
    fn a_manifest_without_markers_or_on_another_id_is_refused() {
        assert!(patch_manifest("<manifest package=\"a.b\"/>", "a.b", "c.d", &[]).is_err());
        assert!(patch_manifest(TEMPLATE, "x.y", "c.d", &[]).is_err());
    }

    #[test]
    fn the_stable_ids_move_to_the_apps_package() {
        let ids = "at.exponential.runner.template:mipmap/ic_launcher = 0x7f0a0000\nat.exponential.runner.template:string/runner_label = 0x7f0b0001\n";
        assert_eq!(rename_ids(ids, "at.exponential.runner.template", "at.acme.album"), "at.acme.album:mipmap/ic_launcher = 0x7f0a0000\nat.acme.album:string/runner_label = 0x7f0b0001\n");
    }

    #[test]
    fn the_label_is_escaped_for_aapt2() {
        assert!(label_xml("Photobook").contains(">Photobook</string>"));
        assert!(label_xml("Ann's <Book> & \"co\"").contains(">Ann\\'s &lt;Book&gt; &amp; \\\"co\\\"</string>"));
        assert!(label_xml("@home").contains(">\\@home</string>"));
        assert!(label_xml("a@b").contains(">a@b</string>"));
    }

    #[test]
    fn payload_entries_land_in_the_base_module() {
        assert_eq!(module_path("classes.dex"), "dex/classes.dex");
        assert_eq!(module_path("classes2.dex"), "dex/classes2.dex");
        assert_eq!(module_path("lib/arm64-v8a/libeengine_ffi.so"), "lib/arm64-v8a/libeengine_ffi.so");
        assert_eq!(module_path("assets/dexopt/baseline.prof"), "assets/dexopt/baseline.prof");
        assert_eq!(module_path("META-INF/services/x"), "root/META-INF/services/x");
        assert_eq!(module_path("kotlin/kotlin.kotlin_builtins"), "root/kotlin/kotlin.kotlin_builtins");
        assert_eq!(proto_module_path("AndroidManifest.xml"), "manifest/AndroidManifest.xml");
        assert_eq!(proto_module_path("resources.pb"), "resources.pb");
        assert_eq!(proto_module_path("res/mipmap-mdpi-v4/ic_launcher.png"), "res/mipmap-mdpi-v4/ic_launcher.png");
    }

    #[test]
    fn signing_reads_the_authors_keystore_else_none() {
        let none = |_: &str| None;
        assert_eq!(Signer::read(None, none).unwrap(), None);
        let t: serde_json::Map<String, serde_json::Value> = serde_json::from_str(r#"{"keystore": "release.jks", "alias": "upload", "store-password": "s"}"#).unwrap();
        assert_eq!(
            Signer::read(Some(&t), none).unwrap(),
            Some(Signer::Author { keystore: "release.jks".into(), alias: "upload".into(), store_password: "s".into(), key_password: "s".into() })
        );
        let env = |k: &str| (k == "EXPONENTIAL_ANDROID_KEY_PASSWORD").then(|| String::from("k"));
        assert!(matches!(Signer::read(Some(&t), env).unwrap(), Some(Signer::Author { key_password, .. }) if key_password == "k"));
        let no_alias: serde_json::Map<String, serde_json::Value> = serde_json::from_str(r#"{"keystore": "release.jks", "store-password": "s"}"#).unwrap();
        assert!(Signer::read(Some(&no_alias), none).is_err());
        let no_pass: serde_json::Map<String, serde_json::Value> = serde_json::from_str(r#"{"keystore": "release.jks", "alias": "upload"}"#).unwrap();
        assert!(Signer::read(Some(&no_pass), none).is_err());
    }
}
