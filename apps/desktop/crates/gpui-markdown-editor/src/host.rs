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
