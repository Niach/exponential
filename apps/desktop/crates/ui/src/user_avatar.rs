//! Shared user-avatar rendering (EXP-311): the signed-in person's profile
//! image with an initials fallback, unified with the web sidebar avatar
//! (`Avatar` + `AvatarImage`/`AvatarFallback{getInitials}`).
//!
//! gpui has no HTTP client installed (`NullHttpClient`), so `Avatar::src(url)`
//! with a remote URI would silently render nothing. Like the markdown
//! `ImageCache` (`markdown/editor.rs`), bytes are fetched on the background
//! executor — here through the plain shared reqwest client WITHOUT auth
//! headers: `users.image` is an absolute third-party URL (Google/GitHub
//! avatar), and the instance bearer must never ride to a third-party host.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, AppContext as _, Entity, Global};
use gpui_component::avatar::Avatar;
use gpui_component::Sizable as _;

use crate::markdown::sniff_format;

#[derive(Clone)]
enum ImageSlot {
    Loading,
    Ready(Arc<gpui::Image>),
    /// A fetch failure is never permanent: the timestamp gates a re-fetch on
    /// the next render after [`RETRY_AFTER`].
    Failed(std::time::Instant),
}

/// How long a failed fetch is displayed (as initials) before a retry.
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// Process-wide cache of decoded avatar images, keyed by URL. Lives for the
/// session — avatar URLs are stable and tiny.
pub struct AvatarCache {
    slots: HashMap<String, ImageSlot>,
}

struct AvatarCacheGlobal(Entity<AvatarCache>);
impl Global for AvatarCacheGlobal {}

impl AvatarCache {
    pub fn global(cx: &mut App) -> Entity<AvatarCache> {
        if let Some(global) = cx.try_global::<AvatarCacheGlobal>() {
            return global.0.clone();
        }
        let cache = cx.new(|_| AvatarCache {
            slots: HashMap::new(),
        });
        cx.set_global(AvatarCacheGlobal(cache.clone()));
        cache
    }

    fn slot(&mut self, url: &str, cx: &mut gpui::Context<Self>) -> ImageSlot {
        if let Some(slot) = self.slots.get(url) {
            match slot {
                ImageSlot::Failed(at) if at.elapsed() >= RETRY_AFTER => {}
                _ => return slot.clone(),
            }
        }
        self.slots.insert(url.to_string(), ImageSlot::Loading);
        let url_owned = url.to_string();
        cx.spawn(async move |this, cx| {
            let fetch_url = url_owned.clone();
            let result = cx
                .background_executor()
                .spawn(async move { fetch_avatar_bytes(&fetch_url) })
                .await;
            this.update(cx, |cache, cx| {
                let slot = match result {
                    Ok(bytes) => {
                        let format = sniff_format("", &bytes);
                        ImageSlot::Ready(Arc::new(gpui::Image::from_bytes(format, bytes)))
                    }
                    Err(error) => {
                        log::warn!("avatar fetch failed for {url_owned}: {error}");
                        ImageSlot::Failed(std::time::Instant::now())
                    }
                };
                cache.slots.insert(url_owned.clone(), slot);
                cx.notify();
            })
            .ok();
        })
        .detach();
        ImageSlot::Loading
    }
}

/// Plain unauthenticated GET through the shared client (EXP-304: the ONE
/// HTTP stack). Blocking — background executor only.
fn fetch_avatar_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let response = api::http::shared().get(url).send()?.error_for_status()?;
    Ok(response.bytes()?.to_vec())
}

/// The decoded profile image for `image_url` if it is (or becomes) available
/// — triggers the fetch on first ask. Only absolute http(s) URLs are
/// fetchable (Better Auth `image` is always absolute); anything else falls
/// back to initials.
pub fn cached_avatar_image(cx: &mut App, image_url: Option<&str>) -> Option<Arc<gpui::Image>> {
    let url = image_url?.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let cache = AvatarCache::global(cx);
    cache.update(cx, |cache, cx| match cache.slot(url, cx) {
        ImageSlot::Ready(image) => Some(image),
        _ => None,
    })
}

/// The unified user avatar: profile image when the bytes have landed,
/// hue-hashed initials otherwise (`Avatar::name` initials match the web's
/// `getInitials`). `label` is the display name with the email fallback —
/// never empty.
pub fn user_avatar(
    label: &str,
    image_url: Option<&str>,
    size: gpui_component::Size,
    cx: &mut App,
) -> Avatar {
    let avatar = Avatar::new()
        .name(gpui::SharedString::from(label.to_string()))
        .with_size(size);
    match cached_avatar_image(cx, image_url) {
        Some(image) => avatar.src(image),
        None => avatar,
    }
}

/// Chrome surfaces (rail button — web-sidebar parity, EXP-311) show only the
/// first name; the full name + email live in account settings.
pub fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::first_name;

    #[test]
    fn first_name_takes_first_word() {
        assert_eq!(first_name("Danny Strähhuber"), "Danny");
        assert_eq!(first_name("  Ada   Lovelace  "), "Ada");
        assert_eq!(first_name("mononym"), "mononym");
        assert_eq!(first_name("user@example.com"), "user@example.com");
        assert_eq!(first_name(""), "");
    }
}
