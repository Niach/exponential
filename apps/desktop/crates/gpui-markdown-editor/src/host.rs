use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow};

/// Image payload handed to the host's storage policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PastedImage {
    LocalPath(PathBuf),
    Encoded {
        bytes: Arc<[u8]>,
        suggested_extension: String,
    },
}

/// Materialized Markdown image information returned by the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageTarget {
    pub alt: String,
    pub source: String,
}

/// EXP-261 vendoring: how a markdown image `src` resolves for display when a
/// host [`ImageSourceResolver`] is installed.
#[derive(Clone)]
pub enum ImageSourceResolution {
    /// Decoded image bytes ready to render (the host's authenticated cache).
    Decoded(Arc<gpui::Image>),
    /// The host is still fetching the bytes — render a loading placeholder.
    Pending,
    /// The fetch failed — render the unavailable placeholder.
    Failed,
}

/// EXP-261 vendoring: kind of an inline reference decorated by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    /// `@email` member mention.
    Mention,
    /// `#IDENT` issue reference.
    IssueRef,
}

/// EXP-423: a chip's leading status icon — an embedded SVG asset painted
/// into the chip's NBSP gutter in the resolved status' color. Domain-free:
/// the host resolves status → glyph → asset path.
#[derive(Clone, Debug, PartialEq)]
pub struct ChipIcon {
    /// Embedded SVG asset path (e.g. `icons/progress-2-4.svg`).
    pub svg_path: gpui::SharedString,
    /// Pre-resolved tint (status color).
    pub color: gpui::Hsla,
}

/// EXP-261 vendoring: a resolved reference span inside a block's visible
/// text. The host returns only spans that RESOLVE (unknown identifiers stay
/// plain text — the cross-client contract).
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceSpan {
    /// Byte range in the scanned visible text.
    pub range: std::ops::Range<usize>,
    pub kind: ReferenceKind,
    /// EXP-322 vendoring: display-only glyphs rendered right after `range` —
    /// the issue title, so a chip reads `#EXP-238 Fix login flow` while
    /// editing (web draws the same text from a CSS `::after`). NEVER part of
    /// the document, so serialization is untouched; newlines are flattened.
    pub display_suffix: Option<gpui::SharedString>,
    /// EXP-423: the status icon painted into the chip's leading gutter.
    /// `None` (mentions, unresolved statuses) renders no gutter at all.
    pub icon: Option<ChipIcon>,
}

/// EXP-261 vendoring: host hook decorating `@email` / `#IDENT` tokens as
/// pills (render-time only — serialization is untouched, the tokens are plain
/// GFM text). Send + Sync like the other environment hooks; the host backs it
/// with a snapshot it refreshes on the UI thread.
pub trait ReferenceDecorator: Send + Sync + 'static {
    fn scan(&self, text: &str) -> Vec<ReferenceSpan>;
}

/// EXP-261 vendoring: host hook resolving markdown image sources the default
/// local/remote classification cannot handle (relative attachment URLs that
/// need authentication, staged `draft://` bytes). Return `None` to fall back
/// to the default resolution. Send + Sync so it can ride the shared editor
/// environment; the host keeps the backing state in a mutex it refreshes from
/// the UI thread (repaint via `set_environment`, which rebuilds image
/// runtimes).
pub trait ImageSourceResolver: Send + Sync + 'static {
    fn resolve(&self, src: &str) -> Option<ImageSourceResolution>;

    /// EXP-261 vendoring: the image's natural pixel size, when the host knows
    /// it (Exponential syncs probed attachment dimensions). Without it an
    /// `img` element fills its whole max-width box and letterboxes the picture
    /// inside, so the overlay controls cannot sit on the picture's edges and a
    /// resize drag has no honest upper clamp. Default `None` keeps the
    /// fill-and-letterbox behaviour for hosts that cannot answer.
    fn natural_size(&self, _src: &str) -> Option<(f32, f32)> {
        None
    }
}

/// Host policy for materializing pasted images.
///
/// Implementations may copy files or persist clipboard bytes. This operation is
/// synchronous so insertion, selection, and undo capture remain one transaction.
pub trait ImagePasteHandler: Send + Sync + 'static {
    fn materialize(
        &self,
        source: PastedImage,
        document_base_dir: Option<&Path>,
    ) -> Result<ImageTarget>;
}

/// Default policy: insert local paths without copying. Encoded clipboard images
/// require an explicit host policy because the component does not own storage.
#[derive(Default)]
pub struct InsertOriginalImagePath;

impl ImagePasteHandler for InsertOriginalImagePath {
    fn materialize(
        &self,
        source: PastedImage,
        _document_base_dir: Option<&Path>,
    ) -> Result<ImageTarget> {
        match source {
            PastedImage::LocalPath(path) => Ok(ImageTarget {
                alt: image_alt(&path),
                source: markdown_path(&path),
            }),
            PastedImage::Encoded { .. } => Err(anyhow!(
                "pasting encoded image bytes requires a host ImagePasteHandler"
            )),
        }
    }
}

pub(crate) fn default_image_paste_handler() -> Arc<dyn ImagePasteHandler> {
    Arc::new(InsertOriginalImagePath)
}

fn image_alt(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("image")
        .to_string()
}

fn markdown_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
