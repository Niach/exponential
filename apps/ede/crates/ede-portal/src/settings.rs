//! `org.freedesktop.impl.portal.Settings`: `org.freedesktop.appearance`
//! from `[appearance]` (dark by default, Exponential's own look). The GTK
//! backend serves the rest (`exponential-portals.conf` lists both).

use std::{collections::HashMap, sync::Arc};

use zbus::{interface, zvariant::OwnedValue};

use crate::{value, Config, Ctx, PortalError, Vardict, PATH};

const APPEARANCE: &str = "org.freedesktop.appearance";

pub struct Settings {
    pub ctx: Arc<Ctx>,
}

pub fn values(config: &Config) -> Vardict {
    let mut v = Vardict::new();
    v.insert("color-scheme".into(), value(config.color_scheme()));
    v.insert("contrast".into(), value(u32::from(config.appearance.high_contrast)));
    if let Some(accent) = config.accent() {
        v.insert("accent-color".into(), value(accent));
    }
    v
}

/// `ReadAll`'s namespace filter: exact, or a trailing `*` prefix; empty =
/// everything.
fn wanted(namespaces: &[String], ns: &str) -> bool {
    namespaces.is_empty()
        || namespaces.iter().any(|n| n.is_empty() || n == ns || n.strip_suffix('*').is_some_and(|prefix| ns.starts_with(prefix)))
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl Settings {
    async fn read_all(&self, namespaces: Vec<String>) -> HashMap<String, Vardict> {
        let mut out = HashMap::new();
        if wanted(&namespaces, APPEARANCE) {
            out.insert(APPEARANCE.to_string(), values(&self.ctx.config.lock().unwrap()));
        }
        out
    }

    async fn read(&self, namespace: String, key: String) -> Result<OwnedValue, PortalError> {
        if namespace == APPEARANCE {
            if let Some(v) = values(&self.ctx.config.lock().unwrap()).remove(&key) {
                return Ok(v);
            }
        }
        Err(PortalError::NotFound(format!("{namespace} {key}")))
    }

    #[zbus(signal)]
    async fn setting_changed(emitter: &zbus::object_server::SignalEmitter<'_>, namespace: &str, key: &str, value: zbus::zvariant::Value<'_>) -> zbus::Result<()>;

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        2
    }
}

/// After a reload: `SettingChanged` for every value that changed.
pub fn announce_changes(ctx: &Ctx, old: &Config, new: &Config) {
    let Some(conn) = ctx.connection() else { return };
    let (before, after) = (values(old), values(new));
    for (key, v) in &after {
        if before.get(key) != Some(v) {
            let v = zbus::zvariant::Value::from(v.try_clone().expect("no fds"));
            let _ = zbus::block_on(conn.emit_signal(None::<()>, PATH, "org.freedesktop.impl.portal.Settings", "SettingChanged", &(APPEARANCE, key.as_str(), v)));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn namespace_globs() {
        let ns = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(super::wanted(&ns(&[]), "org.freedesktop.appearance"));
        assert!(super::wanted(&ns(&["org.freedesktop.*"]), "org.freedesktop.appearance"));
        assert!(!super::wanted(&ns(&["org.gnome.*"]), "org.freedesktop.appearance"));
    }
}
