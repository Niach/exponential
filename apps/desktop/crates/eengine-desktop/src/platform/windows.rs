//! Windows: IFileOpenDialog/IFileSaveDialog (COM, on a fresh STA thread per
//! call) and toasts (notify-rust → WinRT). NOT provided: `share` (the
//! DataTransferManager needs a WinRT interop window handle this host-agnostic
//! crate does not have) and `print` (the shell's `print` verb prints silently
//! through the default app: no dialog, so no consent; README.md).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_CANCELLED;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
    SIGDN_FILESYSPATH,
};

use crate::{DesktopConfig, Dialogs, NativeError, NativeResult, Notifier, OpenRequest, Printer, SaveRequest, ShareSheet, Shown, ShownHandle};

pub(super) fn dialogs(_: &DesktopConfig) -> Option<Arc<dyn Dialogs>> {
    Some(Arc::new(ComDialogs))
}

pub(super) fn share_sheet(_: &DesktopConfig) -> Option<Arc<dyn ShareSheet>> {
    None
}

pub(super) fn printer(_: &DesktopConfig) -> Option<Arc<dyn Printer>> {
    None
}

pub(super) fn notifier(config: &DesktopConfig) -> Option<Arc<dyn Notifier>> {
    Some(Arc::new(ToastNotifier { app_id: config.app_id.clone(), next: AtomicU64::new(1) }))
}

fn failed(message: impl Into<String>) -> NativeError {
    NativeError::Failed(message.into())
}

/// Run `work` on a fresh single-threaded COM apartment.
fn on_sta<T: Send + 'static>(work: impl FnOnce() -> NativeResult<T> + Send + 'static) -> NativeResult<T> {
    std::thread::Builder::new()
        .name("eengine-desktop-dialog".into())
        .spawn(move || {
            let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if initialized.is_err() {
                return Err(failed(format!("COM: {initialized:?}")));
            }
            let result = work();
            unsafe { CoUninitialize() };
            result
        })
        .map_err(|e| failed(format!("cannot start the dialog thread: {e}")))?
        .join()
        .map_err(|_| failed("the dialog panicked"))?
}

fn is_cancelled(error: &windows::core::Error) -> bool {
    error.code() == ERROR_CANCELLED.to_hresult()
}

fn com(error: windows::core::Error) -> NativeError {
    failed(format!("file dialog: {error}"))
}

fn item_path(item: &IShellItem) -> Option<PathBuf> {
    unsafe {
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}

struct ComDialogs;

impl Dialogs for ComDialogs {
    fn open(&self, request: &OpenRequest) -> NativeResult<Option<Vec<PathBuf>>> {
        let request = request.clone();
        on_sta(move || unsafe {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(com)?;
            let mut options = dialog.GetOptions().map_err(com)? | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST;
            if request.multiple {
                options |= FOS_ALLOWMULTISELECT;
            }
            dialog.SetOptions(options).map_err(com)?;
            dialog.SetTitle(&HSTRING::from(request.title.as_str())).map_err(com)?;
            // Kept alive until Show returns: the spec points into them.
            let label = HSTRING::from("Supported files");
            let pattern = HSTRING::from(request.filter.extensions.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";"));
            if !request.filter.extensions.is_empty() {
                let spec = [COMDLG_FILTERSPEC { pszName: PCWSTR(label.as_ptr()), pszSpec: PCWSTR(pattern.as_ptr()) }];
                dialog.SetFileTypes(&spec).map_err(com)?;
            }
            match dialog.Show(None) {
                Ok(()) => {}
                Err(error) if is_cancelled(&error) => return Ok(None),
                Err(error) => return Err(com(error)),
            }
            let items = dialog.GetResults().map_err(com)?;
            let count = items.GetCount().map_err(com)?;
            let mut paths = Vec::new();
            for index in 0..count {
                if let Some(path) = items.GetItemAt(index).ok().as_ref().and_then(item_path) {
                    paths.push(path);
                }
            }
            Ok(Some(paths))
        })
    }

    fn save(&self, request: &SaveRequest) -> NativeResult<Option<PathBuf>> {
        let request = request.clone();
        on_sta(move || unsafe {
            let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).map_err(com)?;
            let options = dialog.GetOptions().map_err(com)? | FOS_FORCEFILESYSTEM;
            dialog.SetOptions(options).map_err(com)?;
            dialog.SetFileName(&HSTRING::from(request.name.as_str())).map_err(com)?;
            match dialog.Show(None) {
                Ok(()) => {}
                Err(error) if is_cancelled(&error) => return Ok(None),
                Err(error) => return Err(com(error)),
            }
            let item = dialog.GetResult().map_err(com)?;
            Ok(item_path(&item))
        })
    }
}

/// Toasts through notify-rust (WinRT). A body click = `Default`; a shown
/// toast cannot be withdrawn through notify-rust, so close only drops it.
struct ToastNotifier {
    app_id: Option<String>,
    next: AtomicU64,
}

impl Notifier for ToastNotifier {
    fn show(&self, notification: &Shown, on_event: Box<dyn FnOnce(bool) + Send>) -> NativeResult<ShownHandle> {
        let mut toast = notify_rust::Notification::new();
        toast.summary(&notification.title).body(notification.body.as_deref().unwrap_or(""));
        if let Some(app_id) = &self.app_id {
            toast.app_id(app_id);
        }
        let handle = toast.show().map_err(|e| failed(format!("cannot post: {e}")))?;
        let _ = std::thread::Builder::new().name("eengine-notification".into()).spawn(move || {
            let _ = handle.wait_for_response(move |response: &notify_rust::NotificationResponse| {
                on_event(matches!(response, notify_rust::NotificationResponse::Default));
            });
        });
        Ok(ShownHandle(self.next.fetch_add(1, Ordering::Relaxed)))
    }

    fn close(&self, _handle: ShownHandle) {}
}
