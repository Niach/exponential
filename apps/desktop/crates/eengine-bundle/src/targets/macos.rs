//! `--target macos` (bundle.json `outputs.macos`, `permissions.macos`, `signing.macos`).
//!
//! `{name}.app` = the kit's `exponential-runner` as `Contents/MacOS/<name>`,
//! `Contents/Resources/app` (runner.json `bundle.locations.macos`), the
//! `.icns`, `NOTICES.txt` and an Info.plist carrying the config id, name,
//! version and the deep-link scheme; signed with the sandbox entitlements of
//! the declared scopes. Then `{name}-{version}.dmg` (hdiutil). With the
//! author's `[signing.macos]` keys the app and the dmg are signed with that
//! identity and the dmg is notarized and stapled; without them the app is
//! ad-hoc signed and the CLI says so.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};
use eengine::runner::RunnerConfig;
use serde_json::{Map, Value};

use super::{Job, Output};
use crate::manifest::{macos_entitlements, macos_entitlements_plist, scheme, xml_escape};

mod icns;

/// The kit's runner binary.
pub const KIT_BINARY: &str = "exponential-runner";
/// The .app's floor (release-eengine.yml builds the runner with this deployment target).
pub const MIN_SYSTEM_VERSION: &str = "13.0";
const ICON_FILE: &str = "AppIcon";

/// `[signing.macos]` of bundle.toml.
#[derive(Debug, PartialEq, Eq)]
pub struct Signing {
    /// `identity`: a Developer ID Application identity in the keychain (`codesign`), or
    /// with `p12_file` (+ `p12_password`) rcodesign's certificate (any host).
    pub identity: Option<String>,
    pub p12_file: Option<PathBuf>,
    pub p12_password: Option<String>,
    /// `notary_api_key`: rcodesign's App Store Connect API key JSON
    /// (`rcodesign encode-app-store-connect-api-key`: issuer_id, key_id, private_key).
    pub notary_api_key: Option<PathBuf>,
}

impl Signing {
    pub fn adhoc(&self) -> bool {
        self.identity.is_none() && self.p12_file.is_none()
    }
}

/// `[signing.macos]` → what to sign with; `None` = ad-hoc.
pub fn signing(table: Option<&Map<String, Value>>) -> Result<Signing> {
    let get = |k: &str| table.and_then(|t| t.get(k)).and_then(Value::as_str).filter(|s| !s.trim().is_empty()).map(String::from);
    let s = Signing {
        identity: get("identity"),
        p12_file: get("p12_file").map(PathBuf::from),
        p12_password: get("p12_password"),
        notary_api_key: get("notary_api_key").map(PathBuf::from),
    };
    if let Some(t) = table {
        for k in t.keys() {
            if !["identity", "p12_file", "p12_password", "notary_api_key"].contains(&k.as_str()) {
                bail!("[signing.macos] has an unknown key `{k}` (identity, p12_file, p12_password, notary_api_key)");
            }
        }
    }
    if s.adhoc() && s.notary_api_key.is_some() {
        bail!("[signing.macos] notary_api_key needs an identity (or p12_file): Apple notarizes only Developer ID signed code");
    }
    Ok(s)
}

/// The executable's name in `Contents/MacOS` (the app name without path separators).
pub fn executable(config: &RunnerConfig) -> String {
    config.name.replace(['/', ':'], "-")
}

/// `Contents/Info.plist`.
pub fn info_plist(config: &RunnerConfig) -> String {
    let s = |v: &str| format!("<string>{}</string>", xml_escape(v));
    let entries = [
        ("CFBundleDevelopmentRegion", s("en")),
        ("CFBundleDisplayName", s(&config.name)),
        ("CFBundleExecutable", s(&executable(config))),
        ("CFBundleIconFile", s(ICON_FILE)),
        ("CFBundleIdentifier", s(&config.id)),
        ("CFBundleInfoDictionaryVersion", s("6.0")),
        ("CFBundleName", s(&config.name)),
        ("CFBundlePackageType", s("APPL")),
        ("CFBundleShortVersionString", s(&config.version)),
        (
            "CFBundleURLTypes",
            format!(
                "<array>\n\t\t<dict>\n\t\t\t<key>CFBundleURLName</key>\n\t\t\t{}\n\t\t\t<key>CFBundleURLSchemes</key>\n\t\t\t<array>\n\t\t\t\t{}\n\t\t\t</array>\n\t\t</dict>\n\t</array>",
                s(&config.id),
                s(scheme(config))
            ),
        ),
        ("CFBundleVersion", s(&config.version)),
        ("LSMinimumSystemVersion", s(MIN_SYSTEM_VERSION)),
        ("NSHighResolutionCapable", "<true/>".to_string()),
    ];
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n");
    for (k, v) in entries {
        out.push_str(&format!("\t<key>{k}</key>\n\t{v}\n"));
    }
    out.push_str("</dict>\n</plist>\n");
    out
}

/// Write the unsigned `.app` at `app` (`<name>.app`).
pub fn write_app(job: &Job, app: &Path) -> Result<()> {
    let config = &job.prepared.config;
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&macos)?;
    std::fs::create_dir_all(&resources)?;
    let exe = macos.join(executable(config));
    std::fs::copy(job.kit.join(KIT_BINARY), &exe).with_context(|| format!("the kit has no {KIT_BINARY}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))?;
    }
    job.write_app_dir(&resources.join(eengine::runner::bundle_dir()))?;
    std::fs::copy(job.notices(), resources.join(crate::kit::notices_file()))?;
    std::fs::write(resources.join(format!("{ICON_FILE}.icns")), icns::render(job.icon)?)?;
    std::fs::write(contents.join("Info.plist"), info_plist(config))?;
    std::fs::write(contents.join("PkgInfo"), "APPL????")?;
    Ok(())
}

fn have(tool: &str) -> bool {
    Command::new("/usr/bin/which").arg(tool).output().map(|o| o.status.success()).unwrap_or(false)
}

fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd.output().with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !out.status.success() {
        bail!("{:?} failed: {}{}", cmd.get_program(), String::from_utf8_lossy(&out.stderr).trim(), String::from_utf8_lossy(&out.stdout).trim());
    }
    Ok(())
}

/// Sign `path` (the .app with `entitlements`, or the .dmg without).
fn sign(path: &Path, signing: &Signing, entitlements: Option<&Path>, work: &Path) -> Result<()> {
    if signing.p12_file.is_none() && have("codesign") {
        let mut cmd = Command::new("codesign");
        cmd.args(["--force", "--sign", signing.identity.as_deref().unwrap_or("-")]);
        if let Some(e) = entitlements {
            cmd.arg("--entitlements").arg(e).args(["--options", "runtime"]);
        }
        cmd.arg(if signing.adhoc() { "--timestamp=none" } else { "--timestamp" });
        return run(cmd.arg(path));
    }
    if !have("rcodesign") {
        bail!("signing needs codesign (macOS) or rcodesign (https://github.com/indygreg/apple-platform-rs)");
    }
    if signing.adhoc() && entitlements.is_none() {
        return Ok(()); // An ad-hoc dmg carries no signature.
    }
    let mut cmd = Command::new("rcodesign");
    cmd.arg("sign");
    if let Some(e) = entitlements {
        cmd.arg("--entitlements-xml-file").arg(e).args(["--code-signature-flags", "runtime"]);
    }
    if let Some(p12) = &signing.p12_file {
        cmd.arg("--p12-file").arg(p12);
        let pw = work.join("p12-password");
        std::fs::write(&pw, signing.p12_password.as_deref().unwrap_or_default())?;
        cmd.arg("--p12-password-file").arg(&pw);
    }
    let result = run(cmd.arg(path));
    let _ = std::fs::remove_file(work.join("p12-password"));
    result
}

/// Notarize and staple the dmg (rcodesign, else `xcrun notarytool` + `stapler`).
fn notarize(dmg: &Path, key: &Path, work: &Path) -> Result<()> {
    if have("rcodesign") {
        return run(Command::new("rcodesign").arg("notary-submit").arg("--api-key-file").arg(key).arg("--staple").arg(dmg));
    }
    let json: Value = serde_json::from_slice(&std::fs::read(key).with_context(|| format!("read {}", key.display()))?).context("notary_api_key is not rcodesign's API key JSON")?;
    let field = |k: &str| json[k].as_str().map(String::from).with_context(|| format!("notary_api_key has no `{k}`"));
    let (issuer, key_id, private_key) = (field("issuer_id")?, field("key_id")?, field("private_key")?);
    let p8 = work.join(format!("AuthKey_{key_id}.p8"));
    std::fs::write(&p8, private_key)?;
    let result = run(Command::new("xcrun").args(["notarytool", "submit"]).arg(dmg).arg("--key").arg(&p8).args(["--key-id", &key_id, "--issuer", &issuer, "--wait"]));
    let _ = std::fs::remove_file(&p8);
    result?;
    run(Command::new("xcrun").args(["stapler", "staple"]).arg(dmg))
}

fn staple_app(app: &Path) -> Result<()> {
    if have("rcodesign") {
        return run(Command::new("rcodesign").arg("staple").arg(app));
    }
    run(Command::new("xcrun").args(["stapler", "staple"]).arg(app))
}

fn dmg(staging: &Path, volname: &str, out: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink("/Applications", staging.join("Applications"))?;
    let mut last = None;
    // hdiutil fails now and then with "Resource busy": one retry.
    for _ in 0..2 {
        let result = run(Command::new("hdiutil").args(["create", "-volname", volname, "-fs", "HFS+", "-format", "UDZO", "-ov", "-srcfolder"]).arg(staging).arg(out));
        match result {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap())
}

pub fn bundle(job: &Job) -> Result<Output> {
    let config = &job.prepared.config;
    let signing = signing(job.signing("macos"))?;
    let patterns = crate::contract::strings("/outputs/macos");
    let (app_name, dmg_name) = (job.output_name(&patterns[0], ""), job.output_name(&patterns[1], ""));
    let mut output = Output::default();

    let staging = job.work.join("dmg");
    let app = staging.join(&app_name);
    write_app(job, &app)?;
    let entitlements = job.work.join("entitlements.plist");
    std::fs::write(&entitlements, macos_entitlements_plist(&macos_entitlements(&job.prepared.scopes, config)))?;
    sign(&app, &signing, Some(&entitlements), &job.work)?;

    let dmg_path = job.out.join(&dmg_name);
    let made_dmg = if have("hdiutil") {
        dmg(&staging, &config.name, &dmg_path)?;
        if !signing.adhoc() {
            sign(&dmg_path, &signing, None, &job.work)?;
        }
        true
    } else {
        output.notes.push(format!("{dmg_name} needs hdiutil (macOS): only {app_name} was written"));
        false
    };

    let out_app = job.out.join(&app_name);
    if out_app.exists() {
        std::fs::remove_dir_all(&out_app)?;
    }
    std::fs::rename(&app, &out_app)?;

    match (&signing.notary_api_key, made_dmg) {
        (Some(key), true) => {
            notarize(&dmg_path, key, &job.work)?;
            staple_app(&out_app)?;
            output.notes.push(format!("signed with {} and notarized", signing.identity.as_deref().unwrap_or("the p12 certificate")));
        }
        (Some(_), false) => output.notes.push("not notarized: notarization submits the .dmg".into()),
        (None, _) if signing.adhoc() => output.notes.push("signed ad-hoc: it runs on this Mac; on others Gatekeeper blocks it. Add [signing.macos] identity + notary_api_key to bundle.toml to sign and notarize".into()),
        (None, _) => output.notes.push("signed but not notarized (no [signing.macos] notary_api_key): Gatekeeper blocks it on other Macs".into()),
    }

    output.files.push(out_app);
    if made_dmg {
        output.files.push(dmg_path);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eengine::providers::NetworkPolicy;
    use serde_json::json;

    fn config() -> RunnerConfig {
        RunnerConfig { id: "at.acme.album".into(), name: "Album & Co".into(), version: "1.2.3".into(), network: NetworkPolicy::Off }
    }

    #[test]
    fn info_plist_carries_the_config_and_the_scheme() {
        let plist = info_plist(&config());
        for want in [
            "<key>CFBundleIdentifier</key>\n\t<string>at.acme.album</string>",
            "<key>CFBundleName</key>\n\t<string>Album &amp; Co</string>",
            "<key>CFBundleExecutable</key>\n\t<string>Album &amp; Co</string>",
            "<key>CFBundleShortVersionString</key>\n\t<string>1.2.3</string>",
            "<key>CFBundleVersion</key>\n\t<string>1.2.3</string>",
            "<key>CFBundleURLSchemes</key>\n\t\t\t<array>\n\t\t\t\t<string>at.acme.album</string>",
            "<key>LSMinimumSystemVersion</key>\n\t<string>13.0</string>",
            "<key>NSHighResolutionCapable</key>\n\t<true/>",
            "<key>CFBundleIconFile</key>\n\t<string>AppIcon</string>",
        ] {
            assert!(plist.contains(want), "missing {want:?} in\n{plist}");
        }
    }

    #[test]
    fn the_executable_has_no_path_separators() {
        let c = RunnerConfig { name: "A/B:C".into(), ..config() };
        assert_eq!(executable(&c), "A-B-C");
    }

    #[test]
    fn signing_reads_the_authors_keys() {
        assert!(signing(None).unwrap().adhoc());
        let t = json!({"identity": "Developer ID Application: Acme (TEAM)", "notary_api_key": "key.json"});
        let s = signing(t.as_object()).unwrap();
        assert_eq!(s.identity.as_deref(), Some("Developer ID Application: Acme (TEAM)"));
        assert_eq!(s.notary_api_key, Some(PathBuf::from("key.json")));
        assert!(!s.adhoc());
        // Notarizing ad-hoc code is refused up front, and so is a key we do not read.
        assert!(signing(json!({"notary_api_key": "key.json"}).as_object()).is_err());
        assert!(signing(json!({"identity": "x", "password": "y"}).as_object()).is_err());
    }

    /// The whole target on a Mac, with `/usr/bin/true` (or `EXP_BUNDLE_MACOS_RUNNER`) as the kit's runner:
    /// the .app layout, an ad-hoc signature carrying the sandbox entitlements, the .dmg.
    #[cfg(target_os = "macos")]
    #[test]
    fn bundles_a_signed_app_and_a_dmg() {
        use crate::icons::Icon;
        let tmp = tempfile::tempdir().unwrap();
        let kit = tmp.path().join("kit");
        std::fs::create_dir_all(&kit).unwrap();
        // `EXP_BUNDLE_MACOS_RUNNER` = a real runner (release-eengine.yml's smoke), else any Mach-O.
        let runner = std::env::var("EXP_BUNDLE_MACOS_RUNNER").unwrap_or_else(|_| "/usr/bin/true".into());
        std::fs::copy(runner, kit.join(KIT_BINARY)).unwrap();
        std::fs::write(kit.join("NOTICES.txt"), "notices").unwrap();
        // The web runner's sample: it declares photos.read.
        let file: &[u8] = include_bytes!("../../../../../runner/web/samples/album.app.json");
        let toml = json!({"id": "at.acme.album", "name": "Album", "version": "1.0.0", "network": "off"});
        let prepared = crate::prepare::prepare(file, &toml, true).unwrap();
        let icon = Icon::from_svg(br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" rx="14" fill="#2563eb"/></svg>"##).unwrap();
        let out = tmp.path().join("out");
        let work = out.join(".work/macos");
        std::fs::create_dir_all(&work).unwrap();
        let job = Job { prepared: &prepared, icon: &icon, kit, out: out.clone(), work };
        let result = bundle(&job).unwrap();
        let app = out.join("Album.app");
        assert_eq!(result.files, vec![app.clone(), out.join("Album-1.0.0.dmg")]);
        let res = app.join("Contents/Resources");
        assert_eq!(std::fs::read(res.join("app/package.app.json")).unwrap(), file.to_vec());
        assert!(res.join("app/runner.json").is_file());
        assert!(res.join("NOTICES.txt").is_file());
        assert_eq!(&std::fs::read(res.join("AppIcon.icns")).unwrap()[0..4], b"icns");
        assert!(app.join("Contents/MacOS/Album").is_file());
        assert!(out.join("Album-1.0.0.dmg").is_file());
        assert!(result.notes.iter().any(|n| n.contains("ad-hoc")));

        let verify = Command::new("codesign").args(["--verify", "--strict"]).arg(&app).output().unwrap();
        assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
        let ent = Command::new("codesign").args(["-d", "--entitlements", "-", "--xml"]).arg(&app).output().unwrap();
        let ent = String::from_utf8_lossy(&ent.stdout);
        for want in ["com.apple.security.app-sandbox", "com.apple.security.assets.pictures.read-only", "com.apple.security.files.user-selected.read-only"] {
            assert!(ent.contains(want), "missing {want} in {ent}");
        }
        assert!(!ent.contains("network.client"), "network off asks for no network: {ent}");
        let info = Command::new("codesign").args(["-dv"]).arg(&app).output().unwrap();
        let info = String::from_utf8_lossy(&info.stderr);
        assert!(info.contains("Identifier=at.acme.album") && info.contains("Signature=adhoc") && info.contains("runtime"), "{info}");
    }
}
