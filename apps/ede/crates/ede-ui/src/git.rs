//! The ui directory's history: the git CLI in that directory. Every applied
//! edit is one commit, undo is `git revert`.

use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde::{Deserialize, Serialize};

/// The first commit of a fresh ui directory (never reverted).
pub const INITIAL_MESSAGE: &str = "exponential-ui: history starts";

/// How long a git command waits for another one's lock (`index.lock`,
/// `HEAD.lock`, a ref's: #EDE-153): 40 × 50 ms.
const INDEX_LOCK_TRIES: u32 = 40;
const INDEX_LOCK_WAIT: std::time::Duration = std::time::Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub summary: String,
    /// Seconds since the epoch.
    pub time: i64,
}

#[derive(Debug, Clone)]
pub struct Repo {
    dir: PathBuf,
}

fn failed(what: &str, out: &Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&out.stderr);
    io::Error::other(format!("git {what}: {}", stderr.trim()))
}

impl Repo {
    /// The repository in `dir`, created (with its first commit) when there
    /// is none. Only `dir/.git` counts: a ui directory inside some other
    /// checkout keeps its own history.
    pub fn open(dir: &Path) -> io::Result<Repo> {
        let repo = Repo { dir: dir.to_path_buf() };
        if !dir.join(".git").exists() {
            repo.ok(&["init", "--quiet"], "init")?;
            repo.ok(&["commit", "--quiet", "--allow-empty", "-m", INITIAL_MESSAGE], "commit")?;
        }
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> io::Result<Output> {
        let mut git = Command::new("git");
        git.arg("-C")
            .arg(&self.dir)
            // Commits work without a global identity, and a person's
            // signing or hooks setup never blocks a shell edit.
            .args(["-c", "user.name=Exponential Desktop", "-c", "user.email=exponential-desktop@localhost", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE");
        // Another process's git holds the index or a ref for a moment
        // (`exponential-ui set` or `undo` against the supervisor's reload):
        // wait for it. Its path names the lock in any language git speaks.
        // A reload whose commit still fails is tried again (#EDE-153).
        let mut tries = 0;
        loop {
            let out = git.output()?;
            if out.status.success() || tries == INDEX_LOCK_TRIES || !String::from_utf8_lossy(&out.stderr).contains(".lock") {
                return Ok(out);
            }
            tries += 1;
            std::thread::sleep(INDEX_LOCK_WAIT);
        }
    }

    fn ok(&self, args: &[&str], what: &str) -> io::Result<Output> {
        let out = self.git(args)?;
        if out.status.success() {
            Ok(out)
        } else {
            Err(failed(what, &out))
        }
    }

    /// `HEAD:<file>`, `None` when HEAD does not have it.
    pub fn show_head(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
        self.show_at("HEAD", file)
    }

    /// `<id>:<file>`, `None` when that commit does not have it.
    pub fn show_at(&self, id: &str, file: &str) -> io::Result<Option<Vec<u8>>> {
        let out = self.git(&["show", &format!("{id}:{file}")])?;
        Ok(out.status.success().then_some(out.stdout))
    }

    /// `<file>` as the newest commit whose subject is exactly `subject`
    /// left it; `None` when there is no such commit.
    pub fn show_at_newest(&self, file: &str, subject: &str) -> io::Result<Option<Vec<u8>>> {
        let out = self.ok(&["log", "--format=%H%x09%s", "--", file], "log")?;
        let id = String::from_utf8_lossy(&out.stdout).lines().find_map(|line| {
            let (id, s) = line.split_once('\t')?;
            (s == subject).then(|| id.to_string())
        });
        let Some(id) = id else { return Ok(None) };
        let out = self.git(&["show", &format!("{id}:{file}")])?;
        Ok(out.status.success().then_some(out.stdout))
    }

    /// `file` as every commit with exactly `subject` left it, newest first
    /// (#EDE-202: each factory copy the directory ever had).
    pub fn show_all_at(&self, file: &str, subject: &str) -> io::Result<Vec<Vec<u8>>> {
        let out = self.ok(&["log", "--format=%H%x09%s", "--", file], "log")?;
        let ids: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.split_once('\t').filter(|(_, s)| *s == subject).map(|(id, _)| id.to_string()))
            .collect();
        let mut all = Vec::new();
        for id in ids {
            let out = self.git(&["show", &format!("{id}:{file}")])?;
            if out.status.success() {
                all.push(out.stdout);
            }
        }
        Ok(all)
    }

    /// The newest commit that touched `file`, `None` when none did.
    pub fn newest_of(&self, file: &str) -> io::Result<Option<Commit>> {
        let out = self.ok(&["log", "-1", "--format=%H%x09%ct%x09%s", "--", file], "log")?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().next().and_then(|line| {
            let mut parts = line.splitn(3, '\t');
            let id = parts.next()?.to_string();
            let time = parts.next()?.parse().ok()?;
            Some(Commit { id, summary: parts.next().unwrap_or("").to_string(), time })
        }))
    }

    /// Commit exactly `files` (added, changed or deleted in the working
    /// tree) with `message`. `None` when nothing differed from HEAD.
    pub fn commit(&self, files: &[String], message: &str) -> io::Result<Option<String>> {
        if files.is_empty() {
            return Ok(None);
        }
        let mut add = vec!["add", "--all", "--"];
        add.extend(files.iter().map(String::as_str));
        self.ok(&add, "add")?;
        let mut diff = vec!["diff", "--cached", "--quiet", "HEAD", "--"];
        diff.extend(files.iter().map(String::as_str));
        if self.git(&diff)?.status.success() {
            return Ok(None);
        }
        let mut commit = vec!["commit", "--quiet", "-m", message, "--only", "--"];
        commit.extend(files.iter().map(String::as_str));
        let out = self.git(&commit)?;
        if !out.status.success() {
            // #EDE-153: another process committed the same change between the
            // diff and the commit (the supervisor's reload and `exponential-ui
            // set` on one file): nothing is left to commit, which is no error.
            if self.git(&diff)?.status.success() {
                return Ok(None);
            }
            return Err(failed("commit", &out));
        }
        self.head().map(Some)
    }

    pub fn head(&self) -> io::Result<String> {
        let out = self.ok(&["rev-parse", "HEAD"], "rev-parse")?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Newest first.
    pub fn log(&self, limit: usize) -> io::Result<Vec<Commit>> {
        let out = self.ok(&["log", &format!("-{limit}"), "--format=%H%x09%ct%x09%s"], "log")?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                let id = parts.next()?.to_string();
                let time = parts.next()?.parse().ok()?;
                let summary = parts.next().unwrap_or("").to_string();
                Some(Commit { id, summary, time })
            })
            .collect())
    }

    /// The files a commit touched.
    pub fn files_of(&self, id: &str) -> io::Result<Vec<String>> {
        let out = self.ok(&["show", "--format=", "--name-only", id], "show")?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
    }

    /// Revert `id` as a new commit, message `Undo: <summary>`. Refused (and
    /// nothing changes) when it does not apply cleanly. `None` = nothing to
    /// revert: its change is already undone (#EDE-116), no commit is made
    /// and no revert is left in progress.
    pub fn revert(&self, id: &str, summary: &str) -> io::Result<Option<String>> {
        let out = self.git(&["revert", "--no-commit", id])?;
        if !out.status.success() {
            let _ = self.git(&["revert", "--abort"]);
            return Err(failed("revert", &out));
        }
        if self.git(&["diff", "--cached", "--quiet", "HEAD"])?.status.success() {
            self.ok(&["revert", "--quit"], "revert --quit")?;
            return Ok(None);
        }
        let out = self.git(&["commit", "--quiet", "-m", &format!("Undo: {summary}")])?;
        if !out.status.success() {
            let _ = self.git(&["revert", "--abort"]);
            return Err(failed("commit", &out));
        }
        self.head().map(Some)
    }

    /// Put `file` back to HEAD's version (gone when HEAD does not have it).
    pub fn restore(&self, file: &str) -> io::Result<()> {
        if self.show_head(file)?.is_some() {
            self.ok(&["checkout", "HEAD", "--", file], "checkout")?;
        } else {
            match std::fs::remove_file(self.dir.join(file)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }
}
