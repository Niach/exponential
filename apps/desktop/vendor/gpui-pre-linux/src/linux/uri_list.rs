// Exponential patch (#ERR-4): the dropped data of an XDND / Wayland drag,
// turned into file paths only when it is a `text/uri-list` (RFC 2483).
// Upstream ran `Url::parse(line).log_err()` on every line of ANY accepted
// drag (X11 also accepts TEXT/STRING/UTF8_STRING/text/plain), so dropping a
// 34-line text logged 34 "relative URL without a base" errors.

use std::path::PathBuf;

use smallvec::SmallVec;
use url::Url;

/// The local file paths of a dropped `text/uri-list`: CRLF or LF lines,
/// `#` comment lines and empty lines skipped (RFC 2483 §5). A non-uri-list
/// drag (plain text) yields nothing and logs nothing: gpui has no text-drop
/// input event to deliver it as.
pub(crate) fn dropped_paths(data: &str, is_uri_list: bool) -> SmallVec<[PathBuf; 2]> {
    if !is_uri_list {
        return SmallVec::new();
    }
    data.lines()
        .map(|line| line.trim_end_matches('\r').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| match Url::parse(line) {
            Ok(url) => Some(url),
            Err(err) => {
                log::warn!("dropped uri-list line is not a URI ({err}): {line:?}");
                None
            }
        })
        .filter_map(|url| match url.to_file_path() {
            Ok(path) => Some(path),
            Err(()) => {
                log::warn!("dropped URI is not a local file: {url}");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counter;
    static RECORDS: AtomicUsize = AtomicUsize::new(0);
    impl log::Log for Counter {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, _: &log::Record) {
            RECORDS.fetch_add(1, Ordering::SeqCst);
        }
        fn flush(&self) {}
    }

    #[test]
    fn a_uri_list_with_a_comment_and_crlf_gives_its_paths() {
        let list = "# dragged from a file manager\r\nfile:///home/u/a.txt\r\n\r\nfile:///home/u/b%20c.png\r\n";
        let paths = dropped_paths(list, true);
        assert_eq!(
            paths.as_slice(),
            [PathBuf::from("/home/u/a.txt"), PathBuf::from("/home/u/b c.png")]
        );
    }

    #[test]
    fn plain_text_gives_no_paths_and_logs_nothing() {
        let _ = log::set_logger(&Counter);
        log::set_max_level(log::LevelFilter::Trace);
        let before = RECORDS.load(Ordering::SeqCst);
        let text = (0..34).map(|i| format!("line {i} of a text drop")).collect::<Vec<_>>().join("\n");
        assert!(dropped_paths(&text, false).is_empty());
        assert!(dropped_paths("file:///home/u/a.txt", false).is_empty());
        assert_eq!(RECORDS.load(Ordering::SeqCst), before);
    }
}
