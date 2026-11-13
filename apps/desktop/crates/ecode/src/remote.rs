//! `Remote`: where objects go. Credentials come from a [`CredentialSource`]
//! (the server's `repositories.credentials`, a GitHub token, a keyring);
//! fetch and push run on git argv with a helper scoped to exactly this
//! remote (`credential_helper_key`, `remote.json`). The token reaches git
//! through the child's environment, never its argv or the repo config.

use std::path::Path;

use crate::{CredentialSource, Credentials, GitCommand, Result};

/// One ref update to push: `src` (a sha or local ref, empty = delete) to `dst`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushSpec {
    pub src: String,
    pub dst: String,
    /// Overwrite a non-fast-forward (`+src:dst`).
    pub force: bool,
}

impl PushSpec {
    pub fn branch(src: impl Into<String>, branch: &str, force: bool) -> Self {
        Self { src: src.into(), dst: format!("refs/heads/{branch}"), force }
    }

    fn refspec(&self) -> String {
        format!("{}{}:{}", if self.force { "+" } else { "" }, self.src, self.dst)
    }
}

/// Where objects go: credentials, fetch, push.
pub trait Remote: Send + Sync {
    /// The tokenless URL (`https://…` or `exp://team/repo`).
    fn url(&self) -> String;
    /// Credentials fresh enough for one network op (`None` = anonymous).
    fn credentials(&self) -> Result<Option<Credentials>>;
    /// Fetches `refspecs` into the repository at `repo`.
    fn fetch(&self, repo: &Path, refspecs: &[&str]) -> Result<()>;
    /// Pushes ref updates from the repository at `repo`, all or nothing.
    fn push(&self, repo: &Path, updates: &[PushSpec]) -> Result<()>;
}

/// A git remote reached with git's own transports. `exp://` URLs go through
/// `git-remote-exp` (shipped with the CLI, #EXP-1616), which authenticates
/// on its own; https gets the scoped helper.
pub struct GitRemote<C> {
    url: String,
    source: Option<C>,
}

impl GitRemote<Credentials> {
    /// An anonymous remote (a local path, a public URL).
    pub fn anonymous(url: impl Into<String>) -> Self {
        Self { url: url.into(), source: None }
    }
}

impl<C: CredentialSource> GitRemote<C> {
    /// A remote whose URL and credentials come from `source`.
    pub fn new(source: C) -> Result<Self> {
        let url = source.credentials()?.remote_url().to_string();
        Ok(Self { url, source: Some(source) })
    }

    fn git(&self, repo: &Path) -> Result<GitCommand> {
        let mut command = GitCommand::new(Some(repo));
        let Some(credentials) = self.credentials()? else { return Ok(command) };
        if let (Some(helper), Some(use_path)) = (credentials.helper_key(), credentials.use_http_path_key()) {
            command = command
                .args(["-c", &format!("{use_path}=true")])
                // The empty entry resets inherited helpers for THIS remote only.
                .args(["-c", &format!("{helper}=")])
                .args([
                    "-c",
                    &format!(
                        "{helper}=!f() {{ test \"$1\" = get && printf 'username=%s\\npassword=%s\\n' \"$ECODE_GIT_USERNAME\" \"$ECODE_GIT_TOKEN\"; :; }}; f"
                    ),
                ])
                .env("ECODE_GIT_USERNAME", credentials.username())
                .env("ECODE_GIT_TOKEN", credentials.token());
        }
        Ok(command.scrub(credentials.token()))
    }
}

impl<C: CredentialSource> GitRemote<C> {
    /// Makes the remote's branches and tags EQUAL this repository's: every
    /// one pushed forced, and every one the repository lacks deleted
    /// (`--prune`). A mirror push (#EXP-1616).
    pub fn push_mirror(&self, repo: &Path) -> Result<()> {
        self.git(repo)?
            .args(["push", "--force", "--prune", "--porcelain", &self.url])
            .args(["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"])
            .run()?;
        Ok(())
    }
}

impl<C: CredentialSource> Remote for GitRemote<C> {
    fn url(&self) -> String {
        self.url.clone()
    }

    fn credentials(&self) -> Result<Option<Credentials>> {
        self.source.as_ref().map(CredentialSource::credentials).transpose()
    }

    fn fetch(&self, repo: &Path, refspecs: &[&str]) -> Result<()> {
        self.git(repo)?.args(["fetch", "--no-tags", "--quiet", &self.url]).args(refspecs.iter().copied()).run()?;
        Ok(())
    }

    fn push(&self, repo: &Path, updates: &[PushSpec]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let refspecs: Vec<String> = updates.iter().map(PushSpec::refspec).collect();
        self.git(repo)?.args(["push", "--atomic", "--porcelain", &self.url]).args(refspecs).run()?;
        Ok(())
    }
}
