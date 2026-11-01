//! EXP-1536 A (#FEED-168, #FEED-171): a worktree folder left on disk by an
//! earlier run never fails a start, and removing one never leaves a husk.
//!
//! - [`classify`]: what sits at a layout path a launch wants. A live
//!   worktree is the caller's (reuse / reclaim); a folder WITHOUT a working
//!   `.git` (a cleanup that died after git deleted `.git`, a broken gitlink)
//!   is [`Leftover::Clearable`] only when every file in it is byte-for-byte
//!   a blob of origin's branch or base at the same path (build output and
//!   the launcher's own seed files aside). Anything else is
//!   [`Leftover::Unverified`]: never deleted; the run starts next to it and
//!   asks the person first.
//! - [`remove_worktree`]: build output (`node_modules`, `target`, …) first,
//!   then everything else, `.git` LAST, with long-path safe deletion
//!   ([`long_path`]: `\\?\` on Windows, `core.longpaths` for git). A failure
//!   restores the checkout, so what stays is a valid worktree, never a
//!   `.git`-less folder.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::git_worktree::{run_git, GitError};
use crate::worktree_lifecycle::is_build_output_name;

/// Files the launcher itself drops into a checkout (regenerable, never
/// committed): a leftover holding only these besides origin's files is
/// still clearable.
pub const LAUNCHER_FILES: &[&str] = &[
    crate::mcp_json::MCP_JSON_FILE,
    crate::worktree_agents::AGENTS_FILE,
    crate::launcher::STEER_IMAGES_DIR,
    crate::git_worktree::CLAUDE_WORKTREES_DIR,
];

/// `rel` (`/`-separated) is one of [`LAUNCHER_FILES`].
fn launcher_file(rel: &str) -> bool {
    LAUNCHER_FILES.iter().any(|entry| entry.trim_end_matches('/') == rel)
}

/// How many unverified paths a [`Leftover::Unverified`] names.
const SAMPLE_PATHS: usize = 5;

/// What [`classify`] found at a layout path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leftover {
    /// Nothing there.
    Absent,
    /// A worktree git can read: the caller's existing reuse rules apply.
    Worktree,
    /// No working `.git`, and every file matches origin: safe to clear.
    Clearable,
    /// No working `.git`, and `count` files git cannot vouch for
    /// (`samples` = the first few, relative): never deleted.
    Unverified { count: usize, samples: Vec<String> },
}

/// Classify `dir` (a layout path of `clone`) against `refs` (most wanted
/// first, e.g. `origin/<branch>`, `<branch>`, `origin/<base>`; refs the
/// clone lacks are skipped).
pub fn classify(clone: &Path, dir: &Path, refs: &[String]) -> Leftover {
    if std::fs::symlink_metadata(dir).is_err() {
        return Leftover::Absent;
    }
    if is_live_worktree(dir) {
        return Leftover::Worktree;
    }
    let files = collect_files(dir);
    if files.is_empty() {
        return Leftover::Clearable;
    }
    let trees: Vec<BTreeMap<String, String>> =
        refs.iter().filter_map(|reference| ls_tree(clone, reference)).collect();
    let hashes = hash_files(clone, dir, &files);
    let unverified: Vec<&String> = files
        .iter()
        .filter(|rel| {
            let candidates = hashes.get(*rel);
            !trees.iter().any(|tree| {
                tree.get(*rel)
                    .is_some_and(|blob| candidates.is_some_and(|ours| ours.contains(blob)))
            })
        })
        .collect();
    if unverified.is_empty() {
        Leftover::Clearable
    } else {
        Leftover::Unverified {
            count: unverified.len(),
            samples: unverified.iter().take(SAMPLE_PATHS).map(|rel| (*rel).clone()).collect(),
        }
    }
}

/// `dir` holds a `.git` that git resolves to `dir` itself.
fn is_live_worktree(dir: &Path) -> bool {
    if std::fs::symlink_metadata(dir.join(".git")).is_err() {
        return false;
    }
    let Ok(top) = run_git(Some(dir), &["rev-parse", "--show-toplevel"], None, "git rev-parse --show-toplevel")
    else {
        return false;
    };
    let top = PathBuf::from(top.trim());
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(&top) == canonical(dir)
}

/// Every regular file and symlink under `dir`, relative with `/`
/// separators, skipping build output, the launcher's files and a top-level
/// `.git` (a dead gitlink file or a half-deleted dir).
fn collect_files(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(long_path(&dir.join(&rel))) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy().into_owned();
            let child = rel.join(&name);
            let child_rel = child.to_string_lossy().replace('\\', "/");
            if (rel.as_os_str().is_empty() && name == ".git") || launcher_file(&child_rel) {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !is_build_output_name(&name) {
                    stack.push(child);
                }
            } else {
                found.push(child_rel);
            }
        }
    }
    found.sort();
    found
}

/// `<ref>`'s blobs by path, `None` when the clone has no such ref.
fn ls_tree(clone: &Path, reference: &str) -> Option<BTreeMap<String, String>> {
    let out = run_git(
        Some(clone),
        &["ls-tree", "-r", "-z", "--full-tree", reference],
        None,
        &format!("git ls-tree {reference}"),
    )
    .ok()?;
    Some(
        out.split('\0')
            .filter_map(|line| {
                let (meta, path) = line.split_once('\t')?;
                let blob = meta.split_whitespace().nth(2)?;
                Some((path.to_string(), blob.to_string()))
            })
            .collect(),
    )
}

/// Each file's blob ids as git would store it: raw bytes, and with CRLF
/// normalized (a Windows checkout under `core.autocrlf`). Symlinks hash
/// their target, like git does.
fn hash_files(clone: &Path, dir: &Path, files: &[String]) -> BTreeMap<String, BTreeSet<String>> {
    let mut hashes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut plain = Vec::new();
    for rel in files {
        let path = dir.join(rel);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                if let Ok(target) = std::fs::read_link(&path) {
                    let blob = blob_id(target.to_string_lossy().replace('\\', "/").as_bytes());
                    hashes.entry(rel.clone()).or_default().insert(blob);
                }
            }
            _ => plain.push(rel.clone()),
        }
    }
    for chunk in plain.chunks(64) {
        let paths: Vec<String> =
            chunk.iter().map(|rel| dir.join(rel).to_string_lossy().into_owned()).collect();
        for filters in [&["--no-filters"][..], &[][..]] {
            let mut args: Vec<&str> = vec!["-c", "core.autocrlf=true", "hash-object"];
            args.extend_from_slice(filters);
            args.push("--");
            args.extend(paths.iter().map(String::as_str));
            let Ok(out) = run_git(Some(clone), &args, None, "git hash-object (leftover)") else {
                continue;
            };
            for (rel, blob) in chunk.iter().zip(out.lines()) {
                hashes.entry(rel.clone()).or_default().insert(blob.trim().to_string());
            }
        }
    }
    hashes
}

/// A git blob id (sha1 of `blob <len>\0<bytes>`), for symlink targets.
fn blob_id(bytes: &[u8]) -> String {
    let mut data = format!("blob {}\0", bytes.len()).into_bytes();
    data.extend_from_slice(bytes);
    sha1_hex(&data)
}

/// A minimal SHA-1 (FIPS 180-1), only for [`blob_id`]: symlink targets are
/// a few bytes, and git's own hash-object would follow the link.
fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

/// `path` in the form Windows accepts past `MAX_PATH`: absolute paths get
/// the `\\?\` (or `\\?\UNC\`) prefix. Identity elsewhere.
pub fn long_path(path: &Path) -> PathBuf {
    if cfg!(windows) {
        verbatim(path)
    } else {
        path.to_path_buf()
    }
}

/// The `\\?\` rewrite itself (platform-free, so every CI runs its test).
fn verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
        return path.to_path_buf();
    }
    let text = text.replace('/', r"\");
    if let Some(unc) = text.strip_prefix(r"\\") {
        return PathBuf::from(format!(r"\\?\UNC\{unc}"));
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
        return PathBuf::from(format!(r"\\?\{text}"));
    }
    path.to_path_buf()
}

/// Remove `path` (a file or a whole tree), long-path safe; a read-only
/// entry (Windows git objects, some `node_modules`) is made writable and
/// retried once. Already gone = `Ok`.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    let long = long_path(path);
    let Ok(meta) = std::fs::symlink_metadata(&long) else {
        return Ok(());
    };
    let remove = |p: &Path| {
        if meta.is_dir() {
            std::fs::remove_dir_all(p)
        } else {
            std::fs::remove_file(p)
        }
    };
    match remove(&long) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
            clear_readonly(&long);
            remove(&long)
        }
        Err(err) => Err(err),
    }
}

#[allow(clippy::permissions_set_readonly_false)]
fn clear_readonly(path: &Path) {
    let mut stack = vec![path.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&next) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(&next, perms);
        }
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&next) {
                stack.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
}

/// Clear a [`Leftover::Clearable`] folder: build output first, the rest
/// after, a dead `.git` last.
pub fn clear_leftover(dir: &Path) -> io::Result<()> {
    remove_contents(dir)?;
    remove_tree(&dir.join(".git"))?;
    remove_tree(dir)
}

/// Delete everything in `dir` except its top-level `.git`: build-output
/// dirs (at any depth up to the lifecycle's) first, then the remaining
/// top-level entries.
fn remove_contents(dir: &Path) -> io::Result<()> {
    for rel in crate::worktree_lifecycle::build_output_candidates(dir) {
        remove_tree(&dir.join(rel))?;
    }
    let Ok(entries) = std::fs::read_dir(long_path(dir)) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        if entry.file_name() == ".git" {
            continue;
        }
        remove_tree(&dir.join(entry.file_name()))?;
    }
    Ok(())
}

/// Remove a registered worktree whose work the caller already verified
/// (clean, or untracked debris only, or everything pushed): its content
/// goes first (build output, then the rest), `.git` and git's registration
/// last through `git -c core.longpaths=true worktree remove --force`. Any
/// failure restores the tracked files (`git checkout`), so the folder left
/// behind is still a valid worktree on its branch.
pub fn remove_worktree(clone: &Path, worktree: &Path, op: &str) -> Result<(), GitError> {
    let restore = |detail: String| {
        let _ = run_git(
            Some(worktree),
            &["-c", "core.longpaths=true", "checkout", "--", "."],
            None,
            "git checkout (restore after a failed removal)",
        );
        GitError { op: op.to_string(), detail }
    };
    if let Err(err) = remove_contents(worktree) {
        return Err(restore(err.to_string()));
    }
    let path = worktree.to_string_lossy().into_owned();
    if let Err(err) = run_git(
        Some(clone),
        &["-c", "core.longpaths=true", "worktree", "remove", "--force", &path],
        None,
        op,
    ) {
        return Err(restore(err.detail));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("exp-leftover-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A clone with `main` (README.md, src/lib.rs) and a worktree on
    /// `exp/A-1` at `<clone>.worktrees/exp-A-1`.
    fn clone_with_worktree(root: &Path) -> (PathBuf, PathBuf) {
        let clone = root.join("app");
        fs::create_dir_all(clone.join("src")).unwrap();
        git(&clone, &["init", "--quiet", "-b", "main"]);
        fs::write(clone.join("README.md"), "seed\n").unwrap();
        fs::write(clone.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        fs::write(clone.join(".gitignore"), "node_modules/\n").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "--quiet", "-m", "seed"]);
        let worktree = root.join("app.worktrees").join("exp-A-1");
        git(&clone, &["worktree", "add", "--quiet", "-b", "exp/A-1", worktree.to_str().unwrap()]);
        (clone, worktree)
    }

    fn refs() -> Vec<String> {
        vec!["exp/A-1".to_string(), "main".to_string()]
    }

    #[test]
    fn sha1_matches_git_blob_ids() {
        assert_eq!(blob_id(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        for bytes in [&b"seed\n"[..], b"../shared/target", &[7u8; 200][..]] {
            assert_eq!(blob_id(bytes), git_blob(bytes));
        }
    }

    fn git_blob(bytes: &[u8]) -> String {
        use std::io::Write;
        let mut child = Command::new("git")
            .args(["hash-object", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).trim().to_string()
    }

    #[test]
    fn verbatim_prefixes_drive_and_unc_paths_only() {
        assert_eq!(verbatim(Path::new(r"C:\Users\m\app.worktrees\exp-MA-1430")), PathBuf::from(r"\\?\C:\Users\m\app.worktrees\exp-MA-1430"));
        assert_eq!(verbatim(Path::new("C:/Users/m/app")), PathBuf::from(r"\\?\C:\Users\m\app"));
        assert_eq!(verbatim(Path::new(r"\\nas\share\app")), PathBuf::from(r"\\?\UNC\nas\share\app"));
        assert_eq!(verbatim(Path::new(r"\\?\C:\already")), PathBuf::from(r"\\?\C:\already"));
        assert_eq!(verbatim(Path::new("relative/dir")), PathBuf::from("relative/dir"));
        if !cfg!(windows) {
            assert_eq!(long_path(Path::new("/tmp/x")), PathBuf::from("/tmp/x"));
        }
    }

    #[test]
    fn a_live_worktree_and_a_missing_dir_are_told_apart() {
        let dir = temp_dir("live");
        let (clone, worktree) = clone_with_worktree(&dir.0);
        assert_eq!(classify(&clone, &worktree, &refs()), Leftover::Worktree);
        assert_eq!(classify(&clone, &dir.0.join("nope"), &refs()), Leftover::Absent);
    }

    /// #FEED-171: the cleanup died after `.git` was gone; what is left is
    /// origin's files plus `node_modules` (and a CRLF checkout): clearable.
    #[test]
    fn a_husk_matching_origin_is_clearable_and_cleared() {
        let dir = temp_dir("husk");
        let (clone, worktree) = clone_with_worktree(&dir.0);
        fs::create_dir_all(worktree.join("node_modules/a/b")).unwrap();
        fs::write(worktree.join("node_modules/a/b/index.js"), "x").unwrap();
        fs::write(worktree.join(crate::mcp_json::MCP_JSON_FILE), "{}").unwrap();
        fs::write(worktree.join("README.md"), "seed\r\n").unwrap();
        fs::remove_file(worktree.join(".git")).unwrap();
        assert_eq!(classify(&clone, &worktree, &refs()), Leftover::Clearable);
        clear_leftover(&worktree).unwrap();
        assert!(!worktree.exists());
    }

    /// A broken gitlink (git's admin dir pruned) reads like a husk, too.
    #[test]
    fn a_dead_gitlink_is_not_a_worktree() {
        let dir = temp_dir("gitlink");
        let (clone, worktree) = clone_with_worktree(&dir.0);
        fs::write(worktree.join(".git"), "gitdir: /nowhere/at/all\n").unwrap();
        assert_eq!(classify(&clone, &worktree, &refs()), Leftover::Clearable);
    }

    /// Never delete unverified work: an edited or a new file keeps the
    /// folder, and the verdict names it.
    #[test]
    fn a_husk_with_unknown_files_is_unverified() {
        let dir = temp_dir("unverified");
        let (clone, worktree) = clone_with_worktree(&dir.0);
        fs::write(worktree.join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        fs::write(worktree.join("notes.txt"), "mine").unwrap();
        fs::remove_file(worktree.join(".git")).unwrap();
        match classify(&clone, &worktree, &refs()) {
            Leftover::Unverified { count, samples } => {
                assert_eq!(count, 2);
                assert_eq!(samples, vec!["notes.txt".to_string(), "src/lib.rs".to_string()]);
            }
            other => panic!("expected Unverified, got {other:?}"),
        }
        // No ref to compare with = nothing is verified.
        fs::write(worktree.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        fs::remove_file(worktree.join("notes.txt")).unwrap();
        assert!(matches!(classify(&clone, &worktree, &["gone".to_string()]), Leftover::Unverified { .. }));
    }

    /// #FEED-171: removal goes build output first and `.git` last; the
    /// worktree and its registration are gone.
    #[test]
    fn remove_worktree_removes_content_then_git() {
        let dir = temp_dir("remove");
        let (clone, worktree) = clone_with_worktree(&dir.0);
        fs::create_dir_all(worktree.join("node_modules/deep/er")).unwrap();
        fs::write(worktree.join("node_modules/deep/er/x.js"), "x").unwrap();
        remove_worktree(&clone, &worktree, "git worktree remove (exp/A-1)").unwrap();
        assert!(!worktree.exists());
        assert!(!git(&clone, &["worktree", "list", "--porcelain"]).contains("exp-A-1"));
    }

    /// A removal that fails half-way leaves a VALID worktree (tracked files
    /// restored, `.git` intact), never a husk.
    #[cfg(unix)]
    #[test]
    fn a_failed_removal_leaves_a_valid_worktree() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("fail");
        let (clone, worktree) = clone_with_worktree(&dir.0);
        let stuck = worktree.join("zz-stuck");
        fs::create_dir_all(stuck.join("inner")).unwrap();
        fs::write(stuck.join("inner/file"), "x").unwrap();
        fs::set_permissions(&stuck, fs::Permissions::from_mode(0o500)).unwrap();
        if fs::remove_file(stuck.join("inner/file")).is_ok() {
            return; // running as root: permissions do not stop deletion
        }
        let err = remove_worktree(&clone, &worktree, "git worktree remove (exp/A-1)");
        fs::set_permissions(&stuck, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(err.is_err());
        assert!(worktree.join(".git").exists());
        assert_eq!(classify(&clone, &worktree, &refs()), Leftover::Worktree);
        assert!(worktree.join("README.md").exists(), "tracked files restored");
        assert_eq!(git(&worktree, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "exp/A-1");
    }
}
