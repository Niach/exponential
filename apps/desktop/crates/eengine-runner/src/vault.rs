//! The Vault (runner.json `storage`): the platform keystore (Keychain on
//! macOS, the Secret Service on Linux, Credential Manager on Windows), one
//! entry per key under the app's config id. It holds the grants, the last
//! screen and the `local` identity's device uuid.
//!
//! A keystore that is not there (a Linux session without a Secret Service,
//! a locked collection or a macOS Keychain item that would prompt) falls back to `vault.json` in the
//! app's data directory (0600 on unix): a read tries the keystore, then the
//! file, so a value written while the keystore was away stays readable.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eengine::Vault;

/// One platform keystore. `Err` = the keystore is unavailable.
pub trait Keystore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// `vault.json`: one JSON object of key → value.
pub struct FileVault {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileVault {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FileVault { path: path.into(), lock: Mutex::new(()) }
    }

    fn load(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(&self.path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn store(&self, values: &BTreeMap<String, String>) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string(values).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

impl Keystore for FileVault {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        Ok(self.load().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut values = self.load();
        values.insert(key.to_string(), value.to_string());
        self.store(&values)
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut values = self.load();
        if values.remove(key).is_some() {
            self.store(&values)?;
        }
        Ok(())
    }
}

/// The Engine's Vault: the keystore first, the file when it is away.
pub struct RunnerVault {
    keystore: Option<Arc<dyn Keystore>>,
    file: FileVault,
}

impl RunnerVault {
    pub fn new(keystore: Option<Arc<dyn Keystore>>, file: FileVault) -> Self {
        RunnerVault { keystore, file }
    }

    /// The platform keystore for app `id`, falling back to `<data dir>/vault.json`.
    pub fn platform(id: &str, name: &str, data_dir: &std::path::Path) -> Self {
        RunnerVault::new(platform_keystore(id, name), FileVault::new(data_dir.join("vault.json")))
    }
}

impl Vault for RunnerVault {
    fn read(&self, key: &str) -> Option<String> {
        if let Some(Ok(Some(value))) = self.keystore.as_ref().map(|k| k.get(key)) {
            return Some(value);
        }
        self.file.get(key).ok().flatten()
    }

    fn write(&self, key: &str, value: &str) -> Result<(), String> {
        match self.keystore.as_ref().map(|k| k.set(key, value)) {
            Some(Ok(())) => {
                // The keystore holds it now: no stale copy in the file.
                let _ = self.file.delete(key);
                Ok(())
            }
            _ => self.file.set(key, value),
        }
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        if let Some(k) = &self.keystore {
            let _ = k.delete(key);
        }
        self.file.delete(key)
    }
}

/// The `local` identity's device uuid under runner.json `storage.deviceKey`, created once.
pub fn device_id(vault: &dyn Vault) -> String {
    let key = eengine::runner::device_key();
    if let Some(id) = vault.read(key).filter(|id| uuid::Uuid::parse_str(id).is_ok()) {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = vault.write(key, &id);
    id
}

#[cfg(target_os = "macos")]
fn platform_keystore(id: &str, _name: &str) -> Option<Arc<dyn Keystore>> {
    Some(Arc::new(mac::Keychain { service: id.to_string() }))
}

#[cfg(target_os = "linux")]
fn platform_keystore(id: &str, name: &str) -> Option<Arc<dyn Keystore>> {
    Some(Arc::new(linux::SecretService::new(id, name)))
}

#[cfg(target_os = "windows")]
fn platform_keystore(id: &str, _name: &str) -> Option<Arc<dyn Keystore>> {
    Some(Arc::new(win::CredentialManager { prefix: id.to_string() }))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_keystore(_id: &str, _name: &str) -> Option<Arc<dyn Keystore>> {
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use security_framework::os::macos::keychain::{KeychainUserInteractionLock, SecKeychain};
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    /// errSecItemNotFound.
    const NOT_FOUND: i32 = -25300;

    /// Generic passwords: service = the config id, account = the key.
    pub struct Keychain {
        pub service: String,
    }

    /// Never a Keychain prompt: an item this build may not read without
    /// asking (another build's ACL, e.g. a rebundled ad-hoc app) or a locked
    /// keychain answers errSecInteractionNotAllowed, and the Vault falls back
    /// to `vault.json` instead of blocking the launch on a hidden dialog.
    fn quiet() -> Result<KeychainUserInteractionLock, String> {
        SecKeychain::disable_user_interaction().map_err(|e| e.to_string())
    }

    impl super::Keystore for Keychain {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            let _quiet = quiet()?;
            match get_generic_password(&self.service, key) {
                Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
                Err(e) if e.code() == NOT_FOUND => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            let _quiet = quiet()?;
            set_generic_password(&self.service, key, value.as_bytes()).map_err(|e| e.to_string())
        }

        fn delete(&self, key: &str) -> Result<(), String> {
            let _quiet = quiet()?;
            match delete_generic_password(&self.service, key) {
                Err(e) if e.code() != NOT_FOUND => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    //! org.freedesktop.Secret.Service, the `plain` session (the session bus
    //! is the user's own), items in the default collection keyed by the
    //! attributes `application` (the config id) and `key`.

    use std::collections::HashMap;
    use std::sync::Mutex;

    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    const DEST: &str = "org.freedesktop.secrets";
    const SERVICE: &str = "org.freedesktop.Secret.Service";
    const ITEM: &str = "org.freedesktop.Secret.Item";
    const DEFAULT: &str = "/org/freedesktop/secrets/aliases/default";

    /// (session, parameters, value, content type).
    type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

    struct Session {
        conn: Connection,
        path: OwnedObjectPath,
    }

    pub struct SecretService {
        app: String,
        name: String,
        session: Mutex<Option<Session>>,
    }

    impl SecretService {
        pub fn new(app: &str, name: &str) -> Self {
            SecretService { app: app.to_string(), name: name.to_string(), session: Mutex::new(None) }
        }

        fn with<T>(&self, f: impl FnOnce(&Session) -> zbus::Result<T>) -> Result<T, String> {
            let mut guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
            if guard.is_none() {
                // The Secret Service answers in milliseconds; one that never
                // answers (a keyring activated outside a session) must not
                // hold the launch: past the timeout the vault is unavailable.
                let conn = zbus::blocking::connection::Builder::session()
                    .and_then(|b| b.method_timeout(std::time::Duration::from_secs(5)).build())
                    .map_err(|e| e.to_string())?;
                let service = Proxy::new(&conn, DEST, "/org/freedesktop/secrets", SERVICE).map_err(|e| e.to_string())?;
                let (_, path): (OwnedValue, OwnedObjectPath) = service.call("OpenSession", &("plain", Value::from(""))).map_err(|e| e.to_string())?;
                *guard = Some(Session { conn, path });
            }
            let result = f(guard.as_ref().expect("a session"));
            if result.is_err() {
                *guard = None;
            }
            result.map_err(|e| e.to_string())
        }

        fn attributes<'a>(&'a self, key: &'a str) -> HashMap<&'a str, &'a str> {
            HashMap::from([("application", self.app.as_str()), ("key", key)])
        }

        /// The unlocked items for `key`; a locked one = unavailable (unlocking would prompt).
        fn items(&self, s: &Session, key: &str) -> zbus::Result<Vec<OwnedObjectPath>> {
            let service = Proxy::new(&s.conn, DEST, "/org/freedesktop/secrets", SERVICE)?;
            let (unlocked, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) = service.call("SearchItems", &(self.attributes(key),))?;
            if unlocked.is_empty() && !locked.is_empty() {
                return Err(zbus::Error::Failure("the collection is locked".into()));
            }
            Ok(unlocked)
        }
    }

    impl super::Keystore for SecretService {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            self.with(|s| {
                let Some(item) = self.items(s, key)?.into_iter().next() else { return Ok(None) };
                let item = Proxy::new(&s.conn, DEST, item, ITEM)?;
                let secret: Secret = item.call("GetSecret", &(&s.path,))?;
                Ok(String::from_utf8(secret.2).ok())
            })
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.with(|s| {
                let collection = Proxy::new(&s.conn, DEST, DEFAULT, "org.freedesktop.Secret.Collection")?;
                let label = format!("{}: {key}", self.name);
                let mut properties: HashMap<&str, Value> = HashMap::new();
                properties.insert("org.freedesktop.Secret.Item.Label", Value::from(label));
                properties.insert("org.freedesktop.Secret.Item.Attributes", Value::from(self.attributes(key)));
                let secret: Secret = (s.path.clone(), Vec::new(), value.as_bytes().to_vec(), "text/plain".into());
                let (_, prompt): (OwnedObjectPath, OwnedObjectPath) = collection.call("CreateItem", &(properties, secret, true))?;
                if prompt.as_str() != "/" {
                    return Err(zbus::Error::Failure("the collection is locked".into()));
                }
                Ok(())
            })
        }

        fn delete(&self, key: &str) -> Result<(), String> {
            self.with(|s| {
                for item in self.items(s, key)? {
                    let item = Proxy::new(&s.conn, DEST, item, ITEM)?;
                    let _prompt: OwnedObjectPath = item.call("Delete", &())?;
                }
                Ok(())
            })
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    //! Generic credentials, target `<config id>/<key>`, persisted per machine user.

    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};

    pub struct CredentialManager {
        pub prefix: String,
    }

    impl CredentialManager {
        fn target(&self, key: &str) -> HSTRING {
            HSTRING::from(format!("{}/{key}", self.prefix))
        }
    }

    fn not_found(e: &windows::core::Error) -> bool {
        e.code() == ERROR_NOT_FOUND.to_hresult()
    }

    impl super::Keystore for CredentialManager {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            let target = self.target(key);
            let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
            match unsafe { CredReadW(&target, CRED_TYPE_GENERIC, None, &mut cred) } {
                Ok(()) => {
                    let value = unsafe {
                        let c = &*cred;
                        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
                        CredFree(cred as *const _);
                        bytes
                    };
                    Ok(String::from_utf8(value).ok())
                }
                Err(e) if not_found(&e) => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            let mut target: Vec<u16> = format!("{}/{key}", self.prefix).encode_utf16().chain(Some(0)).collect();
            let mut blob = value.as_bytes().to_vec();
            let cred = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR(target.as_mut_ptr()),
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            unsafe { CredWriteW(&cred, 0) }.map_err(|e| e.to_string())
        }

        fn delete(&self, key: &str) -> Result<(), String> {
            match unsafe { CredDeleteW(&self.target(key), CRED_TYPE_GENERIC, None) } {
                Err(e) if !not_found(&e) => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Away;

    impl Keystore for Away {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err("no keystore".into())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("no keystore".into())
        }
        fn delete(&self, _: &str) -> Result<(), String> {
            Err("no keystore".into())
        }
    }

    #[test]
    fn the_file_holds_what_the_keystore_could_not_and_stays_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.json");
        let away = RunnerVault::new(Some(Arc::new(Away)), FileVault::new(&path));
        away.write("k", "v1").unwrap();
        assert_eq!(away.read("k").as_deref(), Some("v1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // The keystore is back: the file's value still reads, a write moves it over.
        let store = Arc::new(FileVault::new(dir.path().join("keystore.json")));
        let back = RunnerVault::new(Some(store.clone()), FileVault::new(&path));
        assert_eq!(back.read("k").as_deref(), Some("v1"));
        back.write("k", "v2").unwrap();
        assert_eq!(store.get("k").unwrap().as_deref(), Some("v2"));
        assert_eq!(FileVault::new(&path).get("k").unwrap(), None);
        back.delete("k").unwrap();
        assert_eq!(back.read("k"), None);
    }

    #[test]
    fn the_device_uuid_is_created_once() {
        let dir = tempfile::tempdir().unwrap();
        let vault = RunnerVault::new(None, FileVault::new(dir.path().join("vault.json")));
        let id = device_id(&vault);
        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(device_id(&vault), id);
        assert_eq!(vault.read("eengine/runner/device").as_deref(), Some(id.as_str()));
    }
}
