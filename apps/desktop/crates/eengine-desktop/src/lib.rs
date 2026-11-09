//! VAPP-265 (Engine E5): the Engine's native capabilities on the desktop.
//! Five [`eengine::Capability`] providers (`photos`, `files`, `share`,
//! `print`, `notifications`) answering the native half of
//! `packages/eengine/fixtures/capabilities.json`: the bridge (validation,
//! fileIds, the BlobStore) is the Engine's; a native here gets the bridge's
//! native arguments (bytes as base64 `data`) and answers an MCP call result
//! whose `structuredContent` matches the tool's `nativeOutputSchema`.
//!
//! THREADING: every `call` BLOCKS its caller until the person answers the
//! platform's picker, sheet or dialog. Call from a worker thread, never the
//! UI/main thread: on macOS the AppKit work is dispatched to the main queue
//! and waited for (a main-thread call answers `failed`), on Linux the portal
//! calls run on their own thread, on Windows COM runs on its own thread.
//!
//! Every platform API sits behind a seam ([`Dialogs`], [`ShareSheet`],
//! [`Printer`], [`Notifier`], the Pictures folder), so the pure parts are
//! tested on any OS with fakes.

use std::path::PathBuf;
use std::sync::Arc;

use eengine::Capability;
use serde_json::Value;

mod files;
mod mcp;
mod mime;
mod notifications;
mod photos;
mod platform;
mod print;
mod share;

#[cfg(test)]
mod tests;

pub use files::FilesCapability;
pub use notifications::{NotificationsCapability, Shown};
pub use photos::PhotosCapability;
pub use print::PrintCapability;
pub use share::ShareCapability;

/// What a native seam answers besides its value.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeError {
    /// This platform cannot do it (isError `unavailable`).
    Unavailable,
    /// photos.get: no such photo (isError `notFound`).
    NotFound,
    /// It went wrong (isError `failed: <message>`).
    Failed(String),
}

pub type NativeResult<T> = Result<T, NativeError>;

/// Which files a picker offers. Both empty = any file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileFilter {
    /// MIME types as given (`image/*`, `text/csv`): the Linux portal filters by them.
    pub mime_types: Vec<String>,
    /// Lowercase extensions without the dot (`csv`): macOS and Windows filter by them.
    /// Empty while `mime_types` is not = an accepted type maps to no known
    /// extension, so the extension platforms offer every file.
    pub extensions: Vec<String>,
}

impl FileFilter {
    pub fn is_any(&self) -> bool {
        self.mime_types.is_empty() && self.extensions.is_empty()
    }
}

/// An open dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenRequest {
    pub title: String,
    pub filter: FileFilter,
    pub multiple: bool,
}

/// A save dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveRequest {
    pub name: String,
    pub mime_type: String,
}

/// The platform's file dialogs. `Ok(None)` = the person cancelled.
pub trait Dialogs: Send + Sync {
    fn open(&self, request: &OpenRequest) -> NativeResult<Option<Vec<PathBuf>>>;
    fn save(&self, request: &SaveRequest) -> NativeResult<Option<PathBuf>>;
}

/// A file's name, type and bytes (the bridge's `{name, mimeType, data}`, decoded).
#[derive(Clone, Debug, PartialEq)]
pub struct NamedBytes {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// What share.send hands the share sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShareItem {
    pub title: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub file: Option<NamedBytes>,
}

/// The platform's share sheet. `Ok(false)` = dismissed; `Ok(true)` where it cannot tell.
pub trait ShareSheet: Send + Sync {
    fn share(&self, item: &ShareItem) -> NativeResult<bool>;
}

/// The platform's print dialog for a PDF, PNG or JPEG. `Ok(false)` = cancelled.
pub trait Printer: Send + Sync {
    fn print(&self, job: &NamedBytes) -> NativeResult<bool>;
}

/// An opaque handle of a shown notification (the platform's id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShownHandle(pub u64);

/// The platform's notification center. `on_event(true)` = the person clicked
/// it, `on_event(false)` = it was dismissed or closed; called at most once,
/// from any thread (possibly never: a platform that cannot tell).
pub trait Notifier: Send + Sync {
    fn show(&self, notification: &Shown, on_event: Box<dyn FnOnce(bool) + Send>) -> NativeResult<ShownHandle>;
    /// Withdraw a shown one where the platform allows (best effort).
    fn close(&self, handle: ShownHandle);
}

/// The host's tap callback: the stored `tap` JSON (lifecycle.json's
/// `{type: notificationTap, app, screen, params}`), verbatim; the host sends
/// it to the app instance as `event(tap)`.
pub type OnTap = Arc<dyn Fn(Value) + Send + Sync>;

/// What [`desktop_capabilities`] takes.
#[derive(Clone)]
pub struct DesktopConfig {
    /// The host's display name (Linux notifications' app name, dialog titles).
    pub app_name: String,
    /// macOS: the bundle identifier notifications are posted as (default: the
    /// main bundle's). Windows: the AppUserModelID toasts are posted as.
    pub app_id: Option<String>,
    /// The photo library folder; None = the platform's Pictures folder.
    pub pictures_dir: Option<PathBuf>,
    /// Called when the person taps a notification that carried a `tap`.
    pub on_tap: OnTap,
}

impl DesktopConfig {
    pub fn new(app_name: impl Into<String>, on_tap: OnTap) -> Self {
        DesktopConfig { app_name: app_name.into(), app_id: None, pictures_dir: None, on_tap }
    }
}

/// THIS OS's file dialogs (the Linux portal's file chooser), for a host's
/// own pickers (#EDE-65: eDE's chat composer); `None` = no backend.
pub fn dialogs(config: &DesktopConfig) -> Option<Arc<dyn Dialogs>> {
    platform::dialogs(config)
}

/// The capabilities THIS OS provides, ready for the Engine's broker. One the
/// platform has no backend for is left out (Linux has no share portal,
/// Windows gets no share or print: see README.md), so the broker answers
/// `unavailable` before any call.
pub fn desktop_capabilities(config: DesktopConfig) -> Vec<Arc<dyn Capability>> {
    let dialogs = platform::dialogs(&config);
    let pictures = config.pictures_dir.clone().or_else(dirs::picture_dir);
    let mut out: Vec<Arc<dyn Capability>> = vec![Arc::new(PhotosCapability::new(pictures, dialogs.clone()))];
    if let Some(dialogs) = dialogs {
        out.push(Arc::new(FilesCapability::new(dialogs)));
    }
    if let Some(sheet) = platform::share_sheet(&config) {
        out.push(Arc::new(ShareCapability::new(sheet)));
    }
    if let Some(printer) = platform::printer(&config) {
        out.push(Arc::new(PrintCapability::new(printer)));
    }
    if let Some(notifier) = platform::notifier(&config) {
        out.push(Arc::new(NotificationsCapability::new(notifier, config.on_tap.clone())));
    }
    out
}
