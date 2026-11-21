//! The domains. Each talks to its daemon over the system bus (zbus,
//! blocking; `media` over the session bus) or a command the distribution ships, and answers
//! `unavailable` when it is not there, so a page can say so.

pub mod about;
pub mod bluetooth;
pub mod disks;
pub mod firmware;
pub mod media;
pub mod network;
pub mod power;
pub mod printers;
pub mod sound;
pub mod updates;

use std::{
    collections::HashMap,
    io::Read,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

use zbus::{blocking::Connection, zvariant::{OwnedObjectPath, OwnedValue}};

use crate::Service;

pub fn all() -> Vec<Box<dyn Service>> {
    vec![
        Box::new(network::Network),
        Box::new(sound::Sound),
        Box::new(media::Media),
        Box::new(bluetooth::Bluetooth),
        Box::new(power::Power),
        Box::new(disks::Disks),
        Box::new(updates::Updates),
        Box::new(firmware::Firmware),
        Box::new(printers::Printers),
        Box::new(about::About),
    ]
}

/// `org.freedesktop.DBus.ObjectManager.GetManagedObjects` of `dest` at `path`.
pub type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

pub fn managed_objects(conn: &Connection, dest: &str, path: &str) -> Result<Managed, String> {
    conn.call_method(Some(dest), path, Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &()).and_then(|r| r.body().deserialize::<Managed>()).map_err(|e| e.to_string())
}

/// The objects ordered by path (a map's own order differs on every read,
/// #EDE-135).
pub fn by_path(objects: &Managed) -> Vec<(&OwnedObjectPath, &HashMap<String, HashMap<String, OwnedValue>>)> {
    let mut list: Vec<_> = objects.iter().collect();
    list.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    list
}

/// How long a snapshot's bus call may take before it fails (#EDE-135).
pub const SNAPSHOT_CALL_TIMEOUT: Duration = Duration::from_secs(5);

static POLLED_SYSTEM: Mutex<Option<Connection>> = Mutex::new(None);
static POLLED_SESSION: Mutex<Option<Connection>> = Mutex::new(None);

fn polled(slot: &Mutex<Option<Connection>>, build: fn() -> zbus::Result<zbus::blocking::connection::Builder<'static>>, what: &str) -> Result<Connection, String> {
    let mut slot = slot.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(conn) = slot.as_ref() {
        return Ok(conn.clone());
    }
    let conn = build().and_then(|b| b.method_timeout(SNAPSHOT_CALL_TIMEOUT).build()).map_err(|e| format!("no {what} bus: {e}"))?;
    *slot = Some(conn.clone());
    Ok(conn)
}

/// The system bus the snapshots read: one connection for every poll, each
/// call bounded by [`SNAPSHOT_CALL_TIMEOUT`]. Actions use [`system_bus`]
/// (polkit may ask the person, which takes as long as it takes).
pub fn polled_system_bus() -> Result<Connection, String> {
    polled(&POLLED_SYSTEM, zbus::blocking::connection::Builder::system, "system")
}

/// The session bus the snapshots read, like [`polled_system_bus`].
pub fn polled_session_bus() -> Result<Connection, String> {
    polled(&POLLED_SESSION, zbus::blocking::connection::Builder::session, "session")
}

pub fn system_bus() -> Result<Connection, String> {
    Connection::system().map_err(|e| format!("no system bus: {e}"))
}

/// The session bus (MPRIS players live there).
pub fn session_bus() -> Result<Connection, String> {
    Connection::session().map_err(|e| format!("no session bus: {e}"))
}

/// A property read as `T`.
pub fn prop<T: TryFrom<OwnedValue>>(conn: &Connection, dest: &str, path: &str, iface: &str, name: &str) -> Option<T> {
    let reply = conn.call_method(Some(dest), path, Some("org.freedesktop.DBus.Properties"), "Get", &(iface, name)).ok()?;
    let value: OwnedValue = reply.body().deserialize().ok()?;
    T::try_from(value).ok()
}

/// An action's property write, through [`call_asking`].
pub fn set_prop<'a>(conn: &Connection, dest: &str, path: &str, iface: &str, name: &str, value: zbus::zvariant::Value<'a>) -> Result<(), String> {
    call_asking::<_, ()>(conn, dest, path, "org.freedesktop.DBus.Properties", "Set", &(iface, name, value))
}

/// An action's method call (#EDE-182): the message carries
/// ALLOW_INTERACTIVE_AUTHORIZATION, so a daemon that checks polkit lets the
/// person's agent ask instead of refusing ("interactive authentication has
/// not been enabled by the calling program"). No timeout: it waits as long
/// as the prompt is open, so it never runs on a UI thread. The snapshots'
/// reads never ask ([`polled_system_bus`]).
pub fn call_asking<B, R>(conn: &Connection, dest: &str, path: &str, iface: &str, method: &str, body: &B) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let proxy: zbus::blocking::Proxy<'_> = zbus::blocking::proxy::Builder::new(conn)
        .destination(dest)
        .and_then(|b| b.path(path))
        .and_then(|b| b.interface(iface))
        .and_then(|b| b.cache_properties(zbus::proxy::CacheProperties::No).build())
        .map_err(|e| e.to_string())?;
    proxy.call_with_flags(method, zbus::proxy::MethodFlags::AllowInteractiveAuth.into(), body).map_err(refusal)?.ok_or_else(|| format!("{method}: no reply"))
}

/// What a refused call says, as a failed action's toast describes it: the
/// daemon's own sentence (`org.freedesktop.DBus.Error.AccessDenied:
/// Permission denied` reads `Permission denied`), its error's name when it
/// gave none.
pub fn refusal(e: zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, message, _) => message.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or_else(|| name.to_string()),
        other => other.to_string(),
    }
}

pub fn str_of(props: &HashMap<String, OwnedValue>, key: &str) -> String {
    props.get(key).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default()
}

pub fn bool_of(props: &HashMap<String, OwnedValue>, key: &str) -> bool {
    props.get(key).and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false)
}

pub fn u64_of(props: &HashMap<String, OwnedValue>, key: &str) -> u64 {
    props.get(key).and_then(|v| u64::try_from(v.clone()).ok().or_else(|| u32::try_from(v.clone()).ok().map(u64::from))).unwrap_or(0)
}

/// How long a command may run before it is killed (#EDE-135: an `lpstat`
/// against an unreachable CUPS server).
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// A command's stdout, `None` when it is missing, failed or ran longer than
/// [`COMMAND_TIMEOUT`].
pub fn command(program: &str, args: &[&str]) -> Option<String> {
    command_within(program, args, COMMAND_TIMEOUT)
}

/// [`command`], killed after `limit`.
pub fn command_within(program: &str, args: &[&str], limit: Duration) -> Option<String> {
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // Read while it runs: a full pipe would stall it.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::warn!(program, ?args, "killed after {limit:?}");
                return None;
            }
        }
    };
    let out = reader.join().ok()?;
    status.success().then(|| String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A dbus-daemon of the test's own, never the machine's system bus.
    struct PrivateBus {
        child: std::process::Child,
        address: String,
        _dir: tempfile::TempDir,
    }

    impl PrivateBus {
        fn start() -> PrivateBus {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("bus");
            let config = dir.path().join("bus.conf");
            std::fs::write(&config, format!(r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:path={}</listen><policy context="default"><allow send_destination="*"/><allow own="*"/></policy></busconfig>"#, socket.display())).unwrap();
            let child = Command::new("dbus-daemon").arg("--nofork").arg(format!("--config-file={}", config.display())).stdout(Stdio::null()).spawn().expect("dbus-daemon");
            let deadline = Instant::now() + Duration::from_secs(5);
            while !socket.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            PrivateBus { child, address: format!("unix:path={}", socket.display()), _dir: dir }
        }

        fn connect(&self) -> zbus::blocking::connection::Builder<'static> {
            zbus::blocking::connection::Builder::address(self.address.as_str()).unwrap()
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// logind's stand-in: each call as (method, whether the message allowed
    /// interactive authorization, its `interactive` argument); `Reboot`
    /// answers as systemd does when the person cancels the prompt.
    #[derive(Clone, Default)]
    struct Login(Arc<Mutex<Vec<(String, bool, bool)>>>);

    impl Login {
        fn note(&self, method: &str, header: &zbus::message::Header<'_>, interactive: bool) {
            let asked = header.primary().flags().contains(zbus::message::Flags::AllowInteractiveAuth);
            self.0.lock().unwrap().push((method.to_string(), asked, interactive));
        }
    }

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Login {
        async fn power_off(&self, #[zbus(header)] header: zbus::message::Header<'_>, interactive: bool) {
            self.note("PowerOff", &header, interactive);
        }

        async fn reboot(&self, #[zbus(header)] header: zbus::message::Header<'_>, interactive: bool) -> zbus::fdo::Result<()> {
            self.note("Reboot", &header, interactive);
            Err(zbus::fdo::Error::AccessDenied("Permission denied".into()))
        }
    }

    /// #EDE-182: an action's call lets polkit ask (the flag on the message,
    /// logind's `interactive` true), and a refusal reads as the daemon's
    /// sentence, never a bus dump.
    #[test]
    fn actions_let_polkit_ask_and_a_refusal_says_the_daemons_sentence() {
        let bus = PrivateBus::start();
        let login = Login::default();
        let _logind = bus.connect().name("org.freedesktop.login1").unwrap().serve_at("/org/freedesktop/login1", login.clone()).unwrap().build().unwrap();
        let conn = bus.connect().build().unwrap();
        assert_eq!(power::login(&conn, "poweroff"), Ok(()));
        assert_eq!(power::login(&conn, "reboot"), Err("Permission denied".into()), "a cancelled prompt");
        assert!(power::login(&conn, "dance").is_err());
        assert_eq!(*login.0.lock().unwrap(), vec![("PowerOff".to_string(), true, true), ("Reboot".to_string(), true, true)]);
        assert_eq!(call_asking::<_, ()>(&conn, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager", "Halt", &(true,)), Err("Unknown method 'Halt'".into()), "the sentence, not `org.freedesktop.DBus.Error.UnknownMethod: …`");
    }

    #[test]
    fn a_command_that_hangs_is_killed_at_its_limit() {
        let started = Instant::now();
        assert_eq!(command_within("sleep", &["30"], Duration::from_millis(200)), None);
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
        assert_eq!(command_within("echo", &["hi"], Duration::from_secs(5)).as_deref(), Some("hi\n"));
    }
}
