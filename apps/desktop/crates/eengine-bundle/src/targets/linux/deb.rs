//! A `.deb` written here, no dpkg needed: an `ar` archive of `debian-binary`,
//! `control.tar.gz` and `data.tar.gz`, every member owned by root.

use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result};
use flate2::{write::GzEncoder, Compression};

/// A Debian package name from a config id: lowercase, `_` → `-` (policy §5.6.1).
pub fn package_name(id: &str) -> String {
    id.chars().map(|c| if c == '_' { '-' } else { c.to_ascii_lowercase() }).collect()
}

/// A semver as a Debian version: a pre-release sorts before its release (`-` → `~`).
pub fn version(semver: &str) -> String {
    match semver.split_once('+') {
        Some((v, build)) => format!("{}+{build}", v.replacen('-', "~", 1)),
        None => semver.replacen('-', "~", 1),
    }
}

/// Debian's name of a runner arch.
pub fn arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// The `control` file. `description` is the synopsis (one line).
pub fn control(package: &str, version: &str, arch: &str, maintainer: &str, depends: &[&str], installed_kib: u64, description: &str) -> String {
    let synopsis: String = description.lines().next().unwrap_or_default().chars().take(80).collect();
    let mut out = format!("Package: {package}\nVersion: {version}\nArchitecture: {arch}\nMaintainer: {maintainer}\nInstalled-Size: {installed_kib}\nSection: utils\nPriority: optional\n");
    if !depends.is_empty() {
        out.push_str(&format!("Depends: {}\n", depends.join(", ")));
    }
    out.push_str(&format!("Description: {}\n", synopsis.trim()));
    out
}

/// A `tar.gz` of the tree under `root` (`./` first), owned by root:root,
/// modes 0755 (directories, executables) or 0644, at `mtime`.
pub fn tar_gz(root: &Path, mtime: u64) -> Result<Vec<u8>> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::best()));
    let mut dir = tar::Header::new_gnu();
    dir.set_entry_type(tar::EntryType::Directory);
    set_owner(&mut dir, 0o755, mtime);
    dir.set_size(0);
    append(&mut tar, &mut dir, "./", &[])?;
    append_tree(&mut tar, root, root, mtime)?;
    Ok(tar.into_inner()?.finish()?)
}

/// An entry named exactly `name` (dpkg-deb's `./…`; tar's path setter drops the `./`),
/// a GNU long name past 100 bytes.
fn append<W: std::io::Write>(tar: &mut tar::Builder<W>, h: &mut tar::Header, name: &str, data: &[u8]) -> Result<()> {
    if name.len() < 100 {
        let field = &mut h.as_gnu_mut().expect("a GNU header").name;
        field.fill(0);
        field[..name.len()].copy_from_slice(name.as_bytes());
        h.set_cksum();
        tar.append(h, data)?;
    } else {
        tar.append_data(h, name, data)?;
    }
    Ok(())
}

fn set_owner(h: &mut tar::Header, mode: u32, mtime: u64) {
    h.set_mode(mode);
    h.set_uid(0);
    h.set_gid(0);
    let _ = h.set_username("root");
    let _ = h.set_groupname("root");
    h.set_mtime(mtime);
}

fn append_tree<W: std::io::Write>(tar: &mut tar::Builder<W>, root: &Path, dir: &Path, mtime: u64) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        let meta = std::fs::symlink_metadata(&path)?;
        let mut h = tar::Header::new_gnu();
        if meta.is_dir() {
            h.set_entry_type(tar::EntryType::Directory);
            set_owner(&mut h, 0o755, mtime);
            h.set_size(0);
            append(tar, &mut h, &format!("./{rel}/"), &[])?;
            append_tree(tar, root, &path, mtime)?;
        } else {
            let data = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            h.set_entry_type(tar::EntryType::Regular);
            set_owner(&mut h, if executable(&meta) { 0o755 } else { 0o644 }, mtime);
            h.set_size(data.len() as u64);
            append(tar, &mut h, &format!("./{rel}"), &data)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Bytes under `root` in KiB, rounded up per file (`Installed-Size`).
pub fn installed_kib(root: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        total += if meta.is_dir() { installed_kib(&entry.path())? } else { meta.len().div_ceil(1024) };
    }
    Ok(total)
}

/// An `ar` archive of `(name, bytes)` members (the common format dpkg reads).
pub fn ar(members: &[(&str, &[u8])], mtime: u64) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        let _ = write!(out, "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n", name, mtime, 0, 0, "100644", data.len());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
    out
}

/// The `.deb`: `control` + the tree under `root`.
pub fn build(control: &str, root: &Path, mtime: u64) -> Result<Vec<u8>> {
    let control_dir = tempfile_dir(root)?;
    std::fs::write(control_dir.join("control"), control)?;
    let control_tar = tar_gz(&control_dir, mtime);
    let _ = std::fs::remove_dir_all(&control_dir);
    let control_tar = control_tar?;
    let data_tar = tar_gz(root, mtime)?;
    Ok(ar(&[("debian-binary", b"2.0\n"), ("control.tar.gz", &control_tar), ("data.tar.gz", &data_tar)], mtime))
}

fn tempfile_dir(root: &Path) -> Result<std::path::PathBuf> {
    let dir = root.with_extension("control");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_versions_arches() {
        assert_eq!(package_name("at.Acme.photo_book"), "at.acme.photo-book");
        assert_eq!(version("1.0.0"), "1.0.0");
        assert_eq!(version("1.0.0-beta.1"), "1.0.0~beta.1");
        assert_eq!(version("1.0.0-rc-2+build.5"), "1.0.0~rc-2+build.5");
        assert_eq!(arch("x86_64"), "amd64");
        assert_eq!(arch("aarch64"), "arm64");
    }

    #[test]
    fn control_has_the_required_fields() {
        let c = control("acme.album", "1.0.0", "amd64", "Album", &["libc6"], 12, "The sample.\nMore.");
        assert_eq!(c, "Package: acme.album\nVersion: 1.0.0\nArchitecture: amd64\nMaintainer: Album\nInstalled-Size: 12\nSection: utils\nPriority: optional\nDepends: libc6\nDescription: The sample.\n");
    }

    #[test]
    fn ar_members_are_even_aligned() {
        let a = ar(&[("debian-binary", b"2.0\n"), ("x", b"odd")], 7);
        assert!(a.starts_with(b"!<arch>\ndebian-binary   7           0     0     100644  4         `\n2.0\n"));
        assert!(a.ends_with(b"`\nodd\n"));
        assert_eq!(a.len() % 2, 0);
    }

    #[test]
    fn the_tree_is_owned_by_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::write(root.join("usr/bin/x"), b"bin").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(root.join("usr/bin/x"), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let gz = tar_gz(&root, 5).unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz.as_slice()));
        let got: Vec<(String, u32, u64)> = archive.entries().unwrap().map(|e| {
            let e = e.unwrap();
            let h = e.header();
            (e.path().unwrap().to_string_lossy().into_owned(), h.mode().unwrap(), h.uid().unwrap())
        }).collect();
        let exe_mode = if cfg!(unix) { 0o755 } else { 0o644 };
        assert_eq!(got, vec![("./".into(), 0o755, 0), ("./usr/".into(), 0o755, 0), ("./usr/bin/".into(), 0o755, 0), ("./usr/bin/x".into(), exe_mode, 0)]);
    }
}
