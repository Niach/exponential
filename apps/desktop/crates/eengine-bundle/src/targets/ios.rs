//! `--target ios` (bundle.json `outputs.ios`, `permissions.ios`, `signing.ios`).
//!
//! The ONE target compiled per app (ITMS-90683: a linked privacy API needs its
//! purpose string, so only the declared capability modules are linked). The kit
//! is the runner template (`scripts/runner-kits/ios.sh`); its
//! `apps/runner/ios/make-project.sh` gets the modules and purpose strings from
//! `manifest` (the one mapping), the 1024 AppIcon, the notices and `app/`, and
//! builds for the device (`aarch64-apple-ios`). The `.ipa` is `Payload/<name>.app`
//! zipped: unsigned without a team (for the author to re-sign), else
//! `xcodebuild archive` + `-exportArchive` for App Store Connect (TestFlight).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context as _, Result};
use serde_json::{Map, Value};

use super::{Job, Output};
use crate::contract::BUNDLE;
use crate::icons::Icon;
use crate::manifest::{ios_modules, ios_purposes, xml_escape};

/// The template's generator, at its workspace path inside the kit.
pub const MAKE_PROJECT: &str = "apps/runner/ios/make-project.sh";

/// Set to `1` to also write `{name}-{version}-simulator.app` (an unsigned
/// iphonesimulator build of the same project): a development check, never shipped.
pub const SIMULATOR_ENV: &str = "EENGINE_BUNDLE_IOS_SIMULATOR";

/// `[signing.ios]` (or `DEVELOPMENT_TEAM` in the environment): the author's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signing {
    /// The Apple team id; without one the .ipa is unsigned.
    pub team: String,
    /// An installed App Store provisioning profile (name or UUID): manual signing.
    /// Absent = automatic signing (`-allowProvisioningUpdates`).
    pub profile: Option<String>,
    /// The distribution certificate in the keychain (manual signing), default "Apple Distribution".
    pub identity: Option<String>,
    /// An App Store Connect API key for automatic signing without an Xcode account:
    /// `api_key_path` (.p8), `api_key_id`, `api_issuer_id`.
    pub api_key: Option<(String, String, String)>,
}

impl Signing {
    pub fn from_table(table: Option<&Map<String, Value>>, env_team: Option<String>) -> Result<Option<Self>> {
        let get = |k: &str| table.and_then(|t| t.get(k)).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        let Some(team) = get("team").or(env_team.filter(|t| !t.trim().is_empty())) else {
            return Ok(None);
        };
        let api_key = match (get("api_key_path"), get("api_key_id"), get("api_issuer_id")) {
            (Some(p), Some(k), Some(i)) => Some((p, k, i)),
            (None, None, None) => None,
            _ => bail!("[signing.ios]: api_key_path, api_key_id and api_issuer_id go together"),
        };
        Ok(Some(Self { team, profile: get("profile"), identity: get("identity"), api_key }))
    }

    /// make-project.sh's signing arguments (written onto the app target only).
    pub fn project_args(&self) -> Vec<String> {
        let mut args = vec!["--team".into(), self.team.clone()];
        if let Some(p) = &self.profile {
            args.extend(["--profile".into(), p.clone()]);
            args.extend(["--identity".into(), self.identity.clone().unwrap_or_else(|| "Apple Distribution".into())]);
        }
        args
    }

    /// The xcodebuild flags of automatic signing (none for manual).
    pub fn xcodebuild_auth(&self) -> Vec<String> {
        if self.profile.is_some() {
            return Vec::new();
        }
        let mut args = vec!["-allowProvisioningUpdates".to_string()];
        if let Some((path, id, issuer)) = &self.api_key {
            args.extend(["-authenticationKeyPath".into(), path.clone(), "-authenticationKeyID".into(), id.clone(), "-authenticationKeyIssuerID".into(), issuer.clone()]);
        }
        args
    }

    /// `-exportOptionsPlist`: App Store Connect (TestFlight-ready), no symbols uploaded.
    pub fn export_options(&self, bundle_id: &str) -> String {
        let mut d = format!("\t<key>method</key>\n\t<string>app-store-connect</string>\n\t<key>destination</key>\n\t<string>export</string>\n\t<key>teamID</key>\n\t<string>{}</string>\n\t<key>uploadSymbols</key>\n\t<false/>\n", xml_escape(&self.team));
        match &self.profile {
            Some(p) => d.push_str(&format!(
                "\t<key>signingStyle</key>\n\t<string>manual</string>\n\t<key>signingCertificate</key>\n\t<string>{}</string>\n\t<key>provisioningProfiles</key>\n\t<dict>\n\t\t<key>{}</key>\n\t\t<string>{}</string>\n\t</dict>\n",
                xml_escape(self.identity.as_deref().unwrap_or("Apple Distribution")), xml_escape(bundle_id), xml_escape(p)
            )),
            None => d.push_str("\t<key>signingStyle</key>\n\t<string>automatic</string>\n"),
        }
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n{d}</dict>\n</plist>\n")
    }
}

/// The bundle id make-project.sh writes: the config id with `_` → `-` (the URL scheme keeps the id).
pub fn bundle_id(config_id: &str) -> String {
    config_id.replace('_', "-")
}

/// The App Store icon: 1024 px, opaque (an alpha channel is refused), the
/// SVG composed over the runner's background (`permissions.web.color`).
pub fn app_icon_png(icon: &Icon) -> Result<Vec<u8>> {
    let size = crate::icons::sizes("ios").first().map(|(_, n)| *n).unwrap_or(1024);
    let bg = background();
    let rgba = icon.rgba(size)?;
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| {
        let a = p[3] as u32;
        [0, 1, 2].map(|i| ((p[i] as u32 * a + bg[i] as u32 * (255 - a) + 127) / 255) as u8)
    }).collect();
    let mut out = Vec::new();
    image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut out), &rgb, size, size, image::ExtendedColorType::Rgb8).context("encode the AppIcon PNG")?;
    Ok(out)
}

fn background() -> [u8; 3] {
    let hex = BUNDLE["permissions"]["web"]["color"].as_str().unwrap_or("#000000").trim_start_matches('#');
    let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
    [byte(0), byte(2), byte(4)]
}

/// make-project.sh's arguments for one build (`build` = none, simulator or device).
pub fn make_project_args(inputs: &Inputs, out: &Path, build: &str, signing: Option<&Signing>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--bundle".into(), inputs.app.display().to_string(),
        "--out".into(), out.display().to_string(),
        "--modules".into(), inputs.modules.join(","),
        "--purposes".into(), inputs.purposes.display().to_string(),
        "--icon".into(), inputs.icon.display().to_string(),
        "--notices".into(), inputs.notices.display().to_string(),
    ];
    if let Some(s) = signing {
        args.extend(s.project_args());
    }
    args.extend(["--build".into(), build.into()]);
    args
}

/// What make-project.sh is given, written under the job's scratch dir.
pub struct Inputs {
    pub app: PathBuf,
    pub modules: Vec<String>,
    pub purposes: PathBuf,
    pub icon: PathBuf,
    pub notices: PathBuf,
}

pub fn bundle(job: &Job) -> Result<Output> {
    if !cfg!(target_os = "macos") {
        bail!("--target ios builds on a Mac (Xcode 26+, xcodegen): it is the one target compiled per app");
    }
    let script = job.kit.join(MAKE_PROJECT);
    if !script.is_file() {
        bail!("the ios runner kit at {} has no {MAKE_PROJECT} (scripts/runner-kits.sh --out <dir> ios)", job.kit.display());
    }
    let p = job.prepared;
    let inputs = Inputs {
        app: job.work.join("app"),
        modules: ios_modules(&p.scopes),
        purposes: job.work.join("purposes.json"),
        icon: job.work.join("AppIcon.png"),
        notices: job.notices(),
    };
    job.write_app_dir(&inputs.app)?;
    let purposes: Map<String, Value> = ios_purposes(&p.scopes, &p.config, &p.whys).into_iter().map(|(k, v)| (k, Value::String(v))).collect();
    std::fs::write(&inputs.purposes, serde_json::to_vec_pretty(&purposes)?)?;
    std::fs::write(&inputs.icon, app_icon_png(job.icon)?)?;
    let signing = Signing::from_table(job.signing("ios"), std::env::var("DEVELOPMENT_TEAM").ok())?;

    let ipa = job.out.join(job.output_name("{name}-{version}.ipa", "arm64"));
    let _ = std::fs::remove_file(&ipa);
    let mut out = Output::default();
    let modules = if inputs.modules.is_empty() { "none".to_string() } else { inputs.modules.join(", ") };
    match &signing {
        None => {
            let lines = make_project(&script, &make_project_args(&inputs, &job.work.join("device"), "device", None))?;
            let app = PathBuf::from(lines.last().ok_or_else(|| anyhow!("make-project.sh printed no .app"))?);
            package_ipa(&app, &ipa, &job.work)?;
            out.notes.push(format!("{}: unsigned (no [signing.ios] team): re-sign it with your distribution identity before installing or uploading", file_name(&ipa)));
        }
        Some(s) => {
            let lines = make_project(&script, &make_project_args(&inputs, &job.work.join("device"), "none", Some(s)))?;
            let project = lines.iter().rev().find_map(|l| l.strip_prefix("project ")).ok_or_else(|| anyhow!("make-project.sh printed no project"))?;
            archive_and_export(Path::new(project), s, &bundle_id(&p.config.id), &ipa, &job.work)?;
            out.notes.push(format!("{}: signed for App Store Connect (team {}), ready for TestFlight", file_name(&ipa), s.team));
        }
    }
    out.notes.push(format!("linked capability modules: {modules}"));
    out.files.push(ipa);

    if std::env::var(SIMULATOR_ENV).as_deref() == Ok("1") {
        let lines = make_project(&script, &make_project_args(&inputs, &job.work.join("simulator"), "simulator", None))?;
        let app = PathBuf::from(lines.last().ok_or_else(|| anyhow!("make-project.sh printed no .app"))?);
        let dest = job.out.join(job.output_name("{name}-{version}-simulator.app", "arm64"));
        let _ = std::fs::remove_dir_all(&dest);
        run(Command::new("ditto").arg(&app).arg(&dest))?;
        out.notes.push(format!("{}: a simulator build ({SIMULATOR_ENV}), a development check, never shipped", file_name(&dest)));
        out.files.push(dest);
    }
    Ok(out)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Run make-project.sh; its stdout lines (the last = the .app it built).
fn make_project(script: &Path, args: &[String]) -> Result<Vec<String>> {
    // The unsigned path never picks up a team from the lane's environment.
    let output = Command::new("bash").arg(script).args(args).env_remove("RUNNER_DEVELOPMENT_TEAM").stderr(std::process::Stdio::inherit()).output().context("run make-project.sh")?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        bail!("make-project.sh failed ({}):\n{stdout}", output.status);
    }
    Ok(stdout.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status().with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{:?} failed ({status})", cmd.get_program());
    }
    Ok(())
}

/// `Payload/<name>.app`, zipped (ditto keeps the bundle's symlinks and modes).
pub fn package_ipa(app: &Path, ipa: &Path, work: &Path) -> Result<()> {
    if !app.is_dir() {
        bail!("no {}", app.display());
    }
    let payload = work.join("ipa").join("Payload");
    let _ = std::fs::remove_dir_all(&payload);
    std::fs::create_dir_all(&payload)?;
    run(Command::new("ditto").arg(app).arg(payload.join(app.file_name().ok_or_else(|| anyhow!("no .app name"))?)))?;
    run(Command::new("ditto").args(["-c", "-k", "--norsrc", "--keepParent"]).arg(&payload).arg(ipa))
}

fn archive_and_export(project: &Path, signing: &Signing, bundle_id: &str, ipa: &Path, work: &Path) -> Result<()> {
    let scheme = project.file_stem().ok_or_else(|| anyhow!("no scheme"))?.to_string_lossy().into_owned();
    let archive = work.join(format!("{scheme}.xcarchive"));
    let auth = signing.xcodebuild_auth();
    run(Command::new("xcodebuild").arg("-project").arg(project).args(["-scheme", &scheme, "-configuration", "Release", "-destination", "generic/platform=iOS"])
        .arg("-derivedDataPath").arg(work.join("DerivedData")).arg("-archivePath").arg(&archive).args(&auth).args(["archive", "-quiet"]))?;
    let options = work.join("ExportOptions.plist");
    std::fs::write(&options, signing.export_options(bundle_id))?;
    let export = work.join("export");
    run(Command::new("xcodebuild").arg("-exportArchive").arg("-archivePath").arg(&archive).arg("-exportPath").arg(&export).arg("-exportOptionsPlist").arg(&options).args(&auth))?;
    let built = std::fs::read_dir(&export)?.filter_map(|e| e.ok().map(|e| e.path())).find(|p| p.extension().is_some_and(|e| e == "ipa")).ok_or_else(|| anyhow!("-exportArchive wrote no .ipa"))?;
    std::fs::rename(&built, ipa).or_else(|_| std::fs::copy(&built, ipa).map(|_| ()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn table(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn no_team_is_unsigned() {
        assert_eq!(Signing::from_table(None, None).unwrap(), None);
        assert_eq!(Signing::from_table(Some(&table(json!({"profile": "P"}))), Some(" ".into())).unwrap(), None);
    }

    #[test]
    fn the_environment_team_signs_automatically() {
        let s = Signing::from_table(None, Some("ABCDE12345".into())).unwrap().unwrap();
        assert_eq!(s.project_args(), ["--team", "ABCDE12345"]);
        assert_eq!(s.xcodebuild_auth(), ["-allowProvisioningUpdates"]);
        let plist = s.export_options("com.example.album");
        assert!(plist.contains("<string>app-store-connect</string>") && plist.contains("<string>automatic</string>") && plist.contains("<string>ABCDE12345</string>"));
        assert!(!plist.contains("provisioningProfiles"));
    }

    #[test]
    fn a_profile_signs_manually_with_the_distribution_identity() {
        let t = table(json!({"team": "T1", "profile": "Album App Store"}));
        let s = Signing::from_table(Some(&t), Some("IGNORED".into())).unwrap().unwrap();
        assert_eq!(s.team, "T1");
        assert_eq!(s.project_args(), ["--team", "T1", "--profile", "Album App Store", "--identity", "Apple Distribution"]);
        assert!(s.xcodebuild_auth().is_empty());
        let plist = s.export_options("com.example.album");
        assert!(plist.contains("<string>manual</string>"));
        assert!(plist.contains("\t\t<key>com.example.album</key>\n\t\t<string>Album App Store</string>"));
        assert!(plist.contains("<key>signingCertificate</key>\n\t<string>Apple Distribution</string>"));
    }

    #[test]
    fn an_api_key_goes_whole() {
        let t = table(json!({"team": "T", "api_key_path": "/k/AuthKey.p8", "api_key_id": "K", "api_issuer_id": "I"}));
        let s = Signing::from_table(Some(&t), None).unwrap().unwrap();
        assert_eq!(s.xcodebuild_auth(), ["-allowProvisioningUpdates", "-authenticationKeyPath", "/k/AuthKey.p8", "-authenticationKeyID", "K", "-authenticationKeyIssuerID", "I"]);
        assert!(Signing::from_table(Some(&table(json!({"team": "T", "api_key_id": "K"}))), None).is_err());
    }

    #[test]
    fn the_project_gets_the_manifest_modules_and_purposes() {
        let inputs = Inputs { app: "/w/app".into(), modules: ios_modules(&["photos.read".into(), "share.send".into()]), purposes: "/w/p.json".into(), icon: "/w/i.png".into(), notices: "/k/NOTICES.txt".into() };
        let args = make_project_args(&inputs, Path::new("/w/device"), "device", None);
        let at = |k: &str| args[args.iter().position(|a| a == k).unwrap() + 1].clone();
        assert_eq!(at("--modules"), "ExponentialEnginePhotos,ExponentialEngineShare");
        assert_eq!(at("--build"), "device");
        assert_eq!(at("--notices"), "/k/NOTICES.txt");
        assert!(!args.contains(&"--team".to_string()));
    }

    #[test]
    fn the_bundle_id_has_no_underscores() {
        assert_eq!(bundle_id("com.example.photo_book"), "com.example.photo-book");
    }

    #[test]
    fn the_app_icon_is_opaque_1024_over_the_background() {
        let icon = Icon::from_svg(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="3" fill="#ff0000"/></svg>"##).unwrap();
        let png = app_icon_png(&icon).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        assert_eq!((img.width(), img.height()), (1024, 1024));
        assert!(!img.color().has_alpha());
        let rgb = img.to_rgb8();
        assert_eq!(rgb.get_pixel(0, 0).0, background());
        assert_eq!(rgb.get_pixel(512, 512).0, [255, 0, 0]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_ipa_is_payload_app_zipped() {
        let work = tempfile::tempdir().unwrap();
        let app = work.path().join("built/Album.app");
        std::fs::create_dir_all(app.join("app")).unwrap();
        std::fs::write(app.join("app/runner.json"), "{}").unwrap();
        let ipa = work.path().join("Album-1.0.0.ipa");
        package_ipa(&app, &ipa, work.path()).unwrap();
        let list = Command::new("unzip").arg("-Z1").arg(&ipa).output().unwrap();
        let list = String::from_utf8_lossy(&list.stdout);
        assert!(list.lines().any(|l| l == "Payload/Album.app/app/runner.json"), "{list}");
    }
}
