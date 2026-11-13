//! #EXP-1617 (Q6): this daemon ALSO serves GitHub Actions as a self-hosted
//! runner for the team repositories that opted in (`ci.actionsRunners`). Per
//! repository it keeps one OFFICIAL `actions/runner` (pinned version, checked
//! by sha256) in `{data_dir}/actions-runner/<repositoryId>`, configured
//! `--ephemeral --replace` (one job, then a fresh registration with a fresh
//! token) and supervised in its own process group. A repository that drops
//! out (the switch off, the device's runner off) is deregistered with a
//! removal token and its directory removed.
//!
//! The reconcile loop is generic over [`RunnerHost`] (the server) and
//! [`RunnerDist`] (where a runner's files come from), so it is tested with
//! stub scripts and no network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// The device cap of a build that serves Actions.
pub const ACTIONS_RUNNER_CAP: &str = "actions-runner";
/// The pinned `actions/runner` release (GitHub's own asset digests).
pub const RUNNER_VERSION: &str = "2.338.2";
const RUNNER_SHA256: &[(&str, &str)] = &[
    ("linux-x64", "baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40"),
    ("linux-arm64", "670adb2f268fe21dcbb58b3db0e0aa9d5b3bddfe2c651481336e561ff86de945"),
    ("osx-x64", "109fff505a34014a7e8d1acb850c33d404811102f572b8fb9788e2fff013cc49"),
    ("osx-arm64", "25a065af409ffa317ab4d5a3b614dd15fc9c40f2172bc1b35a1b1258e919a388"),
];
/// How often the targets are re-read.
const RECONCILE_EVERY: Duration = Duration::from_secs(300);
/// The wait after a failed configure/run, doubling to the cap.
const BACKOFF_START: Duration = Duration::from_secs(30);
const BACKOFF_CAP: Duration = Duration::from_secs(600);

/// One repository to serve (`api::ci::ActionsRunnerTarget`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub repository_id: String,
    pub url: String,
    pub labels: Vec<String>,
    pub name: String,
}

/// The server's side.
pub trait RunnerHost {
    fn targets(&self) -> Result<Vec<Target>, String>;
    /// `kind` = `registration` | `remove`.
    fn token(&self, repository_id: &str, kind: &str) -> Result<String, String>;
}

/// How `config.sh` takes its `--token` without argv (actions/runner reads
/// `ACTIONS_RUNNER_INPUT_<arg>` and removes it from its environment).
const TOKEN_ENV: &str = "ACTIONS_RUNNER_INPUT_TOKEN";

/// Puts a runner's files (config.sh, run.sh, …) into an empty directory.
pub trait RunnerDist {
    fn install(&self, dir: &Path) -> Result<(), String>;
}

/// This machine's `actions/runner` asset name (`linux-x64`, `osx-arm64`, …).
pub fn platform_asset() -> Option<&'static str> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "osx",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    RUNNER_SHA256.iter().find(|(a, _)| *a == format!("{os}-{arch}")).map(|(a, _)| *a)
}

/// The official release: downloaded once into `cache`, checked, unpacked per runner.
pub struct OfficialDist {
    pub cache: PathBuf,
}

impl RunnerDist for OfficialDist {
    fn install(&self, dir: &Path) -> Result<(), String> {
        let asset = platform_asset().ok_or("no actions/runner build for this machine")?;
        let expected = RUNNER_SHA256.iter().find(|(a, _)| *a == asset).map(|(_, h)| *h).unwrap_or_default();
        let file = format!("actions-runner-{asset}-{RUNNER_VERSION}.tar.gz");
        let tarball = self.cache.join(&file);
        if !tarball.exists() {
            let url = format!("https://github.com/actions/runner/releases/download/v{RUNNER_VERSION}/{file}");
            updater::download(&url, &tarball, |_, _| {}).map_err(|e| format!("download {file}: {e}"))?;
        }
        let bytes = std::fs::read(&tarball).map_err(|e| format!("read {file}: {e}"))?;
        let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if actual != expected {
            let _ = std::fs::remove_file(&tarball);
            return Err(format!("{file}: sha256 {actual}, expected {expected}"));
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let status = Command::new("tar").arg("-xzf").arg(&tarball).arg("-C").arg(dir).status().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("tar exited with {status}"));
        }
        Ok(())
    }
}

struct Served {
    target: Target,
    dir: PathBuf,
    child: Option<Child>,
    retry_at: Option<Instant>,
    backoff: Duration,
}

/// The per-repository runners of this device.
pub struct Supervisor<H, D> {
    host: H,
    dist: D,
    root: PathBuf,
    served: BTreeMap<String, Served>,
}

fn spawn_group(command: &mut Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: signals the runner's own process group (spawned with process_group(0)).
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    #[cfg(unix)]
    // SAFETY: as above.
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl<H: RunnerHost, D: RunnerDist> Supervisor<H, D> {
    pub fn new(host: H, dist: D, root: PathBuf) -> Self {
        Self { host, dist, root, served: BTreeMap::new() }
    }

    /// The repositories with a live runner process.
    pub fn running(&self) -> Vec<String> {
        self.served.iter().filter(|(_, s)| s.child.is_some()).map(|(id, _)| id.clone()).collect()
    }

    /// One pass: read the targets (a failed read changes nothing), drop the
    /// ones that left, then (re)start what is due.
    pub fn reconcile(&mut self) {
        let targets = match self.host.targets() {
            Ok(targets) => targets,
            Err(err) => {
                log::debug!("actions runners: targets unreadable ({err}); keeping the current set");
                return self.tick();
            }
        };
        let wanted: BTreeMap<String, Target> = targets.into_iter().map(|t| (t.repository_id.clone(), t)).collect();
        let gone: Vec<String> = self.served.keys().filter(|id| !wanted.contains_key(*id)).cloned().collect();
        for id in gone {
            if let Some(served) = self.served.remove(&id) {
                self.deregister(served);
            }
        }
        // Directories of runners a previous daemon left behind (it died
        // before deregistering) and that are no longer wanted.
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if id != "dist" && !wanted.contains_key(&id) && entry.path().join("config.sh").exists() {
                    let dir = entry.path();
                    self.remove_registration(&id, &dir);
                    let _ = std::fs::remove_dir_all(&dir);
                }
            }
        }
        for (id, target) in wanted {
            let dir = self.root.join(&id);
            let served = self.served.entry(id).or_insert_with(|| Served {
                target: target.clone(),
                dir,
                child: None,
                retry_at: None,
                backoff: BACKOFF_START,
            });
            served.target = target;
        }
        self.tick();
    }

    /// Restart every runner whose process ended (an ephemeral job finished)
    /// once its backoff has passed.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let ids: Vec<String> = self.served.keys().cloned().collect();
        for id in ids {
            let served = self.served.get_mut(&id).unwrap();
            if let Some(child) = served.child.as_mut() {
                match child.try_wait() {
                    Ok(None) => continue,
                    Ok(Some(status)) => {
                        log::info!("actions runner {}: run.sh ended ({status})", served.target.name);
                        served.child = None;
                        if !status.success() {
                            served.retry_at = Some(now + served.backoff);
                            served.backoff = (served.backoff * 2).min(BACKOFF_CAP);
                        }
                    }
                    Err(_) => served.child = None,
                }
            }
            if served.retry_at.is_some_and(|at| now < at) {
                continue;
            }
            let target = served.target.clone();
            let dir = served.dir.clone();
            match self.start(&target, &dir) {
                Ok(child) => {
                    let served = self.served.get_mut(&id).unwrap();
                    served.child = Some(child);
                    served.retry_at = None;
                }
                Err(err) => {
                    log::warn!("actions runner {}: {err}", target.name);
                    let served = self.served.get_mut(&id).unwrap();
                    served.retry_at = Some(now + served.backoff);
                    served.backoff = (served.backoff * 2).min(BACKOFF_CAP);
                }
            }
        }
    }

    fn start(&self, target: &Target, dir: &Path) -> Result<Child, String> {
        if !dir.join("config.sh").exists() {
            self.dist.install(dir)?;
        }
        let token = self.host.token(&target.repository_id, "registration")?;
        // The token rides the environment (`ACTIONS_RUNNER_INPUT_TOKEN`, which
        // the runner reads and clears), never argv: argv is world-readable in
        // the process list (the dogfood saw it in `ps`).
        let status = Command::new(dir.join("config.sh"))
            .current_dir(dir)
            .env(TOKEN_ENV, &token)
            .args(["--unattended", "--ephemeral", "--replace", "--disableupdate"])
            .args(["--url", &target.url, "--name", &target.name])
            .args(["--labels", &target.labels.join(","), "--work", "_work"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("config.sh: {e}"))?;
        if !status.success() {
            return Err(format!("config.sh exited with {status}"));
        }
        let mut run = Command::new(dir.join("run.sh"));
        run.current_dir(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        spawn_group(&mut run).map_err(|e| format!("run.sh: {e}"))
    }

    fn remove_registration(&self, repository_id: &str, dir: &Path) {
        if !dir.join(".runner").exists() {
            return;
        }
        match self.host.token(repository_id, "remove") {
            Ok(token) => {
                let _ = Command::new(dir.join("config.sh"))
                    .current_dir(dir)
                    .env(TOKEN_ENV, &token)
                    .arg("remove")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            Err(err) => log::warn!("actions runner {repository_id}: no removal token ({err})"),
        }
    }

    fn deregister(&self, mut served: Served) {
        if let Some(child) = served.child.as_mut() {
            kill_group(child);
        }
        self.remove_registration(&served.target.repository_id, &served.dir);
        let _ = std::fs::remove_dir_all(&served.dir);
    }

    /// Stops every runner (the daemon exits); registrations stay (an
    /// ephemeral runner GitHub drops on its own, and the next daemon
    /// reconciles leftovers).
    pub fn stop(&mut self) {
        for served in self.served.values_mut() {
            if let Some(child) = served.child.as_mut() {
                kill_group(child);
            }
            served.child = None;
        }
    }
}

/// The supervisor against the live server, on its own thread.
pub fn spawn(ctx: std::sync::Arc<crate::context::Ctx>) {
    struct Live(std::sync::Arc<crate::context::Ctx>, String);
    impl RunnerHost for Live {
        fn targets(&self) -> Result<Vec<Target>, String> {
            api::ci::actions_runners(&self.0.trpc, &self.1)
                .map(|ts| {
                    ts.into_iter()
                        .map(|t| Target { repository_id: t.repository_id, url: t.url, labels: t.labels, name: t.name })
                        .collect()
                })
                .map_err(|e| e.to_string())
        }
        fn token(&self, repository_id: &str, kind: &str) -> Result<String, String> {
            api::ci::actions_runner_token(&self.0.trpc, &self.1, repository_id, kind).map_err(|e| e.to_string())
        }
    }
    if platform_asset().is_none() {
        return;
    }
    let root = ctx.data_dir.join("actions-runner");
    let dist = OfficialDist { cache: root.join("dist") };
    let device_id = ctx.device_id();
    let spawned = std::thread::Builder::new().name("exp-actions-runner".into()).spawn(move || {
        let mut supervisor = Supervisor::new(Live(ctx, device_id), dist, root);
        let mut last = Instant::now() - RECONCILE_EVERY;
        loop {
            if last.elapsed() >= RECONCILE_EVERY {
                supervisor.reconcile();
                last = Instant::now();
            } else {
                supervisor.tick();
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
    if let Err(err) = spawned {
        log::warn!("actions runner: could not start its thread: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct FakeHost {
        targets: Rc<RefCell<Result<Vec<Target>, String>>>,
        tokens: Rc<RefCell<Vec<String>>>,
    }
    impl RunnerHost for FakeHost {
        fn targets(&self) -> Result<Vec<Target>, String> {
            self.targets.borrow().clone()
        }
        fn token(&self, repository_id: &str, kind: &str) -> Result<String, String> {
            self.tokens.borrow_mut().push(format!("{kind}:{repository_id}"));
            Ok(format!("tok-{kind}"))
        }
    }

    /// Stub scripts: config.sh logs its args and writes `.runner` (remove
    /// deletes it); run.sh sleeps, or exits at once when asked.
    struct StubDist {
        log: PathBuf,
    }
    impl RunnerDist for StubDist {
        fn install(&self, dir: &Path) -> Result<(), String> {
            use std::os::unix::fs::PermissionsExt;
            std::fs::create_dir_all(dir).unwrap();
            let log = self.log.display();
            let config = format!(
                "#!/bin/sh\necho \"config $* token=$ACTIONS_RUNNER_INPUT_TOKEN\" >> {log}\nif [ \"$1\" = remove ]; then rm -f .runner; else touch .runner; fi\n"
            );
            let run = format!("#!/bin/sh\necho run >> {log}\n[ -f exit-now ] && exit 0\nexec sleep 30\n");
            for (name, body) in [("config.sh", config), ("run.sh", run)] {
                std::fs::write(dir.join(name), body).unwrap();
                std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            Ok(())
        }
    }

    fn target(id: &str) -> Target {
        Target {
            repository_id: id.into(),
            url: format!("https://github.com/o/{id}"),
            labels: vec!["self-hosted".into(), "linux".into(), "exp-linux".into()],
            name: format!("mint-{id}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn registers_reruns_ephemerally_and_deregisters() {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("exp-actions-runner-{}-{n}", std::process::id()));
        let log = root.join("log.txt");
        std::fs::create_dir_all(&root).unwrap();
        let targets = Rc::new(RefCell::new(Ok(vec![target("r1")])));
        let tokens = Rc::new(RefCell::new(vec![]));
        let host = FakeHost { targets: Rc::clone(&targets), tokens: Rc::clone(&tokens) };
        let mut sup = Supervisor::new(host, StubDist { log: log.clone() }, root.join("runners"));

        // Each ephemeral job ends cleanly (run.sh exits 0 at once here).
        std::fs::create_dir_all(root.join("runners/r1")).unwrap();
        StubDist { log: log.clone() }.install(&root.join("runners/r1")).unwrap();
        std::fs::write(root.join("runners/r1/exit-now"), "").unwrap();
        sup.reconcile();
        assert!(sup.served.contains_key("r1"));
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains(
            "config --unattended --ephemeral --replace --disableupdate --url https://github.com/o/r1 --name mint-r1 --labels self-hosted,linux,exp-linux --work _work token=tok-registration"
        ), "{text}");
        // The job ended: the next tick registers afresh and runs again.
        std::thread::sleep(Duration::from_millis(300));
        sup.tick();
        assert_eq!(tokens.borrow().iter().filter(|t| t.starts_with("registration")).count(), 2);
        // A long job keeps running between ticks.
        std::fs::remove_file(root.join("runners/r1/exit-now")).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        sup.tick();
        sup.tick();
        assert_eq!(sup.running(), ["r1"]);

        // A failed targets read keeps the runner.
        *targets.borrow_mut() = Err("offline".into());
        sup.reconcile();
        assert!(sup.served.contains_key("r1"));

        // Switched off: deregistered with a removal token, directory gone.
        *targets.borrow_mut() = Ok(vec![]);
        sup.reconcile();
        assert!(sup.served.is_empty());
        assert!(tokens.borrow().contains(&"remove:r1".to_string()));
        assert!(std::fs::read_to_string(&log).unwrap().contains("config remove token=tok-remove"));
        assert!(!root.join("runners/r1").exists());
        sup.stop();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn this_machine_has_a_pinned_asset() {
        if cfg!(any(target_os = "linux", target_os = "macos")) && cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
            assert!(platform_asset().is_some());
        }
    }
}
