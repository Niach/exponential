//! Linux: the xdg-desktop-portal FileChooser and Print (ashpd), D-Bus
//! notifications (notify-rust). No share portal exists, so `share` is not
//! provided.
//!
//! THE ZBUS/TOKIO TRAP (crates/ui/src/file_picker.rs): ashpd and notify-rust
//! talk D-Bus through zbus. This crate asks for zbus' `async-io` backend, but
//! cargo unifies features, and a zbus built with `tokio` panics ("there is no
//! reactor running") on any thread without a Tokio runtime context. So EVERY
//! zbus call here runs on a FRESH thread (never the caller's: it may be a
//! Tokio worker, where `block_on` panics too) inside this crate's own
//! multi-thread runtime: futures through `Runtime::block_on`, blocking APIs
//! with the runtime entered. Both backends then work.

use std::fs::File;
use std::future::Future;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use ashpd::desktop::file_chooser::{FileFilter as PortalFilter, SelectedFiles};
use ashpd::desktop::print::{PreparePrintOptions, PrintOptions, PrintProxy};
use ashpd::desktop::ResponseError;

use crate::{
    DesktopConfig, Dialogs, FileFilter, NamedBytes, NativeError, NativeResult, Notifier, OpenRequest, Printer, SaveRequest, ShareSheet, Shown,
    ShownHandle,
};

pub(super) fn dialogs(_: &DesktopConfig) -> Option<Arc<dyn Dialogs>> {
    Some(Arc::new(PortalDialogs))
}

pub(super) fn share_sheet(_: &DesktopConfig) -> Option<Arc<dyn ShareSheet>> {
    None
}

pub(super) fn printer(config: &DesktopConfig) -> Option<Arc<dyn Printer>> {
    Some(Arc::new(PortalPrinter { app_name: config.app_name.clone() }))
}

pub(super) fn notifier(config: &DesktopConfig) -> Option<Arc<dyn Notifier>> {
    Some(Arc::new(DbusNotifier { app_name: config.app_name.clone() }))
}

fn failed(message: impl Into<String>) -> NativeError {
    NativeError::Failed(message.into())
}

fn runtime() -> NativeResult<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("eengine-desktop-dbus")
                .enable_all()
                .build()
                .map_err(|e| log::warn!("eengine-desktop: no D-Bus runtime: {e}"))
                .ok()
        })
        .as_ref()
        .ok_or_else(|| failed("no D-Bus runtime"))
}

/// Drive a zbus future on a fresh thread inside the private runtime.
fn block_on<T, F>(make: impl FnOnce() -> F + Send + 'static) -> NativeResult<T>
where
    F: Future<Output = T>,
    T: Send + 'static,
{
    let runtime = runtime()?;
    std::thread::Builder::new()
        .name("eengine-desktop-portal".into())
        .spawn(move || runtime.block_on(make()))
        .map_err(|e| failed(format!("cannot start the portal thread: {e}")))?
        .join()
        .map_err(|_| failed("the portal call panicked"))
}

/// Run blocking zbus code on a fresh thread with the private runtime entered.
fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> NativeResult<T> {
    let runtime = runtime()?;
    std::thread::Builder::new()
        .name("eengine-desktop-dbus-call".into())
        .spawn(move || {
            let _entered = runtime.enter();
            work()
        })
        .map_err(|e| failed(format!("cannot start the D-Bus thread: {e}")))?
        .join()
        .map_err(|_| failed("the D-Bus call panicked"))
}

/// Like [`blocking`] but detached (a notification's click wait).
fn detached(work: impl FnOnce() + Send + 'static) {
    let Ok(runtime) = runtime() else { return };
    let _ = std::thread::Builder::new().name("eengine-notification".into()).spawn(move || {
        let _entered = runtime.enter();
        work()
    });
}

fn cancelled(error: &ashpd::Error) -> bool {
    matches!(error, ashpd::Error::Response(ResponseError::Cancelled))
}

fn portal_filter(filter: &FileFilter, label: &str) -> Option<PortalFilter> {
    if filter.is_any() {
        return None;
    }
    let mut portal = PortalFilter::new(label);
    for mime in &filter.mime_types {
        portal = portal.mimetype(mime);
    }
    for ext in &filter.extensions {
        portal = portal.glob(&format!("*.{ext}")).glob(&format!("*.{}", ext.to_ascii_uppercase()));
    }
    Some(portal)
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

struct PortalDialogs;

impl Dialogs for PortalDialogs {
    fn open(&self, request: &OpenRequest) -> NativeResult<Option<Vec<PathBuf>>> {
        let request = request.clone();
        block_on(move || async move {
            let mut chooser = SelectedFiles::open_file().title(request.title.as_str()).modal(true).multiple(request.multiple);
            if let Some(filter) = portal_filter(&request.filter, "Supported files") {
                chooser = chooser.filter(filter);
            }
            match chooser.send().await.and_then(|r| r.response()) {
                Ok(files) => Ok(Some(files.uris().iter().filter_map(|u| uri_path(u.as_str())).collect())),
                Err(error) if cancelled(&error) => Ok(None),
                Err(error) => Err(failed(format!("file chooser: {error}"))),
            }
        })?
    }

    fn save(&self, request: &SaveRequest) -> NativeResult<Option<PathBuf>> {
        let request = request.clone();
        block_on(move || async move {
            let chooser = SelectedFiles::save_file().title("Save").modal(true).current_name(request.name.as_str());
            match chooser.send().await.and_then(|r| r.response()) {
                Ok(files) => Ok(files.uris().iter().find_map(|u| uri_path(u.as_str()))),
                Err(error) if cancelled(&error) => Ok(None),
                Err(error) => Err(failed(format!("file chooser: {error}"))),
            }
        })?
    }
}

/// The Print portal takes a file descriptor: the bytes go to a temp file
/// first (removed after the call). PDFs print as is; PNG/JPEG go through the
/// backend's (CUPS') image filters.
struct PortalPrinter {
    app_name: String,
}

impl Printer for PortalPrinter {
    fn print(&self, job: &NamedBytes) -> NativeResult<bool> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!("eengine-print-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&path, &job.bytes).map_err(|e| failed(format!("cannot write a temp file: {e}")))?;
        let file = File::open(&path).map_err(|e| failed(format!("cannot read the temp file: {e}")));
        let title = if job.name.is_empty() { self.app_name.clone() } else { job.name.clone() };
        let result = file.and_then(|file| {
            block_on(move || async move {
                let proxy = PrintProxy::new().await.map_err(|e| failed(format!("print portal: {e}")))?;
                let prepared = match proxy
                    .prepare_print(None, &title, Default::default(), Default::default(), PreparePrintOptions::default().set_modal(true))
                    .await
                    .and_then(|r| r.response())
                {
                    Ok(prepared) => prepared,
                    Err(error) if cancelled(&error) => return Ok(false),
                    Err(error) => return Err(failed(format!("print portal: {error}"))),
                };
                let options = PrintOptions::default().set_token(prepared.token).set_modal(true);
                match proxy.print(None, &title, &file.as_fd(), options).await.and_then(|r| r.response()) {
                    Ok(()) => Ok(true),
                    Err(error) if cancelled(&error) => Ok(false),
                    Err(error) => Err(failed(format!("print portal: {error}"))),
                }
            })?
        });
        let _ = std::fs::remove_file(&path);
        result
    }
}

/// org.freedesktop.Notifications via notify-rust; a click = the `default`
/// action, waited for on a thread per notification; close = CloseNotification.
struct DbusNotifier {
    app_name: String,
}

impl Notifier for DbusNotifier {
    fn show(&self, notification: &Shown, on_event: Box<dyn FnOnce(bool) + Send>) -> NativeResult<ShownHandle> {
        let app_name = self.app_name.clone();
        let shown = notification.clone();
        let handle = blocking(move || {
            notify_rust::Notification::new()
                .appname(&app_name)
                .summary(&shown.title)
                .body(shown.body.as_deref().unwrap_or(""))
                .action("default", "Open")
                .show()
        })?
        .map_err(|e| failed(format!("cannot post: {e}")))?;
        let id = handle.id();
        detached(move || {
            let _ = handle.wait_for_response(move |response: &notify_rust::NotificationResponse| {
                on_event(matches!(response, notify_rust::NotificationResponse::Default));
            });
        });
        Ok(ShownHandle(u64::from(id)))
    }

    fn close(&self, handle: ShownHandle) {
        let Ok(id) = u32::try_from(handle.0) else { return };
        let closed = block_on(move || async move {
            let connection = zbus::Connection::session().await?;
            connection
                .call_method(
                    Some("org.freedesktop.Notifications"),
                    "/org/freedesktop/Notifications",
                    Some("org.freedesktop.Notifications"),
                    "CloseNotification",
                    &(id,),
                )
                .await
                .map(|_| ())
        });
        if let Ok(Err(error)) = closed {
            log::warn!("eengine-desktop: CloseNotification {id}: {error}");
        }
    }
}
