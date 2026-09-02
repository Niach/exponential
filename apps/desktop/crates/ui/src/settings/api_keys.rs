//! Settings → API keys (EXP-238): the account's personal `expu_` keys.
//!
//! Web parity: the settings `api-keys` section. List/mint/revoke ride the
//! same tRPC surface (`users.listPersonalApiKeys` / `mintPersonalApiKey` /
//! `revokePersonalApiKey`); the raw key is shown EXACTLY ONCE after a mint
//! (the server stores only a hash), so the reveal block lives until the
//! user leaves the pane — a re-entry never shows it again ([`Self::mark_stale`]
//! clears it).
//!
//! Two kinds of rows share the list: keys the user minted here (or in the
//! web pane), and the hidden per-device keys the launcher/CLI mint as
//! `Device: <hostname>` (§7.2). THIS device's own row gets a badge; revoking
//! it also deletes the local token-store copy, otherwise
//! `ensure_personal_key` would keep handing the dead key to coding sessions
//! until a confusing 401.

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, ClipboardItem, Entity, FontWeight,
    IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, WindowExt as _,
};

use api::token_store::{SecretKind, TokenStore};
use api::users::{MintedPersonalKey, PersonalKeyMeta, PERSONAL_KEY_READ_TIMEOUT};

use crate::controls::WebControl as _;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::queries;
use crate::session::AuthContext;

use super::storage::format_created_date;
use super::{card_header, error_notice, section};

/// The `Device: ` name prefix `api::users::device_key_name` mints with —
/// rows carrying it belong to a signed-in desktop/CLI, not a script.
const DEVICE_KEY_PREFIX: &str = "Device: ";

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

    /// The "New key" confirm: a name input rides the alert as extra content;
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
            "New API key",
            "The key acts as you across the API, MCP and CLI, with your full \
             team membership. You'll see the full key exactly once.",
            "Create key",
        )
        .height(gpui::px(300.))
        .content(move |_, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Name"),
                )
                .child(Input::new(&content_input).web_input_sm())
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

    /// Per-row revoke confirm + `users.revokePersonalApiKey` → refetch. When
    /// the revoked row is THIS device's own key, the local token-store copy
    /// goes with it so the launcher re-mints instead of 401ing.
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
        let label = display_name(row);
        let device_row = row
            .name
            .as_deref()
            .is_some_and(|name| name.starts_with(DEVICE_KEY_PREFIX));
        let description = if device_row {
            "This key was minted automatically for a signed-in device. \
             Revoking it signs that device's coding-agent and MCP wiring out \
             until it mints a fresh key (usually at its next coding session \
             or sign-in). Anything else using the key stops working \
             immediately."
        } else {
            "Scripts, MCP clients and CLI logins using this key stop working \
             immediately. This cannot be undone."
        };
        let spec = AlertSpec::new(format!("Revoke \"{label}\"?"), description, "Revoke key")
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
                    .child("Copy your new key now"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("This is the only time the full key is shown."),
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
                        Button::new("api-key-copy")
                            .outline().cursor_pointer()
                            .web_xs()
                            .label("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(key.clone()));
                            }),
                    )
                    .child(
                        Button::new("api-key-dismiss")
                            .ghost().cursor_pointer()
                            .web_xs()
                            .label("Dismiss")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.minted = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// One key row: name (+ "This device" badge), key prefix, created,
    /// last used, revoke.
    fn render_row(
        &self,
        row: &PersonalKeyMeta,
        this_device: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let row_for_revoke = row.clone();
        let start: SharedString = row
            .start
            .clone()
            .map(|start| format!("{start}…").into())
            .unwrap_or_else(|| "expu_…".into());
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

        // EXP-698: one row of an inset-grouped stack — the caller fuses them
        // through `glass_group_rows`, which draws the hairlines.
        crate::surface::glass_row_shell()
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
                            .child(SharedString::from(display_name(row))),
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
            .child(
                div()
                    .w_24()
                    .flex_shrink_0()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_xs()
                    .text_color(muted)
                    .child(start),
            )
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
                Button::new(SharedString::from(format!("api-key-revoke-{}", row.id)))
                    .outline().cursor_pointer()
                    .web_xs()
                    .label("Revoke")
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_revoke(&row_for_revoke, this_device, window, cx);
                    })),
            )
    }
}

impl Render for ApiKeysPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        let mut body = section(cx).child(card_header(
            "API keys",
            "Personal keys authenticate MCP clients, scripts and CLI logins \
             as you. Keys named \"Device: …\" were minted automatically for a \
             signed-in device.",
            cx,
        ));

        if let Some(minted) = self.minted.clone() {
            body = body.child(self.render_minted(&minted, cx));
        }

        let new_key = Button::new("api-key-new")
            .outline().cursor_pointer()
            .web_sm()
            .label("New key")
            .disabled(self.busy || !matches!(self.load, Load::Ready(_)))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_mint_dialog(window, cx);
            }));
        let refresh = Button::new("api-keys-refresh")
            .ghost().cursor_pointer()
            .web_sm()
            .label("Refresh")
            .loading(matches!(self.load, Load::Loading))
            .on_click(cx.listener(|this, _, _, cx| this.refetch(cx)));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap_2()
                        .child(refresh)
                        .child(new_key),
                );
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_full())
                        .child(Skeleton::new().h_4().w_full())
                        .child(Skeleton::new().h_4().w_64()),
                );
            }
            Load::Ready(Loaded {
                list: Err(message), ..
            }) => {
                body = body.child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap_2()
                        .child(refresh)
                        .child(new_key),
                );
                body = body.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Loaded {
                list: Ok(rows),
                device_key_id,
            }) => {
                let plural = if rows.len() == 1 { "" } else { "s" };
                body = body.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(format!(
                                    "{} key{plural}",
                                    rows.len()
                                ))),
                        )
                        .child(refresh)
                        .child(new_key),
                );

                if rows.is_empty() {
                    body = body.child(
                        div()
                            .px_3()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(super::row_stroke(cx))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No API keys yet."),
                    );
                } else {
                    let list: Vec<gpui::Div> = rows
                        .iter()
                        .map(|row| {
                            let this_device = device_key_id.as_deref() == Some(row.id.as_str());
                            self.render_row(row, this_device, cx)
                        })
                        .collect();
                    body = body.child(crate::surface::glass_group_rows(list));
                }
            }
        }

        v_flex().child(body)
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
        // revoke copy keys off this exact prefix.
        assert!("Device: build-box".starts_with(DEVICE_KEY_PREFIX));
    }
}
