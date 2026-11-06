//! `--target linux` (bundle.json `outputs.linux`, `permissions.linux`, `signing.linux`).
//!
//! Per runner arch in the kit (`<kit>/<arch>/exponential-runner`):
//! - the `.deb` (written here, `deb.rs`): the runner at `/usr/bin/<id>`, `app/` at
//!   `/usr/share/<id>/app` (runner.json `bundle.locations.linux`), the `.desktop`
//!   entry with the link scheme, the hicolor icons, the notices in `/usr/share/doc/<id>/`;
//! - the AppImage: the kit's pinned type2 runtime (`<kit>/<arch>/appimage-runtime`)
//!   followed by a squashfs of the AppDir (`mksquashfs`, squashfs-tools), the runner
//!   at the AppDir root beside `app/`. No update information: a new version is a rebundle.
//!
//! Both unsigned (`signing.linux.fallback`): distributions sign their repositories, not packages.

mod deb;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};

use super::{Job, Output};
use crate::manifest::linux_desktop_entry;

/// The runner's file name in the kit, per arch.
pub const RUNNER: &str = "exponential-runner";
/// The AppImage type2 runtime in the kit, per arch.
pub const RUNTIME: &str = "appimage-runtime";
/// The runtime's own notices in the kit, appended to the AppImage's.
pub const RUNTIME_NOTICES: &str = "appimage-runtime.NOTICES.txt";
/// The arches a kit may carry.
pub const ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// What the runner links (its NEEDED entries), plus the GPU loader wgpu opens
/// (Vulkan, else GL through EGL). Wayland is opened when present.
pub const DEPENDS: [&str; 6] = ["libc6", "libgcc-s1", "libxcb1", "libxkbcommon0", "libxkbcommon-x11-0", "libvulkan1 | libegl1"];

pub fn bundle(job: &Job) -> Result<Output> {
    let arches: Vec<&str> = ARCHES.into_iter().filter(|a| job.kit.join(a).join(RUNNER).is_file()).collect();
    if arches.is_empty() {
        bail!("the linux kit at {} has no runner (<arch>/{RUNNER} for one of {})", job.kit.display(), ARCHES.join(", "));
    }
    let mut out = Output::default();
    for arch in arches {
        out.files.push(write_deb(job, arch)?);
        out.files.push(write_appimage(job, arch)?);
    }
    out.notes.push("linux: the .deb and the AppImage are unsigned (distributions sign repositories, not packages)".into());
    Ok(out)
}

fn runner(job: &Job, arch: &str) -> PathBuf {
    job.kit.join(arch).join(RUNNER)
}

/// The hicolor icon theme under `share`: `icons/hicolor/<n>x<n>/apps/<id>.png`.
fn write_hicolor(job: &Job, share: &Path) -> Result<()> {
    for (_, size) in crate::icons::sizes("linux") {
        let dir = share.join(format!("icons/hicolor/{size}x{size}/apps"));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{}.png", job.prepared.config.id)), job.icon.png(size)?)?;
    }
    Ok(())
}

fn copy_executable(from: &Path, to: &Path) -> Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to).with_context(|| format!("copy {}", from.display()))?;
    set_executable(to)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// The `.deb`'s file tree under `root`.
fn deb_tree(job: &Job, arch: &str, root: &Path) -> Result<()> {
    let id = &job.prepared.config.id;
    copy_executable(&runner(job, arch), &root.join("usr/bin").join(id))?;
    let share = root.join("usr/share");
    job.write_app_dir(&share.join(id).join("app"))?;
    std::fs::create_dir_all(share.join("applications"))?;
    std::fs::write(share.join("applications").join(format!("{id}.desktop")), linux_desktop_entry(&job.prepared.config))?;
    write_hicolor(job, &share)?;
    let doc = share.join("doc").join(id);
    std::fs::create_dir_all(&doc)?;
    std::fs::copy(job.notices(), doc.join(crate::kit::notices_file()))?;
    Ok(())
}

fn now() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
}

fn write_deb(job: &Job, arch: &str) -> Result<PathBuf> {
    let c = &job.prepared.config;
    let root = job.work.join(format!("deb-{arch}"));
    deb_tree(job, arch, &root)?;
    let description = job.prepared.package.get("description").and_then(|d| d.as_str()).unwrap_or(&c.name);
    let maintainer = job.prepared.author.as_deref().unwrap_or(&c.name);
    let control = deb::control(&deb::package_name(&c.id), &deb::version(&c.version), deb::arch(arch), maintainer, &DEPENDS, deb::installed_kib(&root)?, description);
    let bytes = deb::build(&control, &root, now())?;
    let file = job.out.join(job.output_name(&output_pattern(0), deb::arch(arch)));
    std::fs::write(&file, bytes)?;
    Ok(file)
}

fn output_pattern(i: usize) -> String {
    crate::contract::strings("/outputs/linux").get(i).cloned().unwrap_or_default()
}

/// The AppDir: the runner beside `app/` (runner.json: `<exe dir>/app`), `AppRun`
/// linking to it, the `.desktop` entry, the icon and `.DirIcon` at the root.
fn appdir(job: &Job, arch: &str, dir: &Path) -> Result<()> {
    let id = &job.prepared.config.id;
    copy_executable(&runner(job, arch), &dir.join(id))?;
    job.write_app_dir(&dir.join("app"))?;
    std::fs::write(dir.join(format!("{id}.desktop")), linux_desktop_entry(&job.prepared.config))?;
    std::fs::write(dir.join(format!("{id}.png")), job.icon.png(256)?)?;
    write_hicolor(job, &dir.join("usr/share"))?;
    std::fs::write(dir.join(crate::kit::notices_file()), appimage_notices(job, arch)?)?;
    link(id, &dir.join("AppRun"))?;
    link(&format!("{id}.png"), &dir.join(".DirIcon"))?;
    Ok(())
}

/// The kit's notices + the AppImage runtime's (it is inside every AppImage).
fn appimage_notices(job: &Job, arch: &str) -> Result<Vec<u8>> {
    let mut text = std::fs::read(job.notices())?;
    for extra in [job.kit.join(arch).join(RUNTIME_NOTICES), job.kit.join(RUNTIME_NOTICES)] {
        if extra.is_file() {
            text.extend_from_slice(b"\n\n");
            text.extend(std::fs::read(extra)?);
            break;
        }
    }
    Ok(text)
}

#[cfg(unix)]
fn link(target: &str, at: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, at).with_context(|| format!("link {}", at.display()))
}

#[cfg(not(unix))]
fn link(target: &str, at: &Path) -> Result<()> {
    let dir = at.parent().unwrap_or(Path::new("."));
    std::fs::copy(dir.join(target), at)?;
    Ok(())
}

/// A tool: `$<env>` else `name` on PATH.
fn tool(env: &str, name: &str) -> PathBuf {
    std::env::var_os(env).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(name))
}

fn write_appimage(job: &Job, arch: &str) -> Result<PathBuf> {
    let runtime = job.kit.join(arch).join(RUNTIME);
    if !runtime.is_file() {
        bail!("the linux kit has no AppImage runtime at {} (scripts/runner-kits/linux.sh fetches the pinned one)", runtime.display());
    }
    let dir = job.work.join(format!("AppDir-{arch}"));
    appdir(job, arch, &dir)?;
    let squashfs = job.work.join(format!("{arch}.squashfs"));
    let mksquashfs = tool("MKSQUASHFS", "mksquashfs");
    let status = Command::new(&mksquashfs)
        .arg(&dir)
        .arg(&squashfs)
        .args(["-root-owned", "-noappend", "-no-progress", "-quiet", "-comp", "zstd", "-b", "1M"])
        .status()
        .with_context(|| format!("the AppImage needs mksquashfs (squashfs-tools; or set MKSQUASHFS): {}", mksquashfs.display()))?;
    if !status.success() {
        bail!("mksquashfs failed ({status})");
    }
    let mut bytes = std::fs::read(&runtime)?;
    bytes.extend(std::fs::read(&squashfs)?);
    let file = job.out.join(job.output_name(&output_pattern(1), arch));
    std::fs::write(&file, bytes)?;
    set_executable(&file)?;
    Ok(file)
}
