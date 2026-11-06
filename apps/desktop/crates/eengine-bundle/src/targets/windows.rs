//! `--target windows` (bundle.json `outputs.windows`, `permissions.windows`, `signing.windows`).
//!
//! One NSIS installer (`makensis`): a per-user install (no elevation) into
//! `%LOCALAPPDATA%\Programs\<name>` of the kit's runner renamed `<name>.exe`, `app/`
//! beside it (runner.json `bundle.locations.windows`), the `.ico` rendered from the
//! SVG, the notices, a Start-menu shortcut, the link scheme under
//! `manifest::windows_protocol_key` (`"<exe>" "%1"`) and an uninstaller that
//! removes all of it (the app's data under `%APPDATA%` stays, like any app's).
//!
//! Signed with osslsigncode when bundle.toml has `[signing.windows]` `pfx` (+
//! `password_env` or `password`, optional `timestamp_url`): the app's `.exe` before
//! packing, then the installer. Without it both are unsigned and SmartScreen warns.

mod ico;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};
use serde_json::Value;

use super::{Job, Output};
use crate::manifest::windows_protocol_key;

/// The runner's file name in the kit.
pub const RUNNER: &str = "exponential-runner.exe";

/// What the installer script is made of.
pub struct Installer<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub publisher: &'a str,
    /// The app's `.exe` file name (`exe_name`).
    pub exe: &'a str,
    /// `HKCU\Software\Classes\<scheme>`.
    pub protocol_key: &'a str,
    /// Prepared inputs: the renamed runner, `app/`, the `.ico`, the notices.
    pub runner: &'a Path,
    pub app_dir: &'a Path,
    pub icon: &'a Path,
    pub notices: &'a Path,
    pub out_file: &'a Path,
}

/// A file name Windows accepts, from the app name (`<>:"/\|?*` and control
/// characters dropped, no trailing dots or spaces, never a reserved device name).
pub fn exe_name(name: &str, id: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c)).collect();
    let clean = clean.trim().trim_end_matches(['.', ' ']).to_string();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    let stem = if clean.is_empty() || reserved.contains(&clean.to_ascii_uppercase().as_str()) { id.to_string() } else { clean };
    format!("{stem}.exe")
}

/// `HKCU\Software\Classes\x` → `("HKCU", "Software\Classes\x")`.
pub fn split_key(key: &str) -> (&str, &str) {
    key.split_once('\\').unwrap_or(("HKCU", key))
}

/// The four numbers of `VIProductVersion` from a semver (`1.2.3-beta` → `1.2.3.0`).
pub fn file_version(semver: &str) -> String {
    let core = semver.split(['-', '+']).next().unwrap_or_default();
    let mut n: Vec<u32> = core.split('.').map(|p| p.parse::<u32>().unwrap_or(0).min(65535)).collect();
    n.resize(4, 0);
    n.iter().map(u32::to_string).collect::<Vec<_>>().join(".")
}

/// An NSIS string's contents: `$` and `"` escaped.
pub fn nsis_str(s: &str) -> String {
    s.replace('$', "$$").replace('"', "$\\\"").replace('\n', "$\\n").replace('\r', "$\\r")
}

fn path(p: &Path) -> String {
    nsis_str(&p.to_string_lossy())
}

/// The `.nsi` script.
pub fn script(i: &Installer) -> String {
    let (root, classes) = split_key(i.protocol_key);
    let q = nsis_str;
    let name = q(i.name);
    let exe = q(i.exe);
    let classes = q(classes);
    let uninstall = format!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}", q(i.id));
    let package = path(&i.app_dir.join(eengine::runner::bundle_package_file()));
    let config = path(&i.app_dir.join(eengine::runner::bundle_config_file()));
    let notices = crate::kit::notices_file();
    format!(
        r#"; Written by `exponential bundle --target windows` (eengine-bundle). Per-user, no updater.
Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma
RequestExecutionLevel user
Name "{name}"
OutFile "{out}"
InstallDir "$LOCALAPPDATA\Programs\{name}"
InstallDirRegKey {root} "{uninstall}" "InstallLocation"
BrandingText " "
VIProductVersion "{fv}"
VIAddVersionKey "ProductName" "{name}"
VIAddVersionKey "ProductVersion" "{version}"
VIAddVersionKey "FileVersion" "{version}"
VIAddVersionKey "FileDescription" "{name} Setup"
VIAddVersionKey "CompanyName" "{publisher}"
VIAddVersionKey "LegalCopyright" "{publisher}"

!include "MUI2.nsh"
!define MUI_ICON "{icon}"
!define MUI_UNICON "{icon}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\{exe}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "/oname={exe}" "{runner}"
  File "/oname=app.ico" "{icon}"
  File "/oname={notices}" "{notices_src}"
  SetOutPath "$INSTDIR\app"
  File "{package}"
  File "{config}"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\{name}.lnk" "$INSTDIR\{exe}" "" "$INSTDIR\app.ico" 0
  WriteRegStr {root} "{classes}" "" "URL:{name}"
  WriteRegStr {root} "{classes}" "URL Protocol" ""
  WriteRegStr {root} "{classes}\DefaultIcon" "" "$INSTDIR\app.ico"
  WriteRegStr {root} "{classes}\shell\open\command" "" '"$INSTDIR\{exe}" "%1"'
  WriteRegStr {root} "{uninstall}" "DisplayName" "{name}"
  WriteRegStr {root} "{uninstall}" "DisplayVersion" "{version}"
  WriteRegStr {root} "{uninstall}" "Publisher" "{publisher}"
  WriteRegStr {root} "{uninstall}" "DisplayIcon" "$INSTDIR\app.ico"
  WriteRegStr {root} "{uninstall}" "InstallLocation" "$INSTDIR"
  WriteRegStr {root} "{uninstall}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr {root} "{uninstall}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD {root} "{uninstall}" "NoModify" 1
  WriteRegDWORD {root} "{uninstall}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\{name}.lnk"
  DeleteRegKey {root} "{classes}"
  DeleteRegKey {root} "{uninstall}"
  Delete "$INSTDIR\app\{package_file}"
  Delete "$INSTDIR\app\{config_file}"
  RMDir "$INSTDIR\app"
  Delete "$INSTDIR\{exe}"
  Delete "$INSTDIR\app.ico"
  Delete "$INSTDIR\{notices}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
"#,
        out = path(i.out_file),
        fv = file_version(i.version),
        version = q(i.version),
        publisher = q(i.publisher),
        icon = path(i.icon),
        runner = path(i.runner),
        notices_src = path(i.notices),
        package_file = eengine::runner::bundle_package_file(),
        config_file = eengine::runner::bundle_config_file(),
    )
}

/// A tool: `$<env>` else `name` on PATH.
fn tool(env: &str, name: &str) -> PathBuf {
    std::env::var_os(env).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(name))
}

/// The author's osslsigncode inputs from `[signing.windows]`.
struct Signing {
    pfx: PathBuf,
    password: String,
    timestamp: Option<String>,
}

fn signing(job: &Job) -> Result<Option<Signing>> {
    let Some(table) = job.signing("windows") else { return Ok(None) };
    let s = |k: &str| table.get(k).and_then(Value::as_str);
    let Some(pfx) = s("pfx") else {
        bail!("[signing.windows] needs `pfx` (a .pfx certificate; `password_env` or `password`; optional `timestamp_url`)");
    };
    let password = match (s("password_env"), s("password")) {
        (Some(var), _) => std::env::var(var).with_context(|| format!("[signing.windows] password_env: ${var} is not set"))?,
        (None, Some(p)) => p.to_string(),
        (None, None) => String::new(),
    };
    Ok(Some(Signing { pfx: PathBuf::from(pfx), password, timestamp: s("timestamp_url").map(String::from) }))
}

/// Sign `file` in place (osslsigncode writes a copy).
fn sign(job: &Job, s: &Signing, file: &Path) -> Result<()> {
    let pass = job.work.join("pfx.pass");
    std::fs::write(&pass, &s.password)?;
    let signed = file.with_extension("signed.exe");
    let osslsigncode = tool("OSSLSIGNCODE", "osslsigncode");
    let mut cmd = Command::new(&osslsigncode);
    cmd.args(["sign", "-h", "sha256", "-pkcs12"]).arg(&s.pfx).arg("-readpass").arg(&pass).args(["-n", &job.prepared.config.name]);
    if let Some(ts) = &s.timestamp {
        cmd.args(["-t", ts]);
    }
    let status = cmd.arg("-in").arg(file).arg("-out").arg(&signed).status().with_context(|| format!("[signing.windows] needs osslsigncode (or set OSSLSIGNCODE): {}", osslsigncode.display()));
    let _ = std::fs::remove_file(&pass);
    if !status?.success() {
        let _ = std::fs::remove_file(&signed);
        bail!("osslsigncode could not sign {}", file.display());
    }
    std::fs::rename(&signed, file)?;
    Ok(())
}

pub fn bundle(job: &Job) -> Result<Output> {
    let c = &job.prepared.config;
    let kit_runner = job.kit.join(RUNNER);
    if !kit_runner.is_file() {
        bail!("the windows kit at {} has no {RUNNER}", job.kit.display());
    }
    let signing = signing(job)?;
    let exe = exe_name(&c.name, &c.id);
    let runner = job.work.join(&exe);
    std::fs::copy(&kit_runner, &runner)?;
    if let Some(s) = &signing {
        sign(job, s, &runner)?;
    }
    let app_dir = job.work.join("app");
    job.write_app_dir(&app_dir)?;
    let icon = job.work.join("app.ico");
    let images = crate::icons::sizes("windows").into_iter().map(|(_, n)| Ok((n, job.icon.png(n)?))).collect::<Result<Vec<_>>>()?;
    std::fs::write(&icon, ico::ico(&images))?;
    let pattern = crate::contract::strings("/outputs/windows").first().cloned().unwrap_or_default();
    let out_file = std::path::absolute(job.out.join(job.output_name(&pattern, "x86_64")))?;
    let protocol_key = windows_protocol_key(c);
    let abs = |p: &Path| std::path::absolute(p);
    let notices = job.notices();
    let (runner, app_dir, icon, notices) = (abs(&runner)?, abs(&app_dir)?, abs(&icon)?, abs(&notices)?);
    let installer = Installer {
        id: &c.id,
        name: &c.name,
        version: &c.version,
        publisher: job.prepared.author.as_deref().unwrap_or(&c.name),
        exe: &exe,
        protocol_key: &protocol_key,
        runner: &runner,
        app_dir: &app_dir,
        icon: &icon,
        notices: &notices,
        out_file: &out_file,
    };
    let nsi = job.work.join("installer.nsi");
    std::fs::write(&nsi, script(&installer))?;
    let makensis = tool("MAKENSIS", "makensis");
    let output = Command::new(&makensis)
        .args(["-V2", "-INPUTCHARSET", "UTF8"])
        .arg(&nsi)
        .output()
        .with_context(|| format!("the installer needs NSIS 3 (makensis; or set MAKENSIS): {}", makensis.display()))?;
    if !output.status.success() {
        bail!("makensis failed: {}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
    let mut out = Output { files: vec![out_file.clone()], notes: Vec::new() };
    match &signing {
        Some(s) => sign(job, s, &out_file)?,
        None => out.notes.push("windows: the installer and the app are unsigned (SmartScreen warns); add [signing.windows] pfx + password_env, or sign the setup.exe with Azure Trusted Signing".into()),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_names_are_windows_file_names() {
        assert_eq!(exe_name("Photobook", "at.acme.photobook"), "Photobook.exe");
        assert_eq!(exe_name("A/B: \"C\"?. ", "x.y"), "AB C.exe");
        assert_eq!(exe_name("con", "x.y"), "x.y.exe");
        assert_eq!(exe_name("…", "x.y"), "….exe");
        assert_eq!(exe_name("", "x.y"), "x.y.exe");
    }

    #[test]
    fn keys_versions_strings() {
        assert_eq!(split_key("HKCU\\Software\\Classes\\acme.album"), ("HKCU", "Software\\Classes\\acme.album"));
        assert_eq!(file_version("1.2.3"), "1.2.3.0");
        assert_eq!(file_version("1.0.0-beta.2+7"), "1.0.0.0");
        assert_eq!(nsis_str("a$b\"c"), "a$$b$\\\"c");
    }

    #[test]
    fn the_script_installs_per_user_with_the_protocol_and_an_uninstaller() {
        let p = Path::new;
        let s = script(&Installer {
            id: "acme.album",
            name: "Album",
            version: "1.0.0",
            publisher: "Acme",
            exe: "Album.exe",
            protocol_key: "HKCU\\Software\\Classes\\acme.album",
            runner: p("/w/Album.exe"),
            app_dir: p("/w/app"),
            icon: p("/w/app.ico"),
            notices: p("/k/NOTICES.txt"),
            out_file: p("/o/Album-1.0.0-setup.exe"),
        });
        for line in [
            "RequestExecutionLevel user",
            "OutFile \"/o/Album-1.0.0-setup.exe\"",
            "InstallDir \"$LOCALAPPDATA\\Programs\\Album\"",
            "File \"/oname=Album.exe\" \"/w/Album.exe\"",
            "File \"/oname=NOTICES.txt\" \"/k/NOTICES.txt\"",
            "SetOutPath \"$INSTDIR\\app\"",
            "File \"/w/app/package.app.json\"",
            "File \"/w/app/runner.json\"",
            "CreateShortCut \"$SMPROGRAMS\\Album.lnk\" \"$INSTDIR\\Album.exe\"",
            "WriteRegStr HKCU \"Software\\Classes\\acme.album\" \"URL Protocol\" \"\"",
            "WriteRegStr HKCU \"Software\\Classes\\acme.album\\shell\\open\\command\" \"\" '\"$INSTDIR\\Album.exe\" \"%1\"'",
            "WriteUninstaller \"$INSTDIR\\uninstall.exe\"",
            "DeleteRegKey HKCU \"Software\\Classes\\acme.album\"",
            "DeleteRegKey HKCU \"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\acme.album\"",
            "VIProductVersion \"1.0.0.0\"",
        ] {
            assert!(s.lines().any(|l| l.trim().starts_with(line)), "missing `{line}` in\n{s}");
        }
        assert!(!s.contains("HKLM"), "per-user only");
    }
}
