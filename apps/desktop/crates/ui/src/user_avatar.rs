//! Shared user-avatar rendering (EXP-311): a person's profile image with an
//! initials fallback, unified across the four clients — EXP-698 r4 put the
//! FALLBACK on one contract too ([`avatar_hue_index`] over the user id +
//! [`initials`], both mirrored by Android `Avatars.kt` and iOS
//! `UserAvatar.swift`), so a picture-less team reads as distinct people
//! everywhere.
//!
//! gpui has no HTTP client installed (`NullHttpClient`), so `Avatar::src(url)`
//! with a remote URI would silently render nothing. Like the markdown
//! `ImageCache` (`markdown/editor.rs`), bytes are fetched on the background
//! executor — here through the plain shared reqwest client WITHOUT auth
//! headers: `users.image` is an absolute third-party URL (Google/GitHub
//! avatar), and the instance bearer must never ride to a third-party host.

use std::collections::HashMap;
use std::sync::Arc;

use domain::team_image::{team_mark, TeamMark, TEAM_IMAGE};
use gpui::{
    div, img, px, App, AppContext as _, Div, Entity, Global, IntoElement as _, ParentElement as _,
    Pixels, StyledImage as _, Styled as _,
};
use gpui_component::avatar::Avatar;
use gpui_component::{Icon, Sizable as _, Size};

use crate::icons::registry;
use crate::markdown::{sniff_format, ImageCache, ImageSlot as MarkdownImageSlot};

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

/// EXP-698 — which of the 8 contract avatar hues (`theme::tokens::avatar::
/// HUES`) a user without a picture wears: FNV-1a/32 over the USER ID's UTF-8
/// bytes, modulo the palette. Keyed on the id, never on the initials, so two
/// members sharing "DS" stay visually distinct — and identical on all four
/// clients (the web `avatarHueIndex`, iOS `avatarHueIndex`, Android
/// `avatarHueIndex`), which is why the constants and the fixture below are
/// the contract rather than an implementation detail.
pub fn avatar_hue_index(user_id: &str) -> usize {
    let mut hash: u32 = 0x811C_9DC5;
    for byte in user_id.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash as usize % theme::tokens::avatar::HUES.len()
}

/// #EXP-1754 (`packages/exponential-ui/fixtures/avatar.json` `keys`, web
/// `avatarHueKey`): what an avatar's hue is keyed on: the person's id, else
/// their name, else their email (trimmed; blank counts as absent). `None` =
/// no key: the muted disc. A widget reporter (no id) keys on their NAME, else
/// their EMAIL; shown copy ("Anonymous visitor") is never a key.
pub fn avatar_hue_key<'a>(
    id: Option<&'a str>,
    name: Option<&'a str>,
    email: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
        return Some(id);
    }
    [name, email]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|label| !label.is_empty())
}

/// The box an avatar occupies at each rung — gpui-component's own
/// `avatar::avatar_size` ladder, which is `pub(super)` upstream and therefore
/// unreachable; the image path (`Avatar`) still sizes itself through it, so
/// these two MUST agree or picture and initials would render at different
/// diameters in the same list.
fn avatar_box(size: Size) -> Pixels {
    match size {
        Size::Large => px(80.),
        Size::Medium => px(48.),
        Size::Small => px(24.),
        Size::XSmall => px(16.),
        Size::Size(size) => size,
    }
}

/// The initials' type size at each rung — upstream's `AvatarSized`, same
/// reason.
fn avatar_text(initials: Div, size: Size) -> Div {
    match size {
        Size::Large => initials
            .text_3xl()
            .font_weight(gpui::FontWeight::SEMIBOLD),
        Size::Medium => initials.text_sm(),
        Size::Small => initials.text_xs(),
        Size::XSmall => initials.text_size(gpui::rems(0.65)),
        Size::Size(size) => initials.text_size(size * 0.5),
    }
}

/// The CROSS-CLIENT initials rule (EXP-698 r4) — the mobile one, which is the
/// one the four clients agree on: Android `initialsFor`, iOS `memberInitials`.
/// An email is reduced to its local part first (`alex.smith@x.dev` is a person,
/// not a domain), the remainder splits on space / `.` / `_` / `-` / `+`, and
/// two or more parts contribute their first letters while a single part gives
/// up its first two characters. Uppercased; nothing at all is `?`.
///
/// Deliberately NOT upstream's `extract_text_initials` (which splits on spaces
/// only, so every `firstname.lastname@` address collapsed to the same two
/// letters) — that one still runs on the `Avatar` image path's own fallback,
/// which only shows if a fetched picture fails to decode.
pub(crate) fn initials(label: &str) -> String {
    let base = label.trim();
    let local = match base.split_once('@') {
        Some((local, _)) => local,
        None => base,
    };
    let parts: Vec<&str> = local
        .split(|c| matches!(c, ' ' | '.' | '_' | '-' | '+'))
        .filter(|part| !part.trim().is_empty())
        .collect();
    let picked: String = match parts.as_slice() {
        [first, second, ..] => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
        _ => local.chars().take(2).collect(),
    };
    if picked.is_empty() {
        return "?".to_string();
    }
    picked.to_uppercase()
}

/// The unified user avatar: the profile picture when its bytes have landed,
/// the hue-hashed initials otherwise (EXP-698 — [`avatar_hue_index`] over the
/// hue KEY, the fill at 20% of that hue, the initials at full, no stroke).
/// `hue_key` is [`avatar_hue_key`]'s answer (a member's id); blank = no key =
/// the muted disc (#EXP-1754, web `bg-muted text-muted-foreground`). `label`
/// is the display name with the email fallback — never empty.
///
/// The fallback is drawn HERE rather than by `Avatar`'s own: upstream derives
/// its colour from a hash of the INITIALS against the theme blue's hue wheel
/// and exposes no setter, so the only way onto the cross-client palette is to
/// paint the disc ourselves. `Avatar` still owns the `src` path.
pub fn user_avatar(
    hue_key: &str,
    label: &str,
    image_url: Option<&str>,
    size: gpui_component::Size,
    cx: &mut App,
) -> gpui::AnyElement {
    avatar_element(hue_key, label, cached_avatar_image(cx, image_url), size)
}

/// [`user_avatar`] for a caller that already holds the decoded bytes (the
/// rail's account button resolves them ONCE and renders the avatar twice, at
/// two sizes).
pub fn avatar_element(
    hue_key: &str,
    label: &str,
    image: Option<Arc<gpui::Image>>,
    size: gpui_component::Size,
) -> gpui::AnyElement {
    match image {
        Some(image) => Avatar::new()
            .name(gpui::SharedString::from(label.to_string()))
            .with_size(size)
            .src(image)
            .into_any_element(),
        None => initials_avatar(hue_key, label, size).into_any_element(),
    }
}

/// The fallback's (fill, initials) colours: the key's contract hue at 20% and
/// full, or the muted pair when there is no key (#EXP-1754).
fn fallback_colors(hue_key: &str) -> (gpui::Hsla, gpui::Hsla) {
    match avatar_hue_key(Some(hue_key), None, None) {
        Some(key) => {
            let hue = theme::tokens::avatar::HUES[avatar_hue_index(key)].to_hsla();
            (hue.opacity(0.2), hue)
        }
        None => (
            theme::tokens::MUTED.to_hsla(),
            theme::tokens::MUTED_FOREGROUND.to_hsla(),
        ),
    }
}

/// The picture-less avatar: initials on the key's contract hue, or on the
/// muted disc without a key.
fn initials_avatar(hue_key: &str, label: &str, size: gpui_component::Size) -> Div {
    let (fill, ink) = fallback_colors(hue_key);
    div()
        .flex_shrink_0()
        .size(avatar_box(size))
        .rounded_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(fill)
        .text_color(ink)
        .child(avatar_text(div(), size).child(initials(label)))
}

/// #EXP-1383 (agent-settings.json `actor` / `assignee`): an AGENT where a
/// person would be (an actor, an assignee) reads as THE AGENT MARK at the
/// avatar's size: its glyph in its colour on the muted disc (the Chats agent
/// bubble's 24px disc with its 14px glyph, scaled), never initials (web
/// `UserAvatar agent`).
pub fn agent_avatar(
    icon: Option<&str>,
    color: Option<&str>,
    size: gpui_component::Size,
    cx: &App,
) -> gpui::AnyElement {
    use gpui_component::ActiveTheme as _;
    let side = avatar_box(size);
    div()
        .flex_shrink_0()
        .size(side)
        .rounded_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().secondary)
        .child(crate::icons::agent_icon(icon, color).with_size(agent_glyph_px(side)))
        .into_any_element()
}

/// The glyph inside [`agent_avatar`]'s disc per side (web
/// `USER_AVATAR_SIZES[*].glyph`: 16→10, 20→12, 24→14, 28→16, 32→18, 48→24).
fn agent_glyph_px(side: Pixels) -> Pixels {
    let side = f32::from(side);
    let glyph = if side >= 48. { 24. } else { (side / 2. + 2.).round() };
    px(glyph)
}

/// The team marks' pictures: ONE [`ImageCache`] for the active account, so a
/// relative `/api/attachments/{id}` resolves against its instance and carries
/// its bearer (the cache's transport sends that bearer to the instance only).
/// Another account gets a new cache.
struct TeamImagesGlobal {
    account: Option<String>,
    images: Entity<ImageCache>,
}
impl Global for TeamImagesGlobal {}

fn team_images(cx: &mut App) -> Entity<ImageCache> {
    let account = sync::Store::try_global(cx)
        .and_then(|store| store.session(cx).account_id().map(str::to_string));
    if let Some(global) = cx.try_global::<TeamImagesGlobal>() {
        if global.account == account {
            return global.images.clone();
        }
    }
    let images = cx.new(|_| ImageCache::new(None));
    cx.set_global(TeamImagesGlobal { account, images: images.clone() });
    images
}

/// The no-image mark: the group glyph at the side, in the surrounding text
/// colour, no chip and no fill.
pub(crate) fn team_glyph(size: f32) -> gpui::AnyElement {
    Icon::new(registry::UI_TEAM).with_size(px(size)).into_any_element()
}

/// #EXP-1645 THE TEAM mark (fixtures/team-image.json, web `team-avatar.tsx`,
/// iOS `TeamAvatar.swift`, Android `Avatars.kt`): the team's image in a square
/// of `size`, cover, rounded at `mark.radiusFraction`, no border and no fill,
/// so a transparent image stays transparent; no image, loading or failed =
/// [`team_glyph`]. `size` is the side in px: 28 in the rail header, 20 in its
/// menu.
pub(crate) fn team_avatar(icon_url: Option<&str>, size: f32, cx: &mut App) -> gpui::AnyElement {
    let image = team_image(icon_url, cx);
    team_avatar_of(image, size)
}

/// #EXP-1725: a team's loaded image, if it has one (the cache fetches it).
pub(crate) fn team_image(icon_url: Option<&str>, cx: &mut App) -> Option<crate::markdown::CachedImage> {
    match team_mark(icon_url) {
        TeamMark::Image { src } => match team_images(cx).update(cx, |images, cx| images.slot(&src, cx)) {
            MarkdownImageSlot::Ready(image) => Some(image),
            _ => None,
        },
        TeamMark::Glyph => None,
    }
}

/// [`team_avatar`] over an image already resolved ([`team_image`]).
pub(crate) fn team_avatar_of(image: Option<crate::markdown::CachedImage>, size: f32) -> gpui::AnyElement {
    let frame = div()
        .flex_shrink_0()
        .size(px(size))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center();
    match image {
        Some(image) => frame
            .child(
                img(image)
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover)
                    .rounded(px(size * TEAM_IMAGE.mark.radius_fraction))
                    .with_loading(move || team_glyph(size))
                    .with_fallback(move || team_glyph(size)),
            )
            .into_any_element(),
        None => frame.child(team_glyph(size)).into_any_element(),
    }
}

/// One avatar + name row (EXP-426): the ONE shape every user-listing surface
/// shares — the assignee picker menus (via `pickers::user_menu_item`) and
/// the `@` autocomplete rows.
pub(crate) fn user_row(
    user_id: &str,
    label: &str,
    image_url: Option<&str>,
    cx: &mut App,
) -> gpui::Div {
    gpui_component::h_flex()
        .gap_2()
        .items_center()
        .child(user_avatar(
            user_id,
            label,
            image_url,
            gpui_component::Size::XSmall,
            cx,
        ))
        .child(gpui::SharedString::from(label.to_string()))
}

/// Chrome surfaces (rail button — web-sidebar parity, EXP-311) show only the
/// first name; the full name + email live in account settings.
pub fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::{avatar_hue_index, avatar_hue_key, fallback_colors, first_name, initials};

    /// #EXP-1754: the hue KEY rule replays the shared fixture's `keys` cases
    /// (id, else name, else email, trimmed; nothing = the muted disc), each
    /// landing on its pinned hue.
    #[test]
    fn avatar_hue_key_replays_the_fixture_keys() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/exponential-ui/fixtures/avatar.json"
        ))
        .unwrap();
        let cases = fixture["keys"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let label = case["case"].as_str().unwrap();
            let field = |name: &str| case.get(name).and_then(|value| value.as_str());
            let key = avatar_hue_key(field("id"), field("name"), field("email"));
            assert_eq!(key, case["key"].as_str(), "{label}");
            assert_eq!(key.map(avatar_hue_index), case["hue"].as_u64().map(|hue| hue as usize), "{label}");
            // The painter: a blank key is the muted disc, never palette slot 5.
            let (fill, ink) = fallback_colors(key.unwrap_or_default());
            match key {
                Some(key) => {
                    let hue = theme::tokens::avatar::HUES[avatar_hue_index(key)].to_hsla();
                    assert_eq!((fill, ink), (hue.opacity(0.2), hue), "{label}");
                }
                None => assert_eq!(
                    (fill, ink),
                    (theme::tokens::MUTED.to_hsla(), theme::tokens::MUTED_FOREGROUND.to_hsla()),
                    "{label}"
                ),
            }
        }
    }

    /// EXP-698: the hue hash is a CROSS-CLIENT contract — the same person
    /// wears the same colour on web, iOS, Android and here. These vectors are
    /// the shared fixture; a change to the algorithm (or to the palette's
    /// length) recolours every avatar on every client at once, so it has to
    /// fail here first.
    #[test]
    fn avatar_hue_index_matches_the_cross_client_fixture() {
        assert_eq!(avatar_hue_index(""), 5);
        assert_eq!(avatar_hue_index("demo-mira"), 2);
        assert_eq!(avatar_hue_index("demo-jonas"), 4);
        assert_eq!(avatar_hue_index("demo-sofia"), 1);
        assert_eq!(avatar_hue_index("alex"), 5);
        assert_eq!(avatar_hue_index("7c9e6679-7425-40de-944b-e07fc1f90ae7"), 3);
        assert_eq!(avatar_hue_index("user_01HZY"), 1);
        assert_eq!(avatar_hue_index("ünïcödé"), 2);
    }

    /// The modulo above is the palette's length, not a hard-coded 8 — but the
    /// fixture only holds while the palette IS 8 hues long.
    #[test]
    fn avatar_palette_is_eight_hues() {
        assert_eq!(theme::tokens::avatar::HUES.len(), 8);
    }

    /// Drawing the fallback ourselves means owning the initials too — and they
    /// have to be the MOBILE rule (Android `initialsFor` / iOS
    /// `memberInitials`), not upstream's space-only split: an email is reduced
    /// to its local part and separators count, so `alex.smith@x.dev` reads
    /// "AS" here exactly as it does on the phones.
    #[test]
    fn initials_follow_the_cross_client_rule() {
        assert_eq!(initials("Mira Chen"), "MC");
        assert_eq!(initials("Foo Bar Dar"), "FB");
        assert_eq!(initials("huacnlee"), "HU");
        assert_eq!(initials("alex.smith@x.dev"), "AS");
        assert_eq!(initials("jonas_weber@x.dev"), "JW");
        assert_eq!(initials("sofia-ruiz+tag@x.dev"), "SR");
        // A single-part local part gives up its first two characters.
        assert_eq!(initials("alex@example.com"), "AL");
        // One character is all there is; nothing at all is the "?" sentinel.
        assert_eq!(initials("a@example.com"), "A");
        assert_eq!(initials(""), "?");
        assert_eq!(initials("   "), "?");
    }

    #[test]
    fn first_name_takes_first_word() {
        assert_eq!(first_name("Danny Strähhuber"), "Danny");
        assert_eq!(first_name("  Ada   Lovelace  "), "Ada");
        assert_eq!(first_name("mononym"), "mononym");
        assert_eq!(first_name("user@example.com"), "user@example.com");
        assert_eq!(first_name(""), "");
    }
}
