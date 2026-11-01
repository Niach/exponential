//! Hot reload: inotify on the ui directory (notify), debounced, then
//! [`Runtime::reload`](crate::Runtime::reload).

use std::{
    io,
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

use notify::{RecursiveMode, Watcher as _};

use crate::{Change, Runtime};

/// Edits that land this close together reload once (an editor's write +
/// rename, `git revert` touching several files).
pub const DEBOUNCE: Duration = Duration::from_millis(120);

/// Watches while it lives; dropping it closes the channel and the
/// reload thread ends.
pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

/// Does an event on `path` concern a document or the theme? (`.git/`,
/// the atomic-write temporaries and editor swap files do not.)
fn relevant(dir: &Path, path: &Path) -> bool {
    path.parent() == Some(dir)
        && path.file_name().and_then(|f| f.to_str()).is_some_and(|f| f.ends_with(".json") && !f.starts_with('.'))
}

/// Reload `runtime` whenever a file in its directory changes and hand the
/// changes (never empty) to `on_change`. A reload that fails (the directory
/// went away, git broke) reports nothing and the last good state stays.
pub fn watch(runtime: Arc<Mutex<Runtime>>, mut on_change: impl FnMut(Vec<Change>) + Send + 'static) -> io::Result<Watcher> {
    let dir = runtime.lock().unwrap_or_else(|p| p.into_inner()).dir().to_path_buf();
    let dir = dir.canonicalize().unwrap_or(dir);
    let (tx, rx) = mpsc::channel::<()>();
    let filter_dir = dir.clone();
    let mut inner = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            if event.paths.iter().any(|p| relevant(&filter_dir, p)) {
                let _ = tx.send(());
            }
        }
    })
    .map_err(io::Error::other)?;
    inner.watch(&dir, RecursiveMode::NonRecursive).map_err(io::Error::other)?;
    thread::Builder::new().name("ede-ui-watch".into()).spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(DEBOUNCE).is_ok() {}
            let changes = runtime.lock().unwrap_or_else(|p| p.into_inner()).reload();
            if let Ok(changes) = changes {
                if !changes.is_empty() {
                    on_change(changes);
                }
            }
        }
    })?;
    Ok(Watcher { _inner: inner })
}
