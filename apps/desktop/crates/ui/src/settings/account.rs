//! Settings → Account: profile identity + timezone (masterplan v5 §8.9, L25).
//!
//! Web parity: the settings `account` section (EXP-238 folded the old
//! Account screen into the settings nav; the email prefs are their own
//! Notifications section now — see [`super::notifications_prefs`]). The old
//! Integrations pane is gone (L25): GitHub App install/manage lives solely in
//! **team settings → Repositories**, and there is no calendar UI anywhere on
//! the desktop. EXP-311: the rail's account button shows only the avatar +
//! first name, so the full name + email live HERE. EXP-369 adds the account's
//! timezone row — the clock the daily digest's send hour is read in.
//! EXP-1126 adds the Sign-in methods + Passkeys bands under the timezone
//! picker row ([`super::sign_in_methods`]).

use gpui::{
    div, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    v_flex, ActiveTheme as _, Disableable as _,
};

use super::sign_in_methods::SignInMethodsSection;
use super::spawn_trpc;
use crate::navigation::Navigation;
use crate::queries;

/// The account's stored IANA timezone (`users.timezone` — a server-only
/// column, so it comes over tRPC, not the users shape).
enum Timezone {
    Idle,
    Loading,
    /// `None` = never captured; the digest sweep reads UTC for those.
    Ready(Option<String>),
    Error,
}

/// The Account section pane — identity + timezone.
pub struct AccountPane {
    timezone: Timezone,
    timezone_generation: u64,
    /// The account the timezone belongs to — a re-login must not show the
    /// previous account's value (same guard as the prefs pane).
    account_id: Option<String>,
    /// EXP-1126: Sign-in methods + Passkeys (server-only, tRPC).
    sign_in_methods: Entity<SignInMethodsSection>,
    _subscriptions: Vec<Subscription>,
}

impl AccountPane {
    pub fn new(nav: Entity<Navigation>, _window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let users = sync::Store::global(cx).collections().users.clone();
        let avatar_cache = crate::user_avatar::AvatarCache::global(cx);
        let subscriptions = vec![
            // The identity header rides the users shape (profile image URL)
            // plus the async avatar-byte cache (EXP-311).
            cx.observe(&users, |_, _, cx| cx.notify()),
            cx.observe(&avatar_cache, |_, _, cx| cx.notify()),
        ];
        let sign_in_methods = cx.new(|cx| SignInMethodsSection::new(nav, cx));
        Self {
            timezone: Timezone::Idle,
            timezone_generation: 0,
            account_id: None,
            sign_in_methods,
            _subscriptions: subscriptions,
        }
    }

    /// EXP-369: this pane is built once per window and outlives every visit,
    /// so its server-only read is dropped whenever the pane is (re-)entered —
    /// see [`super::SettingsView::mark_personal_stale`].
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.timezone, Timezone::Ready(_) | Timezone::Error) {
            self.timezone = Timezone::Idle;
        }
        self.sign_in_methods
            .update(cx, |section, cx| section.mark_stale(cx));
        cx.notify();
    }

    fn ensure_timezone_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.timezone = Timezone::Idle;
        }
        if !matches!(self.timezone, Timezone::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.timezone = Timezone::Loading;
        self.timezone_generation += 1;
        let generation = self.timezone_generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::users::users_timezone(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.timezone_generation != generation {
                    return;
                }
                this.timezone = match result {
                    Ok(timezone) => Timezone::Ready(timezone),
                    Err(err) => {
                        log::warn!("[ui] users.timezone failed: {err}");
                        Timezone::Error
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// A picked zone, written explicitly (not `onlyIfUnset` — this is the
    /// user asking for it), with the shown value updated optimistically.
    fn set_timezone(&mut self, timezone: String, cx: &mut gpui::Context<Self>) {
        self.timezone = Timezone::Ready(Some(timezone.clone()));
        cx.notify();
        spawn_trpc(cx, "users.setTimezone", move |trpc| {
            api::users::users_set_timezone(trpc, &timezone, false)
        });
    }

    /// The clock the daily digest's send hour is read in — web parity: ONE
    /// picker row ("Timezone" · the zone · chevron-right) with a search
    /// field over the tz database, right under the identity card.
    fn render_timezone(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let stored: Option<String> = match &self.timezone {
            Timezone::Ready(Some(timezone)) => Some(timezone.clone()),
            // EXP-771: no stored zone IS UTC — the digest hour is read in it,
            // and the web's picker shows exactly that (`initialTimezone ??
            // "UTC"`), never a "not set" placeholder.
            Timezone::Ready(None) => Some("UTC".to_string()),
            _ => None,
        };
        let value: SharedString = match (&self.timezone, &stored) {
            (_, Some(zone)) => zone.clone().into(),
            (Timezone::Error, _) => "Unavailable".into(),
            _ => "Loading…".into(),
        };
        let trigger = Button::new("account-timezone-trigger")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(cx.theme().foreground.opacity(0.7))
            .disabled(stored.is_none())
            .child(crate::surface::picker_value_label(value))
            .child(crate::surface::picker_row_chevron(cx));
        let control = match stored {
            Some(current) => {
                let items: Vec<crate::picker::PickerItem<String>> =
                    timezone_options(Some(&current))
                        .into_iter()
                        .map(|zone| crate::picker::PickerItem::new(zone.clone(), zone))
                        .collect();
                let pane = cx.entity().downgrade();
                crate::picker::deferred(move |window, cx| {
                    crate::picker::Picker::single(
                        items,
                        Some(current),
                        trigger.into_any_element(),
                        std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut gpui::App| {
                            let Some(zone) = next.into_iter().next() else {
                                return;
                            };
                            let _ = pane.update(cx, |this, cx| this.set_timezone(zone, cx));
                        }),
                    )
                    .search(true)
                    .empty_text("No matching timezone.")
                    .id("account-timezone-picker")
                    .render(window, cx)
                })
                .into_any_element()
            }
            None => trigger.into_any_element(),
        };
        crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row(
            "Timezone",
            None,
            control,
            cx,
        )])
    }

    /// Avatar + full name + email — the one place the full identity shows
    /// (the rail's chrome is first-name-only, EXP-311).
    fn render_identity(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let account = crate::queries::active_account(cx);
        let name = account
            .as_ref()
            .and_then(|account| account.name.clone())
            .filter(|name| !name.trim().is_empty());
        let email = account.as_ref().map(|account| account.email.clone());
        let full_name: SharedString = name
            .clone()
            .or_else(|| email.clone())
            .map(SharedString::from)
            .unwrap_or_else(|| "Not signed in".into());
        // The email sub-line is dropped when it already IS the title
        // (name-less Apple sign-in accounts).
        let sub_email = name
            .is_some()
            .then(|| email.clone())
            .flatten()
            .map(SharedString::from);
        let image_url = crate::queries::active_user(cx).and_then(|user| user.image);
        // EXP-698: the hue key is the USER ID, so a picture-less account wears
        // the same colour here as in every member list.
        let user_id = account
            .as_ref()
            .map(|account| account.user_id.clone())
            .unwrap_or_default();

        // EXP-818: the Linear profile card — one grouped row: the picture
        // leading, the name over the address.
        crate::surface::glass_group_rows(vec![crate::surface::glass_row_shell()
            .child(crate::user_avatar::user_avatar(
                &user_id,
                &full_name,
                image_url.as_deref(),
                gpui_component::Size::Medium,
                cx,
            ))
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(full_name.clone()),
                    )
                    .children(sub_email.map(|email| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(email)
                    })),
            )])
    }
}

impl Render for AccountPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_timezone_loaded(cx);

        // An ordinary section pane since EXP-238 — `SettingsView` owns the
        // scroll container and the shared `detail_column` grid.
        v_flex()
            .gap_6()
            .child(self.render_identity(cx))
            // Web parity: the Timezone row sits right under the identity.
            .child(self.render_timezone(cx))
            .child(self.sign_in_methods.clone())
    }
}

/// The zones the picker offers (web `timezoneOptions`): the system tz
/// database's zones (`zone.tab`, read once), with UTC and the device's own
/// zone always present; a stored zone the list lacks leads it, so the
/// current value is always pickable. Without a tz database (Windows) the
/// list is the short fallback, like the web's.
fn timezone_options(current: Option<&str>) -> Vec<String> {
    static ZONES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let base = ZONES.get_or_init(|| {
        let tab = ["/usr/share/zoneinfo/zone1970.tab", "/usr/share/zoneinfo/zone.tab"]
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let mut zones = zones_from_tab(&tab);
        zones.push("UTC".to_string());
        if let Ok(local) = iana_time_zone::get_timezone() {
            zones.push(local);
        }
        zones.sort();
        zones.dedup();
        zones
    });
    let mut zones = base.clone();
    if let Some(current) = current.filter(|zone| !zone.is_empty()) {
        if !zones.iter().any(|zone| zone == current) {
            zones.insert(0, current.to_string());
        }
    }
    zones
}

/// The zone names of a `zone.tab` / `zone1970.tab` (third tab column;
/// `#` lines are comments).
fn zones_from_tab(tab: &str) -> Vec<String> {
    tab.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(2))
        .map(|zone| zone.trim().to_string())
        .filter(|zone| !zone.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_tab_reads_the_third_column() {
        let tab = "# comment\nAT\t+4813+01620\tEurope/Vienna\nUS\t+404251-0740023\tAmerica/New_York\tEastern\n";
        assert_eq!(zones_from_tab(tab), ["Europe/Vienna", "America/New_York"]);
    }

    #[test]
    fn a_stored_zone_is_always_offered() {
        let zones = timezone_options(Some("Mars/Olympus_Mons"));
        assert_eq!(zones[0], "Mars/Olympus_Mons");
        assert!(zones.iter().any(|zone| zone == "UTC"));
    }
}
