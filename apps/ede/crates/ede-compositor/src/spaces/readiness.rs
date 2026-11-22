//! #EXP-1514 (#EXP-2011, the flip): `space-readiness`. The hello of every
//! compositor that makes agent spaces says it, and
//! [`ede_protocol::Request::SpaceReadiness`] answers from what holds NOW,
//! in this order:
//!
//! 1. **The person's systemd user manager** ([`super::slice::Units`], found
//!    at the session's start): `systemd-run`, `busctl`, `systemctl`,
//!    systemd's manager (`/usr/lib/systemd/systemd`) and `dbus-daemon` are
//!    still there, the manager's private socket
//!    (`$XDG_RUNTIME_DIR/systemd/private`) is still there, and the memory
//!    and pids controllers are delegated to it (`user@<uid>.service`'s
//!    `cgroup.controllers`). Else `no_user_manager`.
//! 2. **The space guard and polkit** ([`super::guard`]): installed, its
//!    probe answered, polkit refusing a space's process. Else `guard`.
//! 3. **The latch**: once a space's session failed to start (its manager
//!    could not start, or ended), `session` until a session start
//!    succeeds (a manager that runs and whose bus is there). What failed
//!    is refused, never started outside a scope: a compositor whose
//!    launches run in scopes never starts one in the person's login session
//!    ([`super::slice`]), and one that loses its user manager answers
//!    `unavailable`.
//!
//! **`signals: open`** (#EDE-157) rides along, never a block: the guard
//! answered that a space's signals to the person's processes stay open
//! here (the kernel runs no BPF LSM programs, or the guard is an older
//! one).
//!
//! There is no `driver` block: on every graphics driver a space's apps
//! start, on the GPU where the driver is proven and in software elsewhere
//! (#EDE-162; an app whose GPU path fails latches its space back to
//! software).
//!
//! Every answer that differs from the one before is logged with its cause;
//! ePilot hands the block on (`exponential doctor` names it).

use std::path::Path;

use super::slice::Units;

/// The controllers a space's slices need from the user manager: the
/// memory limits and the task limit.
pub const DELEGATED: [&str; 2] = ["memory", "pids"];

/// Why a space is not offered now: the answer's word and its cause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// `no_user_manager`, `guard` or `session`.
    pub word: &'static str,
    /// What the log says.
    pub cause: String,
}

impl Block {
    fn new(word: &'static str, cause: impl Into<String>) -> Self {
        Self { word, cause: cause.into() }
    }
}

/// `cgroup.controllers`' text: `Err` names the controllers that are not
/// delegated.
pub fn delegated(controllers: &str) -> Result<(), String> {
    let have: Vec<&str> = controllers.split_whitespace().collect();
    let missing: Vec<&str> = DELEGATED.into_iter().filter(|wanted| !have.contains(wanted)).collect();
    match missing.as_slice() {
        [] => Ok(()),
        [one] => Err(format!("the user manager is not given the {one} controller")),
        more => Err(format!("the user manager is not given the {} controllers", more.join(" and "))),
    }
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The user manager's facts, as they hold now: a few `stat`s and one small
/// file, so it may run in the event loop.
pub fn user_manager_now(units: &Units) -> Result<(), String> {
    let tools = [("systemd-run", &units.systemd_run), ("busctl", &units.busctl), ("systemctl", &units.systemctl), ("systemd's manager", &units.manager), ("dbus-daemon", &units.dbus_daemon)];
    if let Some((name, path)) = tools.into_iter().find(|(_, path)| !executable(path)) {
        return Err(format!("{name} is not there ({})", path.display()));
    }
    if !units.reachable() {
        return Err(format!("no systemd user manager in {}", units.runtime.display()));
    }
    // `…/user@<uid>.service/app.slice` (the probe's scope said so): the
    // user manager's own cgroup is its parent.
    let manager = units.app_slice.parent().unwrap_or(&units.app_slice);
    let file = manager.join("cgroup.controllers");
    let controllers = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    delegated(&controllers)
}

/// The answer (pure): the user manager as found at the session's start
/// (`Err` = why none was) and now, the guard's verdict, the latch.
pub fn answer(units: Result<&Units, &str>, guard: Result<(), String>, latch: Option<&str>) -> Result<(), Block> {
    let units = units.map_err(|why| Block::new("no_user_manager", why))?;
    user_manager_now(units).map_err(|why| Block::new("no_user_manager", why))?;
    guard.map_err(|why| Block::new("guard", why))?;
    match latch {
        Some(why) => Err(Block::new("session", why)),
        None => Ok(()),
    }
}

impl crate::state::Ede {
    /// The hello says `space-readiness`: every compositor that makes spaces.
    pub fn says_space_readiness(&self) -> bool {
        self.spaces.dir.is_some()
    }

    /// The flip's answer, with its cause.
    pub fn space_readiness_why(&self) -> Result<(), Block> {
        let units = self.spaces.units.as_ref().ok_or(self.spaces.units_why.as_deref().unwrap_or("no systemd user manager was found at the session's start"));
        answer(units, self.space_guard_verdict(), self.spaces.session_failed.as_deref())
    }

    /// The flip's answer: `Err` = the block's word.
    pub fn space_readiness(&self) -> Result<(), &'static str> {
        self.space_readiness_why().map_err(|block| block.word)
    }

    /// `space_readiness`'s reply; an answer that differs from the last is
    /// logged with its cause.
    pub(crate) fn space_readiness_reply(&mut self) -> ede_protocol::SpaceReadiness {
        let now = self.space_readiness_why();
        if self.spaces.readiness_said.as_ref() != Some(&now) {
            match &now {
                Ok(()) => tracing::info!("agent spaces are offered: this desktop is ready for them"),
                Err(block) => tracing::warn!("agent spaces are not offered ({}): {}", block.word, block.cause),
            }
            self.spaces.readiness_said = Some(now.clone());
        }
        // #EDE-157: spaces are offered where the guard cannot refuse their
        // signals; the answer says so.
        let signals = self.space_signals_open().then(|| "open".to_string());
        ede_protocol::SpaceReadiness { ready: now.is_ok(), block: now.err().map(|block| block.word.to_string()), max: Some(self.space_max() as u32), signals }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    struct Fake {
        _dir: tempfile::TempDir,
        units: Units,
    }

    /// Every tool an executable file, the private socket, a manager's
    /// cgroup with `controllers`.
    fn fake(controllers: &str) -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let tool = |name: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let runtime = dir.path().join("runtime");
        std::fs::create_dir_all(runtime.join("systemd")).unwrap();
        std::fs::write(runtime.join("systemd/private"), "").unwrap();
        let manager = dir.path().join("user@1000.service");
        std::fs::create_dir_all(manager.join("app.slice")).unwrap();
        std::fs::write(manager.join("cgroup.controllers"), controllers).unwrap();
        let units = Units {
            systemd_run: tool("systemd-run"),
            busctl: tool("busctl"),
            systemctl: tool("systemctl"),
            version: 255,
            shim: "/proc/self/exe".into(),
            runtime,
            bus: None,
            compositor: 41,
            machine: super::super::slice::Machine { cpus: 8, memory: 16 << 30 },
            app_slice: manager.join("app.slice"),
            manager: tool("systemd"),
            dbus_daemon: tool("dbus-daemon"),
            guard: None,
        };
        Fake { _dir: dir, units }
    }

    fn word(result: Result<(), Block>) -> Result<(), &'static str> {
        result.map_err(|block| block.word)
    }

    #[test]
    fn ready_when_the_user_manager_the_guard_and_the_latch_say_so() {
        let fake = fake("cpuset cpu io memory pids\n");
        assert_eq!(answer(Ok(&fake.units), Ok(()), None), Ok(()));
        assert_eq!(word(answer(Err("systemd 249 is older than 252"), Ok(()), None)), Err("no_user_manager"));
        assert_eq!(answer(Err("systemd 249 is older than 252"), Ok(()), None).unwrap_err().cause, "systemd 249 is older than 252");
        assert_eq!(word(answer(Ok(&fake.units), Err("the space guard is not installed".into()), None)), Err("guard"));
        let latched = answer(Ok(&fake.units), Ok(()), Some("space-1: its user manager ended"));
        assert_eq!(latched, Err(Block { word: "session", cause: "space-1: its user manager ended".into() }));
        // The user manager first, the guard second, the latch last.
        assert_eq!(word(answer(Err("none"), Err("no guard".into()), Some("latched"))), Err("no_user_manager"));
        assert_eq!(word(answer(Ok(&fake.units), Err("no guard".into()), Some("latched"))), Err("guard"));
    }

    #[test]
    fn the_user_manager_is_asked_again_at_every_answer() {
        let fake = fake("memory pids");
        assert_eq!(user_manager_now(&fake.units), Ok(()));
        // Each tool, gone or no longer a program.
        let paths: [fn(&Units) -> &PathBuf; 5] = [|u| &u.systemd_run, |u| &u.busctl, |u| &u.systemctl, |u| &u.manager, |u| &u.dbus_daemon];
        for path in paths {
            let path = path(&fake.units).clone();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(user_manager_now(&fake.units).unwrap_err().contains("is not there"), "{}", path.display());
            std::fs::remove_file(&path).unwrap();
            assert!(user_manager_now(&fake.units).unwrap_err().contains(&path.display().to_string()));
            std::fs::write(&path, "").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(user_manager_now(&fake.units), Ok(()));
        // Its private socket gone: the user manager has ended.
        std::fs::remove_file(fake.units.runtime.join("systemd/private")).unwrap();
        assert_eq!(user_manager_now(&fake.units), Err(format!("no systemd user manager in {}", fake.units.runtime.display())));
        std::fs::write(fake.units.runtime.join("systemd/private"), "").unwrap();
        // Not delegated: the limits of a space's slice would not hold.
        let controllers = fake.units.app_slice.parent().unwrap().join("cgroup.controllers");
        std::fs::write(&controllers, "cpu memory").unwrap();
        assert_eq!(user_manager_now(&fake.units), Err("the user manager is not given the pids controller".into()));
        std::fs::remove_file(&controllers).unwrap();
        assert!(user_manager_now(&fake.units).unwrap_err().contains("cgroup.controllers"));
    }

    #[test]
    fn memory_and_pids_must_be_delegated() {
        assert_eq!(delegated("cpuset cpu io memory hugetlb pids rdma misc\n"), Ok(()));
        assert_eq!(delegated("pids memory"), Ok(()));
        assert_eq!(delegated("cpu pids"), Err("the user manager is not given the memory controller".into()));
        assert_eq!(delegated(""), Err("the user manager is not given the memory and pids controllers".into()));
        assert_eq!(delegated("memoryx pids"), Err("the user manager is not given the memory controller".into()), "a word, not a prefix");
    }
}
