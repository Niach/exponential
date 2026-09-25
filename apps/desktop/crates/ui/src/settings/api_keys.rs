//! Settings → Security (EXP-238): the account's personal `expu_` keys.
//!
//! Web parity: the settings `api-keys` section. List/mint/revoke ride the
//! same tRPC surface (`users.listPersonalApiKeys` / `mintPersonalApiKey` /
//! `revokePersonalApiKey`); the raw key is shown EXACTLY ONCE after a mint
//! (the server stores only a hash), so the reveal block lives until the
//! user leaves the pane — a re-entry never shows it again ([`Self::mark_stale`]
//! clears it).
//!
//! EXP-1054: TWO lists. The hidden per-device keys the launcher/CLI mint as
//! `Device: <hostname>` (§7.2) are **Login sessions** — a signed-in device,
//! named by its hostname, never by its token, whose action is **Log out**.
//! Everything else is an **API key** for scripts and MCP clients. THIS
//! device's own session row gets a badge; logging it out also deletes the
//! local token-store copy, otherwise `ensure_personal_key` would keep
//! handing the dead key to coding sessions until a confusing 401.

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, ClipboardItem, Entity, FontWeight,
    IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use gpui_component::{
    button::ButtonVariant,
    h_flex,
    input::InputState,
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, WindowExt as _,
};

use api::token_store::{SecretKind, TokenStore};
use api::users::{MintedPersonalKey, PersonalKeyMeta, PERSONAL_KEY_READ_TIMEOUT};

use crate::controls::{glass_input, WebControl as _};
use crate::surface::{glass_pill_button, PillSize};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::queries;
use crate::session::AuthContext;

use super::storage::format_created_date;
use super::{error_notice, section};

/// The `Device: ` name prefix `api::users::device_key_name` mints with —
/// rows carrying it belong to a signed-in desktop/CLI, not a script.
const DEVICE_KEY_PREFIX: &str = "Device: ";

/// A login-session row (a signed-in device) vs an API key.
fn is_device_row(row: &PersonalKeyMeta) -> bool {
    row.name
        .as_deref()
        .is_some_and(|name| name.starts_with(DEVICE_KEY_PREFIX))
}

/// A login session's display name — the hostname, the mint prefix stripped.
fn device_name(row: &PersonalKeyMeta) -> String {
    row.name
        .as_deref()
        .and_then(|name| name.strip_prefix(DEVICE_KEY_PREFIX))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Device")
        .to_string()
}

struct Loaded {
    list: Result<Vec<PersonalKeyMeta>, String>,
    /// THIS device's own key row id (token store), for the badge + the
    /// local-secret cleanup on revoke. Best-effort — `None` just drops both.
    device_key_id: Option<String>,
}

enum Load {
    Idle,
    Loading,
    Ready(Loaded),
}

pub struct ApiKeysPane {
    load: Load,
    /// The account the loaded list belongs to — a re-login must re-fetch.
    account_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// A mint/revoke in flight — disables the mutation affordances.
    busy: bool,
    /// The last mint's raw key — the one-time reveal block. Cleared on
    /// [`Self::mark_stale`] (pane re-entry) and by the dismiss button.
    minted: Option<MintedPersonalKey>,
    name_input: Entity<InputState>,
}

impl ApiKeysPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Personal key"));
        Self {
            load: Load::Idle,
            account_id: None,
            generation: 0,
            busy: false,
            minted: None,
            name_input,
        }
    }

    /// Server read — refetch on every pane entry, and drop the one-time
    /// reveal with it (a revisit must never show the secret again).
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.minted = None;
        if matches!(self.load, Load::Ready(_)) {
            self.load = Load::Idle;
        }
        cx.notify();
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
            self.minted = None;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let store = local_store(cx);

        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let list = api::users::list_personal_api_keys(&trpc)
                        .map_err(|err| err.to_string());
                    let device_key_id = store.and_then(|(store, account_id)| {
                        store.get_bounded(
                            &account_id,
                            SecretKind::PersonalApiKeyId,
                            PERSONAL_KEY_READ_TIMEOUT,
                        )
                    });
                    Loaded {
                        list,
                        device_key_id,
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = Load::Ready(result);
                cx.notify();
            });
        })
        .detach();
    }

    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    /// The "Create key" confirm: a name input rides the alert as extra content;
    /// OK mints and surfaces the raw key in the pane's one-time reveal.
    fn open_mint_dialog(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.name_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let content_input = self.name_input.clone();
        let ok_input = self.name_input.clone();
        let spec = AlertSpec::new(
            "Create API key",
            "For scripts and MCP clients. The key acts as you with your full \
             team membership; revoke it here at any time.",
            "Create key",
        )
        .height(gpui::px(300.))
        .content(move |window, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Name"),
                )
                .child(glass_input(&content_input, window, cx).web_input_sm())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let typed = ok_input.read(cx).value().trim().to_string();
            let name = if typed.is_empty() {
                "Personal key".to_string()
            } else {
                typed
            };
            let _ = pane.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let pane = pane.clone();
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::users::mint_personal_api_key(&trpc, Some(&name))
                    })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    this.busy = false;
                    match result {
                        Ok(minted) => {
                            this.minted = Some(minted);
                            this.refetch(cx);
                        }
                        Err(err) => {
                            log::warn!("[ui] users.mintPersonalApiKey failed: {err}");
                            let note = Notification::error(SharedString::from(format!(
                                "Could not create the key: {err}"
                            )));
                            let _ = handle.update(cx, |_, window, cx| {
                                window.push_notification(note, cx);
                            });
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// Per-row revoke confirm + `users.revokePersonalApiKey` → refetch. A
    /// login-session row reads as "Log out device" (the same mutation: the
    /// device's session IS its hidden key). When the row is THIS device's
    /// own, the local token-store copy goes with it so the launcher re-mints
    /// instead of 401ing.
    fn confirm_revoke(
        &mut self,
        row: &PersonalKeyMeta,
        this_device: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let key_id = row.id.clone();
        let device_row = is_device_row(row);
        let label = if device_row {
            device_name(row)
        } else {
            display_name(row)
        };
        let (title, description, ok_label) = if device_row {
            (
                "Log out device",
                "The desktop app or CLI on this device signs out. Its coding \
                 sessions and MCP wiring stop until it signs in again.",
                "Log out",
            )
        } else {
            (
                "Revoke API key",
                "Anything still using this key stops working immediately. This \
                 cannot be undone.",
                "Revoke key",
            )
        };
        // EXP-771: the title names the ACTION (web parity), so the row it hits
        // rides the body — its name and, for a key, the visible prefix. A
        // login session is never named by its token.
        let identity_name: SharedString = label.clone().into();
        let identity_preview: Option<SharedString> = (!device_row).then(|| {
            row.start
                .clone()
                .map(|start| format!("{start}\u{2026}").into())
                .unwrap_or_else(|| "expu_\u{2026}".into())
        });
        let spec = AlertSpec::new(title, description, ok_label)
            .height(gpui::px(260.))
            .content(move |_, cx| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(identity_name.clone()),
                    )
                    .children(identity_preview.clone().map(|preview| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(preview)
                    }))
                    .into_any_element()
            })
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, cx| {
                let Some(trpc) = queries::trpc_client(cx) else {
                    return true;
                };
                let _ = pane.update(cx, |this, cx| {
                    this.busy = true;
                    cx.notify();
                });
                let pane = pane.clone();
                let key_id = key_id.clone();
                let label = label.clone();
                let store = this_device.then(|| local_store(cx)).flatten();
                cx.spawn(async move |cx| {
                    let revoke_id = key_id.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let result =
                                api::users::revoke_personal_api_key(&trpc, &revoke_id);
                            if result.is_ok() {
                                // THIS device's key: drop the local copy too,
                                // or ensure_personal_key keeps returning the
                                // revoked secret.
                                if let Some((store, account_id)) = store {
                                    store.delete(&account_id, SecretKind::PersonalApiKey);
                                    store.delete(&account_id, SecretKind::PersonalApiKeyId);
                                }
                            }
                            result
                        })
                        .await;
                    let _ = pane.update(cx, |this, cx| {
                        this.busy = false;
                        if result.is_ok() {
                            this.refetch(cx);
                        }
                        cx.notify();
                    });
                    if let Err(err) = result {
                        log::warn!("[ui] users.revokePersonalApiKey({key_id}) failed: {err}");
                        let note = Notification::error(SharedString::from(format!(
                            "Could not revoke {label}: {err}"
                        )));
                        let _ = handle.update(cx, |_, window, cx| {
                            window.push_notification(note, cx);
                        });
                    }
                })
                .detach();
                true
            });
        open_alert(window, cx, spec);
    }

    /// The one-time reveal block after a mint: raw key + copy + dismiss.
    fn render_minted(
        &self,
        minted: &MintedPersonalKey,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let key = minted.key.clone();
        v_flex()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().primary.opacity(0.4))
            .bg(cx.theme().primary.opacity(0.05))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Copy your API key"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "This is the only time the full key is shown. Store it \
                         somewhere safe \u{2014} only a hash stays on the server.",
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py_1()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(super::row_stroke(cx))
                            .font_family(theme::terminal::FONT_FAMILY)
                            .text_xs()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(key.clone())),
                    )
                    .child(
                        glass_pill_button("api-key-copy", PillSize::Sm, cx)
                            .label("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(key.clone()));
                            }),
                    )
                    .child(
                        glass_pill_button("api-key-dismiss", PillSize::Sm, cx)
                            .label("Done")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.minted = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// One row. An API key: name, key prefix, created, last used, Revoke. A
    /// login session (EXP-1054): hostname (+ "This device" badge), signed
    /// in, last active, Log out — never its token.
    fn render_row(
        &self,
        row: &PersonalKeyMeta,
        this_device: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let row_for_revoke = row.clone();
        let device_row = is_device_row(row);
        let name = if device_row {
            device_name(row)
        } else {
            display_name(row)
        };
        let start: Option<SharedString> = (!device_row).then(|| {
            row.start
                .clone()
                .map(|start| format!("{start}…").into())
                .unwrap_or_else(|| "expu_…".into())
        });
        let created = row
            .created_at
            .as_deref()
            .map(format_created_date)
            .unwrap_or_else(|| "—".to_string());
        let last_used = row
            .last_request
            .as_deref()
            .map(format_created_date)
            .unwrap_or_else(|| "Never".to_string());

        // EXP-862: a FLAT list row (web `ListRow`) — no card, no hairline
        // group; the rows stack straight under the section band.
        crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .text_sm()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(name)),
                    )
                    .when(this_device, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .px_1p5()
                                .py_0p5()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(super::row_stroke(cx))
                                .text_xs()
                                .text_color(muted)
                                .child("This device"),
                        )
                    }),
            )
            .children(start.map(|start| {
                div()
                    .w_24()
                    .flex_shrink_0()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_xs()
                    .text_color(muted)
                    .child(start)
            }))
            .child(
                div()
                    .w_24()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(created)),
            )
            .child(
                div()
                    .w_24()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(last_used)),
            )
            .child(
                glass_pill_button(
                    SharedString::from(format!("api-key-revoke-{}", row.id)),
                    PillSize::Sm,
                    cx,
                )
                    .label(if device_row { "Log out" } else { "Revoke" })
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_revoke(&row_for_revoke, this_device, window, cx);
                    })),
            )
    }
}

impl ApiKeysPane {
    /// EXP-994: one hairline-divided ladder — a hairline between rows,
    /// nothing around them. `device_key_id` marks THIS device's own row.
    fn render_ladder(
        &self,
        rows: Vec<&PersonalKeyMeta>,
        device_key_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let list = rows.into_iter().enumerate().map(|(index, row)| {
            let this_device = device_key_id == Some(row.id.as_str());
            crate::surface::list_row(self.render_row(row, this_device, cx), index)
        });
        v_flex().w_full().min_w_0().children(list)
    }
}

impl Render for ApiKeysPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        // EXP-720: the header pair are Sm pills like every other pane's
        // header action (worktrees' Refresh, the machines band's Add device).
        // EXP-771: they ride the header's TRAILING slot (web
        // `GlassSectionHeader` + `Pill mode="action"`) instead of a row of
        // their own under it, and the "N keys" line above the list is gone —
        // no header counts anywhere since EXP-698.
        let new_key = glass_pill_button("api-key-new", PillSize::Sm, cx)
            .label("Create key")
            .disabled(self.busy || !matches!(self.load, Load::Ready(_)))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_mint_dialog(window, cx);
            }));
        // EXP-862: no Refresh pill (web parity); the list re-reads after
        // every mint and revoke on its own.
        let header_actions = h_flex()
            .items_center()
            .gap_2()
            .child(new_key)
            .into_any_element();

        // EXP-1054: two lists on one page — Login sessions (the signed-in
        // devices) over API keys (for scripts and MCP clients). No caption
        // under either band.
        let mut sessions = section(cx).child(crate::surface::glass_section_header(
            "Login sessions",
            None,
            cx,
        ));
        let mut keys = section(cx).child(crate::surface::glass_section_header(
            "API keys",
            Some(header_actions),
            cx,
        ));

        if let Some(minted) = self.minted.clone() {
            keys = keys.child(self.render_minted(&minted, cx));
        }

        let skeleton = || {
            v_flex()
                .gap_2()
                .child(crate::controls::skeleton().h_4().w_full())
                .child(crate::controls::skeleton().h_4().w_64())
        };
        let empty = |text: &'static str, cx: &App| {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(text)
        };

        match &self.load {
            Load::Idle | Load::Loading => {
                sessions = sessions.child(skeleton());
                keys = keys.child(skeleton());
            }
            Load::Ready(Loaded {
                list: Err(message), ..
            }) => {
                sessions = sessions.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Loaded {
                list: Ok(rows),
                device_key_id,
            }) => {
                let (device_rows, key_rows): (Vec<_>, Vec<_>) =
                    rows.iter().partition(|row| is_device_row(row));
                let sessions_list = (!device_rows.is_empty())
                    .then(|| self.render_ladder(device_rows, device_key_id.as_deref(), cx));
                let keys_list = (!key_rows.is_empty())
                    .then(|| self.render_ladder(key_rows, device_key_id.as_deref(), cx));
                sessions = match sessions_list {
                    Some(list) => sessions.child(list),
                    None => sessions.child(empty("No signed-in devices.", cx)),
                };
                keys = match keys_list {
                    Some(list) => keys.child(list),
                    None => keys.child(empty("No API keys.", cx)),
                };
            }
        }

        v_flex().gap_6().child(sessions).child(keys)
    }
}

/// The row's display name — unnamed keys read generically instead of blank.
fn display_name(row: &PersonalKeyMeta) -> String {
    row.name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "API key".to_string())
}

/// The active account's token store + id, for the "This device" badge and
/// the local-secret cleanup. `None` when signed out (the pane is unreachable
/// then anyway).
fn local_store(cx: &App) -> Option<(TokenStore, String)> {
    let data_dir = cx.try_global::<AuthContext>()?.data_dir.clone();
    let account = queries::active_account(cx)?;
    Some((TokenStore::new(data_dir), account.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: Option<&str>) -> PersonalKeyMeta {
        PersonalKeyMeta {
            id: "k-1".to_string(),
            name: name.map(str::to_string),
            start: Some("expu_ab".to_string()),
            prefix: Some("expu_".to_string()),
            created_at: None,
            last_request: None,
        }
    }

    #[test]
    fn unnamed_keys_read_generically() {
        assert_eq!(display_name(&meta(None)), "API key");
        assert_eq!(display_name(&meta(Some("  "))), "API key");
        assert_eq!(display_name(&meta(Some("CI deploys"))), "CI deploys");
    }

    #[test]
    fn device_prefix_matches_the_miner_name_shape() {
        // `api::users::device_key_name` mints `Device: <hostname>` — the
        // Login sessions split keys off this exact prefix.
        assert!("Device: build-box".starts_with(DEVICE_KEY_PREFIX));
        assert!(is_device_row(&meta(Some("Device: build-box"))));
        assert!(!is_device_row(&meta(Some("CI deploys"))));
        assert!(!is_device_row(&meta(None)));
    }

    #[test]
    fn a_login_session_is_named_by_its_host_never_its_token() {
        assert_eq!(device_name(&meta(Some("Device: build-box"))), "build-box");
        assert_eq!(device_name(&meta(Some("Device:   "))), "Device");
    }
}
