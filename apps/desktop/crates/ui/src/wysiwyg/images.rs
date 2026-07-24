//! EXP-261: host side of the vendored editor's image pipeline — the
//! Send+Sync state shared with the editor environment (paste handler +
//! source resolver), bridged to the authenticated [`ImageCache`] transport by
//! [`super::WysiwygDescription`] on the UI thread.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use gpui_markdown_editor::{
    ImagePasteHandler, ImageSourceResolution, ImageSourceResolver, ImageTarget, PastedImage,
};

use crate::markdown::image_paste::{
    new_draft_url, pasted_image_parts, read_image_file, validate_image, DRAFT_SCHEME,
};
use crate::markdown::{sniff_format, StagedImage};

/// State shared between the vendored editor (via `Send + Sync` trait objects
/// in its environment) and the wrapper on the UI thread.
#[derive(Default)]
pub(crate) struct SharedImageState {
    /// Resolution per query-stripped image src. The wrapper keeps this in
    /// sync with the [`crate::markdown::ImageCache`]; the paste handler
    /// inserts `draft://` bytes directly so a pasted image renders on the
    /// very next frame.
    pub resolutions: Mutex<HashMap<String, ImageSourceResolution>>,
    /// Images staged by the paste handler, drained by the wrapper on the next
    /// change event (detail mode uploads them; dialog mode keeps them for
    /// submit).
    pub paste_inbox: Mutex<Vec<StagedImage>>,
}

/// Query-stripped cache key (the `?w=` display width never affects identity —
/// mirrors the block editor's `ImageCache` keying).
pub(crate) fn cache_key(src: &str) -> &str {
    src.split(['?', '#']).next().unwrap_or(src)
}

/// Whether the wrapper owns fetching for this src (vs the vendored default
/// local/remote classification).
pub(crate) fn is_hosted_src(src: &str) -> bool {
    src.starts_with(DRAFT_SCHEME) || cache_key(src).starts_with("/api/attachments/")
}

pub(crate) struct WysiwygImageResolver {
    pub state: Arc<SharedImageState>,
}

impl ImageSourceResolver for WysiwygImageResolver {
    fn resolve(&self, src: &str) -> Option<ImageSourceResolution> {
        if !is_hosted_src(src) {
            return None;
        }
        let resolutions = self.state.resolutions.lock().ok()?;
        Some(
            resolutions
                .get(cache_key(src))
                .cloned()
                // Not synced yet (first frame after typing the markdown):
                // report Pending; the wrapper's next sync fills it in.
                .unwrap_or(ImageSourceResolution::Pending),
        )
    }
}

pub(crate) struct WysiwygPasteHandler {
    pub state: Arc<SharedImageState>,
}

impl ImagePasteHandler for WysiwygPasteHandler {
    fn materialize(
        &self,
        source: PastedImage,
        _document_base_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<ImageTarget> {
        let (filename, content_type, bytes) = match source {
            PastedImage::LocalPath(path) => read_image_file(&path)?,
            PastedImage::Encoded {
                bytes,
                suggested_extension,
            } => {
                let format = format_for_extension(&suggested_extension);
                let (mime, filename) = pasted_image_parts(format);
                (filename, mime.to_string(), bytes.to_vec())
            }
        };
        validate_image(&content_type, bytes.len()).map_err(|message| anyhow!(message))?;

        let draft_url = new_draft_url();
        let image = Arc::new(gpui::Image::from_bytes(
            sniff_format(&content_type, &bytes),
            bytes.clone(),
        ));
        if let Ok(mut resolutions) = self.state.resolutions.lock() {
            resolutions.insert(draft_url.clone(), ImageSourceResolution::Decoded(image));
        }
        let alt = filename
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_string())
            .unwrap_or_else(|| filename.clone());
        if let Ok(mut inbox) = self.state.paste_inbox.lock() {
            inbox.push(StagedImage {
                draft_url: draft_url.clone(),
                filename,
                content_type,
                bytes: Arc::new(bytes),
            });
        }
        Ok(ImageTarget {
            alt,
            source: draft_url,
        })
    }
}

fn format_for_extension(extension: &str) -> gpui::ImageFormat {
    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => gpui::ImageFormat::Jpeg,
        "webp" => gpui::ImageFormat::Webp,
        "gif" => gpui::ImageFormat::Gif,
        _ => gpui::ImageFormat::Png,
    }
}

/// Pointer-identity-aware equality for resolution maps (Decoded holds an Arc).
pub(crate) fn resolutions_equal(
    a: &HashMap<String, ImageSourceResolution>,
    b: &HashMap<String, ImageSourceResolution>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(key, left)| {
            b.get(key).is_some_and(|right| match (left, right) {
                (ImageSourceResolution::Decoded(x), ImageSourceResolution::Decoded(y)) => {
                    Arc::ptr_eq(x, y)
                }
                (ImageSourceResolution::Pending, ImageSourceResolution::Pending) => true,
                (ImageSourceResolution::Failed, ImageSourceResolution::Failed) => true,
                _ => false,
            })
        })
}
