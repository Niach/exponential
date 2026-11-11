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
//! named by its hostname, never by its token, whose action is **Disconnect**:
//! the key is ONLY the coding runs' MCP credential
//! (`api::token_store::SecretKind::PersonalApiKey`), never the device's
//! sign-in, so revoking it cuts the runs' wiring while the device stays
//! signed in. Everything else is an **API key** for scripts and MCP clients.
//! FEED-76: the Create-key dialog scopes a key to teams/boards with the
//! same picker the web shows (`ScopePicker`; Everything = a full key;
//! a scoped key is MCP-only and wears its pick as the row caption).
//! THIS device's own session row gets a badge; disconnecting it also deletes
//! the local token-store copy, otherwise `ensure_personal_key` would keep
//! handing the dead key to coding sessions until a confusing 401.

use std::rc::Rc;

use gpui::{div, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use gpui_component::{button::ButtonVariant, ActiveTheme as _};
use serde_json::{json, Value};

use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use api::token_store::{SecretKind, TokenStore};
use api::users::{MintedPersonalKey, PersonalKeyMeta, PERSONAL_KEY_READ_TIMEOUT};

use crate::scope_picker::{copy, effective_scope_selection, toggle, ScopeSelection, ScopeTeam};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::queries;
use crate::session::AuthContext;

use super::doc::{and, failed_state, hex, mount, not, path, skeleton, IdeToasts, Ink, SM, XS};
use super::storage::format_created_date;

/// The `Device: ` name prefix `api::users::device_key_name` mints with —
/// rows carrying it belong to a signed-in desktop/CLI, not a script.
const DEVICE_KEY_PREFIX: &str = "Device: ";

/// FEED-76: under the picker while a scope is being picked (web copy).
const SCOPED_KEY_NOTE: &str = "A scoped key works with the MCP endpoint only and cannot be \
                               re-scoped later \u{2014} create a new key to widen it.";

/// FEED-76: the refusal for a pick that names nothing (the server's
/// `EMPTY_SELECTION_MESSAGE`, with the dialog's own way out).
const EMPTY_SCOPE_MESSAGE: &str = "Select at least one team or board, or switch Everything on.";

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
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on entry or after a write fails quietly.
    retry_pressed: bool,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

impl ApiKeysPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The actions leave the binding for the pane: Create and Revoke
        // confirm in a native alert, Copy writes the clipboard.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["create", "revoke", "copy", "done", "retry"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let chrome = Chrome {
            radius: f32::from(theme.radius),
            stroke: hex(super::row_stroke(cx)),
            primary_stroke: premultiplied_hex(theme.primary.opacity(0.4)),
            primary_wash: premultiplied_hex(theme.primary.opacity(0.05)),
            danger: hex(theme.danger),
            button_fg: hex(theme.button_foreground),
            // The wash's 3% alpha has no exact byte: [`premultiplied_hex`];
            // the stroke is the token's own byte.
            button_fill: premultiplied_hex(theme.input_background()),
            button_stroke: hex(theme.input),
            button_fill_disabled: premultiplied_hex(theme.input_background().opacity(0.5)),
            button_stroke_disabled: hex(theme.input.opacity(0.5)),
            button_fg_disabled: hex(theme.muted_foreground.opacity(0.5)),
            terminal: "$type.family.mono".to_string(),
        };
        let view = mount("settings-api-keys", document(&Ink::of(cx), &chrome), &binding, window, cx);
        Self {
            load: Load::Idle,
            account_id: None,
            generation: 0,
            busy: false,
            minted: None,
            retry_pressed: false,
            binding,
            view,
        }
    }

    /// A document action (`create`, `revoke` on the key `key`, `copy`,
    /// `done`, `retry`).
    fn action(&mut self, name: &str, key: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "create" if !self.busy && matches!(self.load, Load::Ready(_)) => self.open_mint_dialog(window, cx),
            "revoke" if !self.busy => {
                let Load::Ready(Loaded { list: Ok(rows), device_key_id }) = &self.load else { return };
                let Some(row) = rows.iter().find(|row| row.id == key).cloned() else { return };
                let this_device = device_key_id.as_deref() == Some(row.id.as_str());
                self.confirm_revoke(&row, this_device, window, cx);
            }
            "copy" => {
                if let Some(minted) = &self.minted {
                    cx.write_to_clipboard(ClipboardItem::new_string(minted.key.clone()));
                }
            }
            "done" => {
                self.minted = None;
                cx.notify();
            }
            "retry" => self.retry_load(cx),
            _ => {}
        }
    }

    /// The pane's state as the document reads it.
    fn project(&self) -> Value {
        let mut out = json!({"loading": false, "failed": false, "ready": false, "busy": self.busy, "createDisabled": self.busy || !matches!(self.load, Load::Ready(_)),
            "minted": self.minted.is_some(), "key": self.minted.as_ref().map(|m| m.key.clone()).unwrap_or_default(),
            "sessions": [], "keys": [], "noSessions": false, "noKeys": false});
        match &self.load {
            Load::Idle | Load::Loading => out["loading"] = json!(true),
            Load::Ready(Loaded { list: Err(_), .. }) => out["failed"] = json!(true),
            Load::Ready(Loaded { list: Ok(rows), device_key_id }) => {
                out["ready"] = json!(true);
                let row = |row: &PersonalKeyMeta| {
                    let device_row = is_device_row(row);
                    json!({
                        "key": row.id,
                        "name": if device_row { device_name(row) } else { display_name(row) },
                        "thisDevice": device_key_id.as_deref() == Some(row.id.as_str()),
                        "subline": row_subline(row, device_row),
                        "hasScope": !device_row,
                        "scope": if device_row { String::new() } else { api::users::scope_caption(row.scope.as_ref()) },
                        "action": if device_row { "Disconnect" } else { "Revoke" },
                        "danger": !device_row,
                    })
                };
                let (device_rows, key_rows): (Vec<_>, Vec<_>) = rows.iter().partition(|row| is_device_row(row));
                out["noSessions"] = json!(device_rows.is_empty());
                out["noKeys"] = json!(key_rows.is_empty());
                out["sessions"] = Value::Array(device_rows.into_iter().map(row).collect());
                out["keys"] = Value::Array(key_rows.into_iter().map(row).collect());
            }
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    /// The pane in a pinned state (the headless store syncs none).
    #[cfg(test)]
    pub(crate) fn show(&mut self, load: ShotLoad, minted: Option<MintedPersonalKey>, cx: &mut gpui::Context<Self>) {
        self.account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        self.load = Load::Ready(match load {
            ShotLoad::Ready(rows, device_key_id) => Loaded { list: Ok(rows), device_key_id },
            ShotLoad::Failed => Loaded { list: Err("offline".into()), device_key_id: None },
        });
        self.minted = minted;
        cx.notify();
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
                        .map_err(|err| err.user_message());
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
                // FEED-79: a failed load = the empty lists + Retry; only a
                // Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.retry_pressed);
                if let Err(message) = &result.list {
                    crate::action_error::failed_load(
                        "your API keys",
                        attended,
                        Some(message.clone()),
                        cx,
                    );
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

    /// The failed list's Retry: the same load, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.refetch(cx);
    }

    /// The "Create key" confirm: the name field and the FEED-76 scope
    /// picker ride the alert as its body ([`MintKeyContent`]); OK mints
    /// (scoped when Everything is off) and surfaces the raw key in the
    /// pane's one-time reveal. An empty pick keeps the dialog open with the
    /// refusal under the picker.
    fn open_mint_dialog(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // The member's synced teams + live boards — the web dialog's
        // `mcpGrants.scopeTree`, read off the store instead of the wire.
        let tree = ScopeTeam::from_store(cx);
        let content = cx.new(|cx| MintKeyContent::new(tree, ScopeSelection::everything(), String::new(), window, cx));
        // Web's dialog opens on the Name field.
        super::doc::focus_input(content.read(cx).view.clone(), "name", window, cx);
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let dialog_content = content.clone();
        let spec = AlertSpec::new(
            "Create API key",
            "For scripts and MCP clients. The key acts as you within the \
             teams and boards you choose; revoke it here at any time.",
            "Create key",
        )
        .content(move |_, _| dialog_content.clone().into_any_element())
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            // FEED-76: "Everything" sends no scope — the key stays an
            // ordinary full key. An empty pick never mints (the web's
            // `hasScopeSelection` gate).
            let scope = content.read(cx).effective();
            if !scope.has_selection() {
                content.update(cx, |content, cx| {
                    content.set_error(Some(EMPTY_SCOPE_MESSAGE.into()), cx);
                });
                return false;
            }
            let scope = (!scope.all_teams).then_some(scope);
            let typed = content.read(cx).typed_name(cx).trim().to_string();
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
                        // A key the person mints for themselves — never the
                        // agent's kind (EXP-1140).
                        api::users::mint_personal_api_key(
                            &trpc,
                            Some(&name),
                            None,
                            scope.as_ref(),
                        )
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
                            let note = crate::action_error::failed_toast(
                                "create",
                                "the key",
                                crate::action_error::action_error_description(&err),
                            );
                            let _ = handle.update(cx, |_, window, cx| {
                                crate::toast::show(note, window, cx);
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
    /// login-session row reads as "Disconnect device" (the same mutation on
    /// the device's hidden key; its sign-in is a separate session token and
    /// survives). When the row is THIS device's own, the local token-store
    /// copy goes with it so the launcher re-mints instead of 401ing.
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
                "Disconnect device",
                "Disconnects this device's coding sessions and their MCP wiring. \
                 The device stays signed in and mints a new key the next time \
                 it signs in or regenerates.",
                "Disconnect",
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
        let preview = (!device_row).then(|| {
            row.start
                .clone()
                .map(|start| format!("{start}\u{2026}"))
                .unwrap_or_else(|| "expu_\u{2026}".to_string())
        });
        let identity = cx.new(|cx| RevokeIdentity::new(label.clone(), preview, window, cx));
        let spec = AlertSpec::new(title, description, ok_label)
            .content(move |_, _| identity.clone().into_any_element())
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
                        let note = crate::action_error::failed_toast(
                            "revoke",
                            &label,
                            crate::action_error::action_error_description(&err),
                        );
                        let _ = handle.update(cx, |_, window, cx| {
                            crate::toast::show(note, window, cx);
                        });
                    }
                })
                .detach();
                true
            });
        open_alert(window, cx, spec);
    }
}

/// The Create dialog's body (FEED-76; #EXP-1629: an eUI document): the name
/// field over web's `ScopePicker` (an Everything switch over the member's
/// teams, each a whole-team checkbox with its boards indented beneath; a
/// board under a ticked team reads checked and disabled). Its own entity so
/// a toggle re-renders just this block (the surrounding native alert never
/// re-renders). The RAW pick lives here; what is SENT is
/// [`effective_scope_selection`], so unticking a team restores the boards
/// ticked under it.
pub(crate) struct MintKeyContent {
    tree: Vec<ScopeTeam>,
    value: ScopeSelection,
    /// The refusal under the picker (an empty pick on Create); cleared by
    /// the next change.
    error: Option<SharedString>,
    /// The name field's bound text (only the bench writes one).
    name: String,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

impl MintKeyContent {
    fn new(tree: Vec<ScopeTeam>, value: ScopeSelection, name: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let me = cx.weak_entity();
        // The first layout already holds the pick (a Null first state would
        // animate the switch into it).
        let first = project(&tree, &value, &None, &name);
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, first, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["all", "team", "board"] {
                let me = me.clone();
                binding.on_action(name, move |_, event, cx| {
                    let id = event.context.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                    let me = me.clone();
                    cx.defer(move |cx| {
                        let _ = me.update(cx, |this, cx| this.toggle(name, &id, cx));
                    });
                    None
                });
            }
            binding
        });
        let view = mount("api-key-create", mint_document(&Ink::of(cx), &hex(cx.theme().danger)), &binding, window, cx);
        Self { tree, value, error: None, name, binding, view }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(tree: Vec<ScopeTeam>, value: ScopeSelection, name: &str, error: Option<&str>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut this = Self::new(tree, value, name.to_string(), window, cx);
        this.error = error.map(|error| SharedString::from(error.to_string()));
        this
    }

    /// The bench's press on a node, as a click makes it.
    #[cfg(test)]
    pub(crate) fn press(&mut self, id: &str, event: &str, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            let index = view.index_of(id).expect("the node");
            view.fire(index, event, None, cx);
        });
    }

    /// The bench's wait for a running transition (the headless clock never
    /// moves on its own).
    #[cfg(test)]
    pub(crate) fn settle(&mut self, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_secs(1), cx));
    }

    /// A press on the Everything switch, a team's or a board's line (web
    /// `toggle`; a board its team covers stays as it is).
    fn toggle(&mut self, kind: &str, id: &str, cx: &mut gpui::Context<Self>) {
        match kind {
            "all" => self.value.all_teams = !self.value.all_teams,
            "team" => {
                let on = !self.value.team_ids.iter().any(|team| team == id);
                toggle(&mut self.value.team_ids, id, on);
            }
            "board" => {
                if self.covered(id) {
                    return;
                }
                let on = !self.value.board_ids.iter().any(|board| board == id);
                toggle(&mut self.value.board_ids, id, on);
            }
            _ => return,
        }
        self.error = None;
        cx.notify();
    }

    /// Whether `board` sits under a ticked whole team.
    fn covered(&self, board: &str) -> bool {
        self.tree.iter().any(|team| self.value.team_ids.contains(&team.id) && team.boards.iter().any(|b| b.id == board))
    }

    /// The pick to SEND (see [`effective_scope_selection`]).
    fn effective(&self) -> ScopeSelection {
        effective_scope_selection(&self.tree, &self.value)
    }

    /// The name as typed (the field's live text, never a debounced echo).
    fn typed_name(&self, cx: &App) -> String {
        self.view.read(cx).field_text("name.field", cx).unwrap_or_default()
    }

    fn set_error(&mut self, error: Option<SharedString>, cx: &mut gpui::Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// The state as the document binds it.
    fn project(&self) -> Value {
        project(&self.tree, &self.value, &self.error, &self.name)
    }
}

/// [`MintKeyContent`]'s state as the document binds it.
fn project(tree: &[ScopeTeam], value: &ScopeSelection, error: &Option<SharedString>, name: &str) -> Value {
    let teams: Vec<Value> = tree
        .iter()
        .map(|team| {
            let whole = value.team_ids.contains(&team.id);
            let boards: Vec<Value> = team
                .boards
                .iter()
                .map(|board| {
                    json!({
                        "id": board.id,
                        "name": board.name,
                        "prefix": board.prefix,
                        "checked": whole || value.board_ids.contains(&board.id),
                        "covered": whole,
                        "cursor": if whole { "default" } else { "pointer" },
                    })
                })
                .collect();
            json!({"id": team.id, "name": team.name, "checked": whole, "noBoards": team.boards.is_empty(), "boards": boards})
        })
        .collect();
    json!({
        "name": name,
        "all": value.all_teams,
        "noTeams": tree.is_empty(),
        "teams": teams,
        "hasError": error.is_some(),
        "error": error.as_ref().map(|error| error.to_string()).unwrap_or_default(),
    })
}

impl Render for MintKeyContent {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "MintKeyContent").entered();
        let projection = self.project();
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// A row of a checkbox line: the box, then the words as the rest of the
/// press target (the IDE's `gap_2` as the words' own lead, so the line is
/// one target as web's `Label htmlFor` makes it).
fn check_line(ink: &Ink, id: &str, event: &str, disabled: Option<Value>, words: Vec<Value>, align: &str, gap: f32) -> Value {
    let mut check = json!({"id": format!("{id}_box"), "component": "Checkbox", "props": {"label": "", "name": id, "checked": path("checked")},
        "accessibility": {"label": path("name")},
        // Its empty label part grows: the box keeps its own 16 px, no gap.
        "style": {"flexGrow": 0, "flexShrink": 0, "gap": 0},
        "on": {"change": {"event": {"name": event, "context": {"id": path("id")}}}}});
    let mut words = json!({"id": format!("{id}_words"), "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("name")},
        "on": {"press": {"event": {"name": event, "context": {"id": path("id")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": align, "flexGrow": 1, "minWidth": 0, "gap": ink.r(gap), "paddingLeft": ink.r(0.5), "cursor": "pointer"},
        "children": words});
    if let Some(disabled) = disabled {
        check["props"]["disabled"] = disabled;
        words["style"]["cursor"] = path("cursor");
    }
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0}, "children": [check, words]})
}

/// The Create body's tree (`gap_4`, `mt_2`): the Name field (`gap_1`: the
/// `text_xs` muted label over the boxed `web_input_sm`), then Access over
/// the picker (`gap_3`): the Everything group row (`px_3 py_2p5`, the label
/// over its hint, the switch trailing); while Everything is off, the team
/// tree on a group under its 168 px scroll cap (`p_3`, `gap_3` between
/// teams; a team `gap_1p5` over its boards, indented `ml_6`, `gap_1`) and
/// the scoped note; the refusal last.
fn mint_document(ink: &Ink, danger: &str) -> Value {
    let label = |id: &str, text: &str| ink.text(id, json!(text), XS, &ink.muted);
    let name_input = json!({"id": "name", "component": "Input", "props": {"variant": "box", "size": "sm", "label": "Name", "name": "name", "placeholder": "Personal key", "value": path("/pane/name")},
        "style": {"width": "100%"}});
    let column = |id: &str, gap: f32, children: Vec<Value>| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(gap)}, "children": children});
    let name = column("name_block", 0.25, vec![label("name_label", "Name"), name_input]);

    let mut everything = ink.text("everything_label", json!(copy::EVERYTHING), SM, &ink.foreground);
    everything["style"]["fontWeight"] = json!(500);
    let words = json!({"id": "everything_words", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "minWidth": 0, "gap": ink.r(0.125)},
        "children": [everything, ink.text("everything_hint", json!(copy::EVERYTHING_HINT), XS, &ink.muted)]});
    let switch = json!({"id": "everything_switch", "component": "Switch", "props": {"label": "", "name": "allTeams", "checked": path("/pane/all")},
        "on": {"change": {"event": {"name": "all"}}}});
    let row = json!({"id": "everything_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "justifyContent": "space-between", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.625)},
        "children": [words, switch]});
    let everything = ink.group("everything", vec![row]);

    let mut team_name = ink.text("team_name", path("name"), SM, &ink.foreground);
    team_name["style"]["fontWeight"] = json!(500);
    let header = check_line(ink, "team", "team", None, vec![team_name, ink.text("team_whole", json!(copy::WHOLE_TEAM), XS, &ink.muted)], "center", 0.5);
    let board = check_line(ink, "board", "board", Some(path("covered")), vec![ink.text("board_name", path("name"), SM, &ink.foreground), ink.text("board_prefix", path("prefix"), XS, &ink.muted)], "baseline", 0.375);
    let mut no_boards = ink.text("no_boards", json!(copy::NO_BOARDS), XS, &ink.muted);
    no_boards["visible"] = path("noBoards");
    // Only one of the list and its empty line shows: an empty list would
    // still take the block's gap.
    let boards = json!({"id": "boards", "component": "List", "visible": not(path("noBoards")), "props": {"gap": "none"}, "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25)},
        "template": {"component": "board", "path": "boards", "key": "id"},
        "children": [board]});
    let boards = json!({"id": "boards_block", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "minWidth": 0, "marginLeft": ink.r(1.5), "gap": ink.r(0.25)}, "children": [boards, no_boards]});
    let team = json!({"id": "team_item", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.375)}, "children": [header, boards]});
    let mut no_teams = ink.text("no_teams", json!(copy::NO_TEAMS), SM, &ink.muted);
    no_teams["visible"] = path("/pane/noTeams");
    let teams = json!({"id": "teams", "component": "List", "visible": not(path("/pane/noTeams")), "props": {"gap": "none"}, "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.75)},
        "template": {"component": "team_item", "path": "/pane/teams", "key": "id"},
        "children": [team]});
    let list = json!({"id": "tree_list", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "gap": ink.r(0.75), "padding": ink.r(0.75)}, "children": [no_teams, teams]});
    let scroll = json!({"id": "tree_scroll", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "flexShrink": 0, "maxHeight": TREE_SCROLL_CAP, "overflowY": "auto"}, "children": [list]});
    let mut tree = ink.group("tree", vec![scroll]);
    tree["visible"] = not(path("/pane/all"));
    let mut note = ink.text("scoped_note", json!(SCOPED_KEY_NOTE), XS, &ink.muted);
    note["visible"] = not(path("/pane/all"));
    let mut error = ink.text("scope_error", path("/pane/error"), SM, danger);
    error["visible"] = path("/pane/hasError");
    let picker = column("picker", 0.75, vec![everything, tree, note, error]);
    let access = column("access_block", 0.25, vec![label("access_label", "Access"), picker]);
    let mut root = column("root", 1., vec![name, access]);
    // `mt_2`: the root's own margin never lays out, its padding does.
    root["style"]["paddingTop"] = json!(ink.r(0.5));
    root
}

/// The team tree's scroll cap in the dialog (the picker's own is web
/// `max-h-72`; the alert keeps it short).
const TREE_SCROLL_CAP: f32 = 168.;

/// The revoke / disconnect alert's body (EXP-771; #EXP-1629: an eUI
/// document): the row it hits, its name (`text_sm` medium) and, for an API
/// key, the visible prefix (`text_xs` muted mono). A login session is never
/// named by its token.
pub(crate) struct RevokeIdentity {
    view: Entity<SurfaceView>,
}

impl RevokeIdentity {
    fn new(name: String, preview: Option<String>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            binding
        });
        let state = json!({"name": name, "hasPreview": preview.is_some(), "preview": preview.unwrap_or_default()});
        binding.update(cx, |binding, cx| binding.update_state(|s| *s = state, cx));
        let view = mount("api-key-revoke", revoke_document(&Ink::of(cx)), &binding, window, cx);
        Self { view }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(name: &str, preview: Option<&str>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(name.to_string(), preview.map(str::to_string), window, cx)
    }
}

impl Render for RevokeIdentity {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "RevokeIdentity").entered();
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The revoke body's tree (`gap_2`, centred): the name, the key's prefix.
fn revoke_document(ink: &Ink) -> Value {
    let mut name = ink.text("identity_name", path("/pane/name"), SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    let mut preview = ink.text("identity_preview", path("/pane/preview"), XS, &ink.muted);
    preview["visible"] = path("/pane/hasPreview");
    preview["style"]["fontFamily"] = json!(ink.mono);
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5)}, "children": [name, preview]})
}

/// The row's subline (web `api-keys-section.tsx`, string for string): a
/// login session reads "Signed in Mar 4, 2026 · last active …", an API key
/// "expu_ab… · created … · last used …"; a missing date reads "never".
fn row_subline(row: &PersonalKeyMeta, device_row: bool) -> String {
    let date = |at: Option<&str>| at.map(format_created_date).unwrap_or_else(|| "–".to_string());
    let last = row
        .last_request
        .as_deref()
        .map(format_created_date)
        .unwrap_or_else(|| "never".to_string());
    if device_row {
        format!("Signed in {} · last active {last}", date(row.created_at.as_deref()))
    } else {
        // Web `keyPreview`: the visible start, else the static prefix.
        let preview = row
            .start
            .clone()
            .or_else(|| row.prefix.clone())
            .filter(|start| !start.is_empty())
            .map(|start| format!("{start}…"))
            .unwrap_or_else(|| "expu_…".to_string());
        format!("{preview} · created {} · last used {last}", date(row.created_at.as_deref()))
    }
}

/// The colours and radii the document needs beyond [`Ink`]: gpui-component's
/// `theme.radius`, the rows' hairline, the reveal block's primary wash, and
/// the outline `Button`'s fills (normal and disabled).
struct Chrome {
    radius: f32,
    stroke: String,
    primary_stroke: String,
    primary_wash: String,
    danger: String,
    button_fg: String,
    button_fill: String,
    button_stroke: String,
    button_fill_disabled: String,
    button_stroke_disabled: String,
    button_fg_disabled: String,
    terminal: String,
}

/// `#rrggbbaa` of a faint translucent colour that paints like the exact
/// one: the alpha rounds to its byte and the channels absorb the rounding,
/// so colour × alpha (what the quad adds over its ground) stays exact; at
/// a few percent alpha one channel step is a thirtieth of a pixel level.
pub(super) fn premultiplied_hex(color: gpui::Hsla) -> String {
    let c = color.to_rgb();
    let alpha = (c.a.clamp(0., 1.) * 255.).round().max(1.);
    let channel = |v: f32| ((v * c.a * 255. / alpha) * 255.).round().clamp(0., 255.) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", channel(c.r), channel(c.g), channel(c.b), alpha as u8)
}

/// A run of text cut to one line with an ellipsis.
fn one_line(ink: &Ink, id: &str, text: Value, rems: f32, color: &str) -> Value {
    let mut t = ink.text(id, text, rems, color);
    t["props"]["lines"] = json!(1);
    t["style"]["minWidth"] = json!(0);
    t["style"]["flexShrink"] = json!(1);
    t
}

/// The web's radius-10 outline `Button` at `web_sm` (gpui-component's
/// outline Default: the input wash and stroke, 32px tall, `px 12`, its
/// `text_sm` label at line height 1, which gpui centres half a point lower
/// than the flex centre: `paddingTop: 1`), in the danger tint where `danger`;
/// disabled = the half wash, the half stroke, the faded muted label (the
/// tint stays: it is the caller's own style).
fn outline_button(ink: &Ink, chrome: &Chrome, id: &str, disabled: Value) -> [Value; 2] {
    let draw = |dim: bool| {
        let id = if dim { format!("{id}_dim") } else { id.to_string() };
        let size = ink.r(SM);
        let label = |color: &str, suffix: &str, visible: Value| json!({"id": format!("{id}_label{suffix}"), "component": "Text", "visible": visible, "props": {"text": path("action"), "lines": 1}, "style": {"fontSize": size, "lineHeight": size, "paddingTop": 1, "color": color, "flexShrink": 0}});
        let plain = if dim { &chrome.button_fg_disabled } else { &chrome.button_fg };
        let mut button = json!({"id": id, "component": "Box", "visible": if dim { disabled.clone() } else { not(disabled.clone()) },
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": 32, "paddingHorizontal": 12,
                "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": if dim { &chrome.button_stroke_disabled } else { &chrome.button_stroke }, "backgroundColor": if dim { &chrome.button_fill_disabled } else { &chrome.button_fill }},
            "children": [label(&chrome.danger, "_danger", path("danger")), label(plain, "", not(path("danger")))]});
        if !dim {
            button["props"] = json!({"pressable": true});
            button["accessibility"] = json!({"label": path("action")});
            button["on"] = json!({"press": {"event": {"name": "revoke", "context": {"key": path("key")}}}});
            button["style"]["cursor"] = json!("pointer");
        }
        button
    };
    [draw(false), draw(true)]
}

/// One list (`sessions` or `keys`): EXP-862 flat rows (`px_3` / `py_2p5`,
/// `gap_3`) under the band, no hairline between them (EXP-1320).
fn ladder(ink: &Ink, chrome: &Chrome, list: &str, visible: Value) -> Value {
    let mut name = one_line(ink, &format!("{list}_name"), path("name"), SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    let mut badge = ink.pill(&format!("{list}_device"), json!("This device"), false, None);
    badge["visible"] = path("thisDevice");
    let mut scope = one_line(ink, &format!("{list}_scope"), path("scope"), XS, &ink.muted);
    scope["visible"] = path("hasScope");
    let [button, button_dim] = outline_button(ink, chrome, &format!("{list}_action"), path("/pane/busy"));
    let row = json!({"id": format!("{list}_row"), "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.625), "borderRadius": ink.radius_md},
        "children": [
            {"id": format!("{list}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
                "children": [
                    {"id": format!("{list}_head"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "minWidth": 0, "gap": ink.r(0.5)}, "children": [name, badge]},
                    one_line(ink, &format!("{list}_subline"), path("subline"), XS, &ink.muted),
                    scope,
                ]},
            button,
            button_dim,
        ]});
    json!({"id": format!("{list}_rows"), "component": "List", "visible": visible, "props": {"gap": "none"},
        "template": {"component": format!("{list}_wrap"), "path": format!("/pane/{list}"), "key": "key"},
        "children": [{"id": format!("{list}_wrap"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]})
}

/// The one-time reveal after a mint (`px_3` / `py_2`, `gap_2`, the theme
/// radius in a 40% primary stroke over a 5% primary wash).
fn reveal(ink: &Ink, chrome: &Chrome) -> Value {
    let mut title = ink.text("reveal_title", json!("Copy your API key"), SM, &ink.foreground);
    title["style"]["fontWeight"] = json!(500);
    let note = ink.text("reveal_note", json!("This is the only time the full key is shown. Store it somewhere safe \u{2014} only a hash stays on the server."), XS, &ink.muted);
    let mut key = one_line(ink, "reveal_key_text", path("/pane/key"), XS, &ink.foreground);
    key["style"]["fontFamily"] = json!(chrome.terminal);
    let [copy, _] = ink.pill_button("reveal_copy", None, json!("Copy"), json!(false), json!({"name": "copy"}));
    let [done, _] = ink.pill_button("reveal_done", None, json!("Done"), json!(false), json!({"name": "done"}));
    json!({"id": "reveal", "component": "Box", "visible": path("/pane/minted"),
        "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.primary_stroke, "backgroundColor": chrome.primary_wash},
        "children": [title, note,
            {"id": "reveal_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.5)},
                "children": [
                    {"id": "reveal_key", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.25), "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.stroke}, "children": [key]},
                    copy, done,
                ]}]})
}

/// The pane's tree: Login sessions over API keys (EXP-1054), each a band
/// over its flat list, the Create pill in the keys band (EXP-771), the
/// one-time reveal under it; skeletons while loading; a failed load = the
/// empty sessions line and the keys' Retry state (FEED-79).
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let empty = |id: &str, text: &str, visible: Value| {
        let mut t = ink.text(id, json!(text), SM, &ink.muted);
        t["visible"] = visible;
        t
    };
    let section = |id: &str, children: Vec<Value>| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": children});
    let sessions = section("sessions", vec![
        ink.band("sessions_band", "Login sessions", None),
        skeleton("sessions_skeleton", path("/pane/loading")),
        empty("sessions_empty", "No signed-in devices.", json!({"@call": "or", "args": {"values": [path("/pane/failed"), and(path("/pane/ready"), path("/pane/noSessions"))]}})),
        ladder(ink, chrome, "sessions", and(path("/pane/ready"), not(path("/pane/noSessions")))),
    ]);
    let create = ink.pill_button("create", None, json!("Create key"), path("/pane/createDisabled"), json!({"name": "create"}));
    let create = json!({"id": "create_slot", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "flexShrink": 0}, "children": create});
    let keys = section("keys", vec![
        ink.band("keys_band", "API keys", Some(create)),
        reveal(ink, chrome),
        skeleton("keys_skeleton", path("/pane/loading")),
        failed_state(ink, "keys_failed", "settings-api", "No API keys", path("/pane/failed")),
        empty("keys_empty", "No API keys.", and(path("/pane/ready"), path("/pane/noKeys"))),
        ladder(ink, chrome, "keys", and(path("/pane/ready"), not(path("/pane/noKeys")))),
    ]);
    // The pane: `v_flex().gap_6()`.
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%"}, "children": [sessions, keys]})
}

impl Render for ApiKeysPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ApiKeysPane").entered();
        self.ensure_loaded(cx);
        let projection = self.project();
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
pub(crate) enum ShotLoad {
    Ready(Vec<PersonalKeyMeta>, Option<String>),
    Failed,
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

    /// #EXP-1629: the alert bodies raise no issue a debug build's mount
    /// check refuses (#EXP-1767's class).
    #[test]
    fn the_alert_documents_raise_no_unexpected_issue() {
        let ink = super::super::readiness::tests::test_ink();
        for (id, tree) in [("api-key-revoke", revoke_document(&ink)), ("api-key-create", mint_document(&ink, "#f87171ff"))] {
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new(id, Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{id}: {issues:?}");
        }
    }

    #[test]
    fn row_subline_matches_the_web() {
        let mut key = PersonalKeyMeta {
            id: "k".into(),
            name: None,
            start: Some("expu_ab".into()),
            prefix: None,
            created_at: Some("2026-03-04T10:00:00Z".into()),
            last_request: None,
            scope: None,
        };
        assert_eq!(
            row_subline(&key, false),
            "expu_ab… · created Mar 4, 2026 · last used never"
        );
        key.last_request = Some("2026-04-01T00:00:00Z".into());
        assert_eq!(
            row_subline(&key, true),
            "Signed in Mar 4, 2026 · last active Apr 1, 2026"
        );
    }

    fn meta(name: Option<&str>) -> PersonalKeyMeta {
        PersonalKeyMeta {
            id: "k-1".to_string(),
            name: name.map(str::to_string),
            start: Some("expu_ab".to_string()),
            prefix: Some("expu_".to_string()),
            created_at: None,
            last_request: None,
            scope: None,
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
