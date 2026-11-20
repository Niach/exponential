//! #EDE-125: a space closes and leaves nothing: no client, no socket, no
//! directory, no global, and once nothing refers to it any more, no seat.

use std::{
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::Child,
    time::{Duration, Instant},
};

use smithay::{
    backend::input::{ButtonState, InputTime, KeyState},
    input::{keyboard::FilterResult, pointer::ButtonEvent, Seat},
    reexports::wayland_server::backend::DisconnectReason,
    utils::SERIAL_COUNTER,
};

use super::{scope::Scope, slice, AgentSpace, Spaces};
use crate::{
    desk::{Desk, DeskId},
    state::Ede,
};

/// A grant lives 15 minutes by the wall clock and the ticks stand still
/// during a suspend: without a grace the first tick after a long one
/// would close every space before its owner could renew.
pub const EXPIRY_GRACE: u32 = 30;

/// The seat of a closed space. smithay's handles look their seat up and
/// panic when it is missing, so it stays until nothing shares it.
pub(crate) struct Closing {
    seat: Seat<Ede>,
    since: Instant,
    logged: bool,
}

/// The directory of a closed space that is still there: what ran in the
/// space may keep it busy for a moment after its clients are gone (GTK
/// apps leave gvfs and document-portal FUSE mounts that unmount by
/// themselves).
pub(crate) struct Leftover {
    dir: std::path::PathBuf,
    ticks: u32,
}

/// How many 1 s ticks a closed space's directory is tried again.
pub const DIR_RETRIES: u32 = 30;

/// Removes `dir` with all in it. A space's own user manager leaves
/// `systemd/inaccessible/dir` with mode 000 in it (#EDE-138), which the
/// owner may not open to empty: what the owner cannot enter is made
/// theirs to enter first.
pub fn remove_tree(dir: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            open_up(dir);
            std::fs::remove_dir_all(dir)
        }
        done => done,
    }
}

/// The FUSE mount points at or under `dir` in `mountinfo`
/// (`/proc/self/mountinfo`), the deepest first.
pub fn fuse_mounts_under(mountinfo: &str, dir: &std::path::Path) -> Vec<PathBuf> {
    // `\040` and the like: a mount point's spaces, tabs and backslashes.
    let unescape = |text: &str| {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            let code: String = if c == '\\' { chars.clone().take(3).collect() } else { String::new() };
            match u8::from_str_radix(&code, 8) {
                Ok(byte) if code.len() == 3 => {
                    out.push(byte as char);
                    chars.nth(2);
                }
                _ => out.push(c),
            }
        }
        out
    };
    let mut points: Vec<PathBuf> = mountinfo
        .lines()
        .filter_map(|line| {
            let point = line.split(' ').nth(4)?;
            let kind = line.split(" - ").nth(1)?.split(' ').next()?;
            (kind == "fuse" || kind.starts_with("fuse.")).then(|| PathBuf::from(unescape(point)))
        })
        .filter(|point| point.starts_with(dir))
        .collect();
    points.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    points
}

/// `fusermount3 -u -z` (else `fusermount`): detached now, gone once
/// nothing uses it; it does not wait for the FUSE daemon.
fn unmount_lazily(point: &std::path::Path) {
    for tool in ["fusermount3", "fusermount"] {
        let done = std::process::Command::new(tool).args(["-u", "-z"]).arg(point).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
        if done.is_ok_and(|status| status.success()) {
            tracing::info!("{}: unmounted what a closed space left mounted there", point.display());
            return;
        }
    }
}

/// Every directory under `dir` (never through a link) open to its owner.
fn open_up(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let path = entry.path();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
            open_up(&path);
        }
    }
}

/// The slice of a closed space (or of a compositor that is gone) while
/// it ends (#EDE-138). The user manager stops it: SIGTERM to every process
/// of every unit in it, SIGKILL two seconds later. A scope that asked to
/// be left alone at a stop (`KillMode=none`) keeps running, and a launch
/// that was still on its way into its scope arrives after the stop: what
/// is in the slice's cgroup after [`STOP_GRACE`] is ended by the kernel,
/// and the slice is stopped once more.
pub(crate) struct Stopping {
    unit: String,
    /// `systemctl stop`, until it is reaped; never waited for.
    stop: Option<Child>,
    /// The slice's cgroup directory.
    cgroup: PathBuf,
    /// It was there when the stop was asked for.
    seen: bool,
    since: Instant,
    /// The grace is over (or was not needed): nothing more is done.
    done: bool,
}

/// After a space closed, how long its slice's processes get to end by the
/// user manager's stop before the kernel ends what is left.
const STOP_GRACE: Duration = Duration::from_millis(2500);

impl Spaces {
    /// Sets where the spaces' directories live, first removing what a
    /// crashed compositor of the same name left there.
    pub fn open(&mut self, dir: std::path::PathBuf) {
        let _ = std::fs::remove_dir_all(&dir);
        self.dir = Some(dir);
    }

    /// Sets where this compositor's spaces keep their homes: `p<pid>` in
    /// `root` (`$XDG_CACHE_HOME/exponential-desktop/spaces`), first removing
    /// what a compositor that is gone left there: a `p<pid>` whose process
    /// is no running compositor, and an earlier one with this process id.
    pub fn open_homes(&mut self, root: std::path::PathBuf) {
        let me = std::process::id();
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(pid) = name.to_str().and_then(|name| name.strip_prefix('p')).and_then(|pid| pid.parse::<u32>().ok()) else { continue };
                if pid == me || !slice::is_compositor(pid) {
                    let _ = (self.remove_dir)(&entry.path());
                }
            }
        }
        let homes = root.join(format!("p{me}"));
        match std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&homes) {
            Ok(()) => self.homes = Some(homes),
            Err(e) => tracing::warn!("{}: {e}: agent spaces keep their homes in the runtime directory", homes.display()),
        }
    }

    /// No closed space's seat is left to remove.
    pub fn settled(&self) -> bool {
        self.closing.is_empty() && self.leftovers.is_empty() && self.stopping.is_empty()
    }

    /// Removes `dir`; `true` = it is gone. What the space's bus started
    /// mounts FUSE file systems in its runtime directory (the document
    /// portal's `doc`, gvfs's `gvfs`); when the space's slice ends their
    /// daemons, the mounts stay, so they are let go of first (#EDE-138).
    fn remove(&self, dir: &std::path::Path) -> bool {
        if let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") {
            for point in fuse_mounts_under(&mountinfo, dir) {
                unmount_lazily(&point);
            }
        }
        (self.remove_dir)(dir).is_ok() || !dir.exists()
    }
}

impl Drop for Spaces {
    /// The compositor ends: the sockets go with their sources, the
    /// directories here.
    fn drop(&mut self) {
        self.kill_launches();
        // The session's decision about the guard ends with it.
        if let Some(file) = &self.guard.file {
            let _ = std::fs::remove_file(file);
        }
        // Everything under the spaces' directory is a space's: the live
        // ones and what a close left behind; their homes likewise.
        for dir in [&self.dir, &self.homes].into_iter().flatten() {
            self.remove(dir);
        }
    }
}

impl Ede {
    /// Closes space `id`; `false` = no such space (never the person's
    /// desk). Its clients' destructors run later and find no desk: their
    /// tags never fall back to the person's.
    pub fn close_space(&mut self, id: DeskId) -> bool {
        let Some(at) = self.spaces.list.iter().position(|s| s.desk.id == id && id != DeskId::PERSON) else { return false };
        let AgentSpace { id: name, desk, output, output_global, dir, listener, clients, sliced, mut session, .. } = self.spaces.list.remove(at);
        // Its user manager ends with its slice; reaped on the tick.
        self.spaces.ended_sessions.extend(session.take());

        // Its seat lets go of everything: what is held down, a grab, the focus.
        let time = InputTime::from_millis(crate::ipc::now_ms() as u32);
        let (keyboard, pointer) = (desk.seat.get_keyboard().unwrap(), desk.seat.get_pointer().unwrap());
        for key in keyboard.pressed_keys() {
            keyboard.input::<(), _>(self, key, KeyState::Released, SERIAL_COUNTER.next_serial(), time, |_, _, _| FilterResult::Forward);
        }
        pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        for (_, button) in desk.driven.buttons.iter().copied() {
            pointer.button(self, &ButtonEvent { button, state: ButtonState::Released, serial: SERIAL_COUNTER.next_serial(), time });
        }
        pointer.frame(self);
        if keyboard.current_focus().is_some() {
            keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
        }
        drop((keyboard, pointer));

        // No new client, no old one, nothing on disk.
        self.loop_handle.remove(listener);
        // What was launched is told to end first: it writes in its home.
        let home = self.spaces.homes.as_ref().map(|homes| homes.join(name.as_str()));
        self.space_launches_closed(id);
        if sliced {
            self.space_slice_closed(id.number());
        }
        for dir in std::iter::once(dir).chain(home) {
            if !self.spaces.remove(&dir) {
                self.spaces.leftovers.push(Leftover { dir, ticks: 0 });
            }
        }
        for client in clients {
            self.display_handle.backend_handle().kill_client(client.id(), DisconnectReason::ConnectionClosed);
        }
        // The removals are told to every client, the person's too: two
        // names they never saw.
        if let Some(global) = desk.seat.global() {
            self.display_handle.remove_global::<Ede>(global);
        }
        self.display_handle.remove_global::<Ede>(output_global);
        // #EDE-126: what still read from its clients answers `unavailable`.
        let Desk { seat, clipboard, .. } = desk;
        self.clipboard_closed(clipboard);
        drop(output);
        self.spaces.closing.push(Closing { seat, since: Instant::now(), logged: false });
        tracing::info!("{} closed", name.as_str());
        true
    }

    /// #EDE-138, at the session's start: finds the person's user manager,
    /// so launches run in scopes of their spaces' slices, and stops the
    /// slices a compositor that is gone left behind (their spaces'
    /// processes would run on). Without a user manager the reason is
    /// logged once and launches run as before, for the whole run.
    pub fn start_space_units(&mut self) {
        self.start_space_guard();
        let units = match slice::Units::probe() {
            Ok(units) => units,
            Err(reason) => {
                if self.spaces.guard.required.is_some() {
                    tracing::warn!("nothing starts in an agent space: the space guard needs a systemd user manager: {reason}");
                } else {
                    tracing::warn!("agent spaces run in the login session, without limits: {reason} (a systemd user manager from {} on is needed)", slice::SYSTEMD_FLOOR);
                }
                return;
            }
        };
        let own = ede_protocol::process::spaces_slice(units.compositor);
        for unit in units.stale() {
            tracing::warn!("{unit} is a gone compositor's: its spaces' processes are stopped");
            let cgroup = units.app_slice.join(&unit);
            if unit == own {
                // An earlier compositor had this process id: gone before
                // this one makes a slice of that name.
                slice::kill_cgroup(&cgroup);
                units.stop_now(&unit, Duration::from_secs(10));
            } else {
                let (stop, seen) = (units.stop(&unit), cgroup.is_dir());
                self.spaces.stopping.push(Stopping { unit, stop, cgroup, seen, since: Instant::now(), done: false });
            }
        }
        tracing::info!("agent spaces run in systemd scopes (systemd {}): {}", units.version, slice::described(&self.config.spaces, units.machine));
        // #EDE-146..148: an installed guard is asked by every launch, whatever
        // it answers now (fail closed); see `guard`.
        let mut units = units;
        units.guard = self.spaces.guard.required.clone();
        self.start_polkit_probe(&units);
        self.spaces.units = Some(units);
        self.write_space_guard_state();
    }

    /// Space number `space` closed and had launches in scopes: its slice
    /// is stopped with all in it. Nothing here waits for the user manager.
    fn space_slice_closed(&mut self, space: u32) {
        let Some(units) = &self.spaces.units else { return };
        let unit = ede_protocol::process::space_slice(units.compositor, space);
        let cgroup = units.space_cgroup(space);
        let (stop, seen) = (units.stop(&unit), cgroup.is_dir());
        self.spaces.stopping.push(Stopping { unit, stop, cgroup, seen, since: Instant::now(), done: false });
    }

    /// Once a second: what a stopped slice still holds after its grace is
    /// ended by the kernel, and the slice is stopped once more.
    pub(crate) fn space_slices_tick(&mut self) {
        let Some(units) = &self.spaces.units else { return };
        self.spaces.stopping.retain_mut(|stopping| {
            if stopping.stop.as_mut().is_some_and(|stop| !matches!(stop.try_wait(), Ok(None))) {
                stopping.stop = None;
            }
            if stopping.done {
                return stopping.stop.is_some();
            }
            // The stop is done and the slice that was there is gone: no
            // need to wait for the grace. (One that was not there yet may
            // still come: a launch on its way into its scope.)
            if stopping.seen && stopping.stop.is_none() && !stopping.cgroup.is_dir() {
                return false;
            }
            if stopping.since.elapsed() > STOP_GRACE {
                stopping.done = true;
                if slice::kill_cgroup(&stopping.cgroup) {
                    tracing::info!("{} outlived its stop: the kernel ended what was left in it", stopping.unit);
                    if stopping.stop.is_none() {
                        stopping.stop = units.stop(&stopping.unit);
                    }
                }
            }
            true
        });
    }

    /// Does `session` hold a `[space]` grant that has not expired?
    fn has_space_grant(&self, session: &ede_protocol::SessionId) -> bool {
        let now = crate::ipc::now_ms();
        self.ipc.grants.iter().any(|g| g.expires_at.is_none_or(|e| now < e) && Scope::of(g) == Scope::Space(session.clone()))
    }

    /// A grant of `session` was revoked: its space closes at once, unless
    /// the session still holds another live space grant.
    pub(crate) fn space_grant_revoked(&mut self, session: &ede_protocol::SessionId) {
        if self.has_space_grant(session) {
            return;
        }
        let desk = self.spaces.iter().find(|s| &s.owner == session).map(|s| s.desk.id);
        if let Some(desk) = desk {
            self.close_space(desk);
        }
    }

    /// A space whose owner has had no live space grant for
    /// `expiry_grace` ticks in a row closes; a grant in time starts the
    /// count over.
    fn spaces_expire(&mut self) {
        let owners: Vec<_> = self.spaces.iter().map(|s| (s.desk.id, s.owner.clone())).collect();
        for (desk, owner) in owners {
            let granted = self.has_space_grant(&owner);
            let grace = self.spaces.expiry_grace;
            let Some(space) = self.spaces.get_mut(desk) else { continue };
            space.expired_ticks = if granted { 0 } else { space.expired_ticks + 1 };
            if space.expired_ticks >= grace {
                tracing::info!("{}: no grant for {grace} ticks", space.id.as_str());
                self.close_space(desk);
            }
        }
    }

    /// Once a second: a closed space's seat leaves smithay's list as soon
    /// as nothing but that list and we hold it. One that stays shared is
    /// kept: a leaked seat is cheap, a handle without its seat panics.
    pub fn spaces_tick(&mut self) {
        self.spaces_expire();
        self.space_sessions_tick();
        self.space_launches_tick();
        let mut closing = std::mem::take(&mut self.spaces.closing);
        closing.retain_mut(|c| {
            if self.seat_state.remove_seat(&c.seat) {
                return false;
            }
            if !c.logged && c.since.elapsed() > Duration::from_secs(10) {
                c.logged = true;
                tracing::warn!("{} is still shared 10 s after its space closed; it is kept", c.seat.name());
            }
            true
        });
        closing.append(&mut self.spaces.closing);
        self.spaces.closing = closing;

        // A directory that was busy at the close: again, for a while.
        let mut leftovers = std::mem::take(&mut self.spaces.leftovers);
        leftovers.retain_mut(|l| {
            if self.spaces.remove(&l.dir) {
                return false;
            }
            l.ticks += 1;
            if l.ticks >= DIR_RETRIES {
                tracing::warn!("{} is still there {DIR_RETRIES} s after its space closed; it stays until the compositor ends", l.dir.display());
                return false;
            }
            true
        });
        leftovers.append(&mut self.spaces.leftovers);
        self.spaces.leftovers = leftovers;
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    /// The document portal's and gvfs's mounts in a space's directory are
    /// found, the deepest first; nothing of another directory or kind.
    #[test]
    fn a_spaces_fuse_mounts_are_found_in_mountinfo() {
        let info = "29 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
329 2286 0:55 / /run/user/1000/x.spaces/space-1/doc rw,nosuid,nodev,relatime shared:644 - fuse.portal portal rw,user_id=1000
409 2286 0:56 / /run/user/1000/x.spaces/space-1/gvfs rw,nosuid,nodev,relatime shared:735 - fuse.gvfsd-fuse gvfsd-fuse rw,user_id=1000
410 409 0:57 / /run/user/1000/x.spaces/space-1/gvfs/a\\040b rw - fuse.sshfs x rw
411 2286 0:58 / /run/user/1000/x.spaces/space-10/doc rw - fuse.portal portal rw
412 2286 0:59 / /run/user/1000/doc rw - fuse.portal portal rw
413 2286 0:60 / /run/user/1000/x.spaces/space-1/tmp rw - tmpfs tmpfs rw
";
        let points = super::fuse_mounts_under(info, std::path::Path::new("/run/user/1000/x.spaces/space-1"));
        assert_eq!(points, ["/run/user/1000/x.spaces/space-1/gvfs/a b", "/run/user/1000/x.spaces/space-1/doc", "/run/user/1000/x.spaces/space-1/gvfs"].map(std::path::PathBuf::from));
    }

    /// What a space's user manager leaves: a directory nobody may enter.
    #[test]
    fn a_directory_nobody_may_enter_goes_too() {
        let dir = tempfile::tempdir().unwrap();
        let space = dir.path().join("space-1");
        let locked = space.join("systemd/inaccessible/dir");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(space.join("systemd/inaccessible/reg"), "").unwrap();
        std::fs::set_permissions(space.join("systemd/inaccessible/reg"), std::fs::Permissions::from_mode(0)).unwrap();
        std::fs::create_dir(locked.join("inside")).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0)).unwrap();
        assert!(std::fs::remove_dir_all(&space).is_err(), "the plain removal fails here");
        super::remove_tree(&space).unwrap();
        assert!(!space.exists());
    }
}
