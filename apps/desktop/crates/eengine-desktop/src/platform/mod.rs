//! The platform backends behind the seams, one module per OS.

use std::sync::Arc;

use crate::{DesktopConfig, Dialogs, Notifier, Printer, ShareSheet};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(target_os = "windows")]
use windows as os;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn dialogs(config: &DesktopConfig) -> Option<Arc<dyn Dialogs>> {
    os::dialogs(config)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn share_sheet(config: &DesktopConfig) -> Option<Arc<dyn ShareSheet>> {
    os::share_sheet(config)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn printer(config: &DesktopConfig) -> Option<Arc<dyn Printer>> {
    os::printer(config)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn notifier(config: &DesktopConfig) -> Option<Arc<dyn Notifier>> {
    os::notifier(config)
}

// Any other OS: the Pictures folder only (photos.pick answers `unavailable`).
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn dialogs(_: &DesktopConfig) -> Option<Arc<dyn Dialogs>> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn share_sheet(_: &DesktopConfig) -> Option<Arc<dyn ShareSheet>> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn printer(_: &DesktopConfig) -> Option<Arc<dyn Printer>> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn notifier(_: &DesktopConfig) -> Option<Arc<dyn Notifier>> {
    None
}
