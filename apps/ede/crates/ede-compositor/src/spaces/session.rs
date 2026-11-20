//! #EDE-138: a space's own systemd user manager, and its one session bus.
//!
//! At a space's first launch the compositor starts `systemd --user` in a
//! `Delegate=yes` scope of the space's slice (the configuration
//! `user@.service` itself runs in, one level deeper), with the space's
//! directory as its runtime directory and the units written here as the
//! only ones it can load. Its `dbus.service` is the space's bus. So `snap
//! run` finds `org.freedesktop.systemd1` on the space's bus, Ptyxis and
//! `systemd-run` find `<space dir>/systemd/private`, and every scope they
//! make is inside the space's slice. Nothing of the space reaches the
//! person's user manager or bus by its environment.
//!
//! Every launch of the space gets that bus (`DBUS_SESSION_BUS_ADDRESS`) and
//! waits in its own process until the bus answers (`space-run`): the
//! compositor's event loop waits for nothing. A manager that ends is
//! started once more; a space whose manager ends a second time closes,
//! and its session's next call reads "new and empty".

use std::{
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Child,
};

use ede_protocol::{ErrorCode, Failure};

use super::launch::Profile;
use crate::{desk::DeskId, state::Ede};

/// The units a space's manager can load, in `<space dir>/units`: no unit
/// of the distribution's or of the person's (`~/.config/systemd/user`).
const UNITS: &str = "units";
/// An empty directory: no generators (no xdg-autostart, no `environment.d`).
const NOTHING: &str = "nothing";
/// What the person's distribution has for a clean end of a user manager
/// (SIGTERM starts `exit.target`); copied when it is there, else the
/// manager is stopped by the slice's SIGKILL two seconds later.
const EXIT_UNITS: [&str; 3] = ["exit.target", "shutdown.target", "systemd-exit.service"];
const DISTRO_UNITS: [&str; 2] = ["/usr/lib/systemd/user", "/lib/systemd/user"];
/// Portals the space's own `xdg-desktop-portal` serves, and those it must
/// not: eDE's own backend would look for its compositor and fail, and a
/// space has no screen to share, no shortcut, no notification of its own.
const PORTALS: &str = "[preferred]
default=gtk
org.freedesktop.impl.portal.ScreenCast=none
org.freedesktop.impl.portal.Screenshot=none
org.freedesktop.impl.portal.RemoteDesktop=none
org.freedesktop.impl.portal.GlobalShortcuts=none
org.freedesktop.impl.portal.Inhibit=none
org.freedesktop.impl.portal.Notification=none
org.freedesktop.impl.portal.Background=none
";

/// A space's session: its manager, how often it was started, its bus.
pub(crate) struct Session {
    /// The manager's process (`space-run`, then `systemd-run`, then
    /// `systemd --user`): `None` = not started yet.
    child: Option<Child>,
    /// How many times it was started.
    starts: u32,
    /// The bus's abstract socket name (`/tmp/dbus-ede-<random>`: a strict
    /// snap's AppArmor profile admits `@/tmp/dbus-*` and no path of ours).
    abstract_name: String,
}

impl Session {
    pub(crate) fn new() -> Self {
        let mut random = [0u8; 8];
        if let Ok(mut urandom) = std::fs::File::open("/dev/urandom") {
            let _ = std::io::Read::read_exact(&mut urandom, &mut random);
        }
        let hex: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        Self { child: None, starts: 0, abstract_name: format!("/tmp/dbus-ede-{hex}") }
    }

    /// The bus every launch of space `dir` gets: the path first (Flatpak's
    /// proxy takes the first `path=`), the abstract socket second.
    pub(crate) fn address(&self, dir: &Path) -> String {
        format!("unix:path={};unix:abstract={}", dir.join("bus").display(), self.abstract_name)
    }

    /// Its manager is gone (or was never there); it is not waited for.
    pub(crate) fn take(&mut self) -> Option<Child> {
        self.child.take()
    }
}

/// `dbus.conf`: a whole session bus configuration, not an include of
/// `session.conf` (that one listens on a `unix:tmpdir` of its own). The
/// abstract socket, then the socket systemd hands over (`dbus.socket`).
pub fn dbus_config(abstract_name: &str) -> String {
    format!(
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:abstract={abstract_name}</listen>
  <listen>systemd:</listen>
  <auth>EXTERNAL</auth>
  <standard_session_servicedirs />
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
  <includedir>/usr/share/dbus-1/session.d</includedir>
  <includedir>/etc/dbus-1/session.d</includedir>
</busconfig>
"#
    )
}

/// The units of a space's manager, by name: the bus and what starts it.
pub fn unit_files(dbus_daemon: &Path, dbus_conf: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("default.target", "[Unit]\nDescription=An Exponential agent space\nWants=dbus.socket dbus.service\n".into()),
        // Every service needs it; the distribution's pulls in sockets.target
        // (PipeWire, gpg-agent, a keyring of the person's).
        ("basic.target", "[Unit]\nDescription=Basic (an agent space)\n".into()),
        ("dbus.socket", "[Unit]\nDescription=The space's session bus socket\n[Socket]\nListenStream=%t/bus\n".into()),
        (
            "dbus.service",
            // No --systemd-activation: what the bus activates starts by
            // its Exec= line, a child of this service, in the space's slice.
            format!(
                "[Unit]\nDescription=The space's session bus\nRequires=dbus.socket\n[Service]\nType=notify\nNotifyAccess=main\nExecStart={} --config-file={} --nofork --nopidfile --syslog-only\n",
                dbus_daemon.display(),
                dbus_conf.display()
            ),
        ),
    ]
}

/// Writes what space `dir`'s manager starts with: its units, the bus's
/// configuration, an empty generator directory, and the portals file in
/// the space's profile (`config` = its `XDG_CONFIG_HOME`).
pub(crate) fn prepare(dir: &Path, config: Option<&Path>, dbus_daemon: &Path, abstract_name: &str) -> std::io::Result<()> {
    let make = |path: &Path| std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path);
    let write = |path: &Path, text: &str| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        std::io::Write::write_all(&mut file, text.as_bytes())
    };
    let units = dir.join(UNITS);
    make(&units)?;
    make(&dir.join(NOTHING))?;
    // dbus-daemon refuses a services directory others may write to.
    make(&dir.join("dbus-1/services"))?;
    let conf = dir.join("dbus.conf");
    write(&conf, &dbus_config(abstract_name))?;
    for (name, text) in unit_files(dbus_daemon, &conf) {
        write(&units.join(name), &text)?;
    }
    for name in EXIT_UNITS {
        if let Some(text) = DISTRO_UNITS.iter().find_map(|distro| std::fs::read_to_string(Path::new(distro).join(name)).ok()) {
            write(&units.join(name), &text)?;
        }
    }
    if let Some(config) = config {
        let portals = config.join("xdg-desktop-portal");
        make(&portals)?;
        write(&portals.join("exponential-portals.conf"), PORTALS)?;
    }
    Ok(())
}

/// What a space's manager has on top of a launch's environment.
pub(crate) fn manager_environment(dir: &Path) -> [(&'static str, PathBuf); 3] {
    [("SYSTEMD_UNIT_PATH", dir.join(UNITS)), ("SYSTEMD_GENERATOR_PATH", dir.join(NOTHING)), ("SYSTEMD_ENVIRONMENT_GENERATOR_PATH", dir.join(NOTHING))]
}

impl Ede {
    /// Space `desk`'s session, started now if it has none (its first
    /// launch), or once more if its manager ended. A manager that ended
    /// after it was started twice closes the space: `Err`.
    pub(crate) fn space_session(&mut self, desk: DeskId) -> Result<(), Failure> {
        let Some(units) = self.spaces.units.clone() else { return Ok(()) };
        let Some(space) = self.spaces.get_mut(desk) else { return Err(Failure::new(ErrorCode::NotFound, "no such space")) };
        if let Some(child) = &mut space.session.child {
            if matches!(child.try_wait(), Ok(None)) {
                return Ok(());
            }
            space.session.child = None;
            if space.session.starts >= 2 {
                let name = space.id.as_str().to_string();
                tracing::warn!("{name}: its user manager ended twice; the space closes");
                self.close_space(desk);
                return Err(Failure::new(ErrorCode::Unavailable, "the space's session ended twice: the space was closed"));
            }
            tracing::warn!("{}: its user manager ended; it is started once more", space.id.as_str());
        }
        let dir = space.dir.clone();
        let abstract_name = space.session.abstract_name.clone();
        let starts = space.session.starts + 1;
        let space = self.spaces.get(desk).expect("there a moment ago");
        let config = Some(self.spaces.home_of(space).join("config"));
        let unavailable = |what: &str, e: &dyn std::fmt::Display| Failure::new(ErrorCode::Unavailable, &format!("{what}: {e}"));
        prepare(&dir, config.as_deref(), &units.dbus_daemon, &abstract_name).map_err(|e| unavailable("the space's session", &e))?;
        let inner = [units.manager.to_string_lossy().into_owned(), "--user".to_string()];
        let mut cmd = units.session_command(&self.config.spaces, desk.number(), starts, &dir, &inner);
        let environment = self.space_environment(desk, Profile::Space)?;
        cmd.env_clear().envs(environment).envs(manager_environment(&dir)).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        super::launch::own_group_dying_with_us(&mut cmd);
        let child = cmd.spawn().map_err(|e| unavailable("the space's user manager", &e))?;
        tracing::info!("{}: its user manager starts (pid {})", dir.file_name().unwrap_or_default().to_string_lossy(), child.id());
        let space = self.spaces.get_mut(desk).expect("there a moment ago");
        space.session.child = Some(child);
        space.session.starts = starts;
        Ok(())
    }

    /// Once a second: a space whose manager ended gets it once more (or
    /// closes); what closed spaces' managers left is reaped.
    pub(crate) fn space_sessions_tick(&mut self) {
        let ended: Vec<DeskId> = self
            .spaces
            .list
            .iter_mut()
            .filter_map(|space| space.session.child.as_mut().is_some_and(|child| !matches!(child.try_wait(), Ok(None))).then_some(space.desk.id))
            .collect();
        for desk in ended {
            let _ = self.space_session(desk);
        }
        self.spaces.ended_sessions.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
}

/// Where the manager's scope says it is: `exponential-p<pid>-s<n>-session<k>.scope`.
pub fn session_scope(compositor: u32, space: u32, start: u32) -> String {
    format!("exponential-p{compositor}-s{space}-session{start}.scope")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_space_bus_listens_on_its_own_sockets_only() {
        let conf = dbus_config("/tmp/dbus-ede-0011");
        assert!(conf.contains("<listen>unix:abstract=/tmp/dbus-ede-0011</listen>\n  <listen>systemd:</listen>"), "{conf}");
        assert_eq!(conf.matches("<listen>").count(), 2, "no tmpdir listener");
        assert!(!conf.contains("session.conf"), "not an include of the distribution's (its own listener)");
        let units = unit_files(Path::new("/usr/bin/dbus-daemon"), Path::new("/run/user/1000/x/space-1/dbus.conf"));
        let unit = |name: &str| units.iter().find(|(n, _)| *n == name).map(|(_, text)| text.as_str()).unwrap();
        assert!(unit("default.target").contains("Wants=dbus.socket dbus.service"));
        assert!(!unit("basic.target").contains("Wants") && !unit("basic.target").contains("Requires"), "nothing of the distribution's basic.target");
        assert!(unit("dbus.socket").contains("ListenStream=%t/bus"));
        assert!(unit("dbus.service").contains("ExecStart=/usr/bin/dbus-daemon --config-file=/run/user/1000/x/space-1/dbus.conf --nofork --nopidfile --syslog-only"));
        assert!(!unit("dbus.service").contains("systemd-activation"), "services start by their Exec= line, in the space");
        let session = Session { child: None, starts: 0, abstract_name: "/tmp/dbus-ede-0011".into() };
        assert_eq!(session.address(Path::new("/run/user/1000/x/space-1")), "unix:path=/run/user/1000/x/space-1/bus;unix:abstract=/tmp/dbus-ede-0011");
        assert_eq!(session_scope(41, 3, 2), "exponential-p41-s3-session2.scope");
        assert!(Session::new().abstract_name.len() > "/tmp/dbus-ede-".len() + 8);
    }

    #[test]
    fn what_a_spaces_manager_starts_with_is_written_into_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let space = dir.path().join("space-1");
        let config = dir.path().join("home/config");
        prepare(&space, Some(&config), Path::new("/usr/bin/dbus-daemon"), "/tmp/dbus-ede-ab").unwrap();
        for name in ["units/default.target", "units/basic.target", "units/dbus.socket", "units/dbus.service", "dbus.conf"] {
            assert!(space.join(name).is_file(), "{name}");
        }
        assert_eq!(std::fs::read_dir(space.join(NOTHING)).unwrap().count(), 0);
        let mode = |path: &Path| std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(path).unwrap().permissions()) & 0o777;
        assert_eq!(mode(&space.join("dbus-1/services")), 0o700, "dbus-daemon refuses a directory others may write to");
        assert!(std::fs::read_to_string(config.join("xdg-desktop-portal/exponential-portals.conf")).unwrap().contains("org.freedesktop.impl.portal.ScreenCast=none"));
        let env = manager_environment(&space);
        assert_eq!(env[0], ("SYSTEMD_UNIT_PATH", space.join("units")), "no trailing ':': ONLY these units");
        // Again (a restart) writes the same over it.
        prepare(&space, Some(&config), Path::new("/usr/bin/dbus-daemon"), "/tmp/dbus-ede-ab").unwrap();
    }
}
