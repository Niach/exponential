//! #FEED-232: the OS app icon's count = `nav_badges::app_icon_badge` over
//! every team's needs-you rows (0 clears it). macOS: the dock tile's badge
//! label; Linux: the Unity LauncherEntry `Update` signal (GNOME docks, KDE,
//! Plank read it); Windows: none yet (a taskbar overlay icon needs a drawn
//! bitmap per count).

use std::sync::{Mutex, OnceLock};

static LAST: Mutex<Option<usize>> = Mutex::new(None);
static APP_ID: OnceLock<String> = OnceLock::new();

/// The `.desktop` id the launcher entry names (`<id>.desktop`); the app sets
/// it with its identity.
pub fn set_app_id(id: &str) {
    let _ = APP_ID.set(id.to_string());
}

/// Show `count` on the app icon; repeats are dropped.
pub(crate) fn set(count: usize) {
    {
        let Ok(mut last) = LAST.lock() else { return };
        if *last == Some(count) {
            return;
        }
        *last = Some(count);
    }
    // Headless tests mount shells off the main thread, without a dock.
    if !cfg!(test) {
        backend::set(count);
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSAutoreleasePool, NSString};
    use objc::{class, msg_send, sel, sel_impl};

    /// On the main thread (the shell's render / observer).
    pub(super) fn set(count: usize) {
        let label = domain::nav_badges::count_badge_label(count as i64);
        // SAFETY: the shared application's dock tile, a property write; the
        // label string is autoreleased.
        unsafe {
            let app: id = msg_send![class!(NSApplication), sharedApplication];
            let tile: id = msg_send![app, dockTile];
            let text: id = if label.is_empty() { nil } else { NSString::alloc(nil).init_str(&label).autorelease() };
            let _: () = msg_send![tile, setBadgeLabel: text];
        }
    }
}

#[cfg(target_os = "linux")]
mod backend {
    use std::collections::HashMap;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Mutex, OnceLock};

    use zbus::zvariant::Value;

    static TX: OnceLock<Option<Mutex<Sender<usize>>>> = OnceLock::new();

    /// One thread owns the session bus connection; a count is a message.
    pub(super) fn set(count: usize) {
        let tx = TX.get_or_init(|| {
            let (tx, rx) = channel();
            std::thread::Builder::new().name("app-badge".into()).spawn(move || run(rx)).ok()?;
            Some(Mutex::new(tx))
        });
        if let Some(tx) = tx.as_ref().and_then(|tx| tx.lock().ok()) {
            let _ = tx.send(count);
        }
    }

    fn run(rx: Receiver<usize>) {
        let Ok(connection) = zbus::blocking::Connection::session() else {
            log::debug!("[app-badge] no session bus");
            return;
        };
        let uri = format!("application://{}.desktop", super::APP_ID.get().map(String::as_str).unwrap_or("at.exponential"));
        for count in rx {
            let mut properties: HashMap<&str, Value<'_>> = HashMap::new();
            properties.insert("count", Value::I64(count as i64));
            properties.insert("count-visible", Value::Bool(count > 0));
            if let Err(err) = connection.emit_signal(
                None::<zbus::names::BusName<'_>>,
                "/at/exponential/LauncherEntry",
                "com.canonical.Unity.LauncherEntry",
                "Update",
                &(uri.as_str(), properties),
            ) {
                log::debug!("[app-badge] launcher entry update failed: {err}");
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod backend {
    pub(super) fn set(_count: usize) {}
}
