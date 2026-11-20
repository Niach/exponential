//! #EXP-1947 (`packages/domain-contract/fixtures/image-actions.json`, the
//! desktop part of #EXP-1926): THE Copy image of every content picture in
//! the app (results, the results tab, plan pages, comment attachments, the
//! lightbox, Markdown images through the eUI hosts) and of the editors'
//! image blocks. A right-click on a RENDERED picture opens the image menu
//! (pointer = `Copy image` only, the copy concept icon); the picture's
//! ORIGINAL bytes (cache first, else the authenticated attachment fetch, the
//! thumbnail `w` dropped) go to the clipboard as PNG (PNG bytes as they are,
//! anything else decoded and re-encoded: a GIF = its first frame). Success
//! = the success toast `Image copied`, any failure = the error toast.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, ClipboardItem, Entity, ImageFormat, InteractiveElement, ParentElement, Styled, Window};
use gpui_component::{menu::PopupMenuItem, Icon};

use domain::contract::{IMAGE_ACTIONS_COPIED, IMAGE_ACTIONS_COPY_FAILED, IMAGE_ACTIONS_COPY_IMAGE};
use domain::image_actions::{copy_encoding, original_src, CopyEncoding};

use crate::controls::PointerContextMenuExt as _;
use crate::markdown::{AttachmentTransport, ImageCache};
use crate::toast::Toast;

/// The bytes the clipboard takes: PNG as it is, anything else decoded and
/// re-encoded to PNG (an animation = its first frame; never downscaled).
pub(crate) fn png_bytes(bytes: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    match copy_encoding(&bytes) {
        CopyEncoding::Passthrough => Ok(bytes),
        CopyEncoding::Reencode => {
            let decoded = image::load_from_memory(&bytes)?;
            let mut out = std::io::Cursor::new(Vec::new());
            decoded.write_to(&mut out, image::ImageFormat::Png)?;
            Ok(out.into_inner())
        }
    }
}

/// Copy the picture `load` returns (run off the main thread) to the
/// clipboard as PNG, then the toast in `window`.
pub(crate) fn copy_image_with(load: impl FnOnce() -> anyhow::Result<Vec<u8>> + Send + 'static, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    let task = cx.background_executor().spawn(async move { load().and_then(png_bytes) });
    cx.spawn(async move |cx| {
        let result = task.await;
        let _ = handle.update(cx, |_, window, cx| {
            let toast = match result {
                Ok(png) => {
                    cx.write_to_clipboard(ClipboardItem::new_image(&gpui::Image::from_bytes(ImageFormat::Png, png)));
                    Toast::success(IMAGE_ACTIONS_COPIED)
                }
                Err(error) => {
                    log::warn!("copy image failed: {error}");
                    Toast::error(IMAGE_ACTIONS_COPY_FAILED)
                }
            };
            crate::toast::show(toast, window, cx);
        });
    })
    .detach();
}

/// Copy an attachment picture by its `url` (`/api/attachments/{id}`, any
/// query): the decoded cache's original bytes first, else the cache's (or
/// the account's) authenticated transport.
pub(crate) fn copy_attachment_image(url: &str, images: Option<&Entity<ImageCache>>, window: &mut Window, cx: &mut App) {
    let original = original_src(url);
    let cached = images
        .and_then(|images| {
            let cache = images.read(cx);
            cache.ready_image(url).or_else(|| cache.ready_image(&original))
        })
        .or_else(|| crate::markdown::CachedImage::live_encoded(&original, cx));
    if let Some(image) = cached {
        let bytes = image.bytes.clone();
        return copy_image_with(move || Ok(bytes), window, cx);
    }
    let transport: Option<Arc<dyn AttachmentTransport>> = images.and_then(|images| images.read(cx).transport()).or_else(|| crate::queries::attachment_transport(cx));
    copy_image_with(
        move || match transport {
            Some(transport) => transport.fetch(&original),
            None => Err(anyhow::anyhow!("not signed in")),
        },
        window,
        cx,
    );
}

/// The origin of `instance` (scheme, host, port as written) the source
/// rule compares against; empty = none.
fn origin_of(instance: Option<&str>) -> String {
    instance.and_then(|instance| url::Url::parse(instance).ok()).map(|url| url.origin().ascii_serialization()).filter(|origin| origin != "null").unwrap_or_default()
}

/// #EXP-1947 (`imageSource`): may the app fetch the picture `src` at all
/// (ours, data or local, against the active account's instance)? Anything
/// else is never fetched.
pub(crate) fn source_allowed(src: &str, cx: &App) -> bool {
    domain::image_actions::image_source_allowed(src, &origin_of(instance(cx).as_deref()))
}

/// #EXP-1968 (`editModeSource`): what a refused picture's block shows in a
/// desktop editor's EDIT mode, `(text, href)`: THE link (`Some(href)`), else
/// the alt, else its src as plain text. Only for a source the rule refuses.
pub(crate) fn edit_mode_stand_in(src: &str, alt: &str, cx: &App) -> (String, Option<String>) {
    edit_mode_verdict(src, alt, &instance_origin(cx)).unwrap_or_else(|| (alt.trim().to_string(), None))
}

/// [`edit_mode_stand_in`] against an `origin` already read (the rich
/// editor's Send + Sync resolver): `None` = the picture loads.
pub(crate) fn edit_mode_verdict(src: &str, alt: &str, origin: &str) -> Option<(String, Option<String>)> {
    use domain::image_actions::{edit_mode_source, ImageSource};
    match edit_mode_source(src, alt, origin) {
        ImageSource::Link { href, text } => Some((text, Some(href))),
        ImageSource::Text(text) => Some((text, None)),
        ImageSource::Load => None,
    }
}

/// The active account's instance origin the source rule compares against
/// (empty signed out: only relative attachments are ours).
pub(crate) fn instance_origin(cx: &App) -> String {
    origin_of(instance(cx).as_deref())
}

/// The active account's instance url (`None` signed out).
pub(crate) fn instance(cx: &App) -> Option<String> {
    crate::queries::instance_origin(cx)
}

/// #EXP-1947 (`imageSource`): an eUI host's verdict for a content picture
/// against the account's `instance` (its origin: scheme, host, port as
/// written; `None` = no origin, only relative attachments are ours).
pub(crate) fn source_verdict(src: &str, alt: &str, instance: Option<&str>) -> eui_gpui::host::ImageSourceVerdict {
    use domain::image_actions::{image_source, ImageSource};
    use eui_gpui::host::ImageSourceVerdict;
    match image_source(src, alt, &origin_of(instance)) {
        ImageSource::Load => ImageSourceVerdict::Load,
        ImageSource::Link { href, text } => ImageSourceVerdict::Link { href, text },
        ImageSource::Text(text) => ImageSourceVerdict::Text(text),
    }
}

/// #EXP-1947: an eUI host's image menu (`HostPlugin::image_menu`): the
/// picture under THE image menu, copying the target's original bytes (an
/// attachment another view already decoded comes from the cache; else the
/// host's own policed media fetch, its bearer included).
pub(crate) fn eui_image_menu(target: eui_gpui::host::ImageTarget, picture: gpui::AnyElement) -> gpui::AnyElement {
    use gpui::IntoElement as _;
    let wrapper = gpui::div().id(target.id.clone()).size_full().child(picture);
    with_image_menu(
        wrapper,
        Rc::new(move |window, cx| {
            if let Some(image) = crate::markdown::CachedImage::live_encoded(&original_src(&target.src), cx).filter(|_| target.src.starts_with("/api/attachments/")) {
                let bytes = image.bytes.clone();
                return copy_image_with(move || Ok(bytes), window, cx);
            }
            let target = target.clone();
            copy_image_with(move || target.bytes().map_err(anyhow::Error::msg), window, cx)
        }),
    )
    .into_any_element()
}

/// THE image menu's `Copy image` row.
pub(crate) fn copy_image_item(copy: Rc<dyn Fn(&mut Window, &mut App)>) -> PopupMenuItem {
    crate::controls::pointer_label_item(IMAGE_ACTIONS_COPY_IMAGE, false)
        .icon(Icon::from(crate::icons::registry::UI_COPY))
        .on_click(move |_, window, cx| copy(window, cx))
}

/// `picture` (a RENDERED content picture, its element with an id) under
/// THE image menu: a right-click on it opens the menu and is claimed, so
/// the menu of the element around it never opens with it (`pressMenu`).
pub(crate) fn with_image_menu<E>(picture: E, copy: Rc<dyn Fn(&mut Window, &mut App)>) -> impl gpui::IntoElement
where
    E: InteractiveElement + ParentElement + Styled + gpui::IntoElement + 'static,
{
    crate::controls::claim_right_click(picture).pointer_context_menu(move |menu, _, _| menu.item(copy_image_item(copy.clone())))
}

/// [`with_image_menu`] copying the attachment `url` ([`copy_attachment_image`]).
pub(crate) fn with_attachment_menu<E>(picture: E, url: &str, images: Option<&Entity<ImageCache>>) -> impl gpui::IntoElement
where
    E: InteractiveElement + ParentElement + Styled + gpui::IntoElement + 'static,
{
    let (url, images) = (url.to_string(), images.cloned());
    with_image_menu(picture, Rc::new(move |window, cx| copy_attachment_image(&url, images.as_ref(), window, cx)))
}

/// `picture` under THE image menu for the attachment `url` when its slot
/// RENDERED (`ready`); a picture still loading or failed keeps no menu, the
/// press falls through.
pub(crate) fn attachment_menu_when<E>(picture: E, ready: bool, url: &str, images: Option<&Entity<ImageCache>>) -> gpui::AnyElement
where
    E: InteractiveElement + ParentElement + Styled + gpui::IntoElement + 'static,
{
    use gpui::IntoElement as _;
    if ready {
        with_attachment_menu(picture, url, images).into_any_element()
    } else {
        picture.into_any_element()
    }
}

/// The slot's bytes rendered (a picture or its strips).
pub(crate) fn slot_ready(slot: &crate::markdown::ImageSlot) -> bool {
    matches!(slot, crate::markdown::ImageSlot::Ready(_) | crate::markdown::ImageSlot::ReadyTall(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(format: image::ImageFormat) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3, 2).write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    /// `copyEncoding`: PNG bytes go as they are; JPEG / GIF / WebP become a
    /// PNG of the same size; bytes that decode as nothing fail (the error
    /// toast).
    #[test]
    fn the_clipboard_takes_png() {
        let png = encode(image::ImageFormat::Png);
        assert_eq!(png_bytes(png.clone()).unwrap(), png, "passthrough");
        for format in [image::ImageFormat::Jpeg, image::ImageFormat::Gif, image::ImageFormat::Bmp] {
            let out = png_bytes(encode(format)).unwrap();
            assert!(out.starts_with(&domain::image_actions::PNG_SIGNATURE), "{format:?} re-encoded");
            let back = image::load_from_memory(&out).unwrap();
            assert_eq!((back.width(), back.height()), (3, 2), "{format:?}: never downscaled");
        }
        assert!(png_bytes(b"<svg/>".to_vec()).is_err());
        assert!(png_bytes(Vec::new()).is_err());
    }
}
