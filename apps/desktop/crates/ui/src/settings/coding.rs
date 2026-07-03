//! Settings → Coding (masterplan-v3 §7.7 DC-3, §7.2, EXP-2a/b/f).
//!
//! The JetBrains-SDK-settings-style pane for the Start-coding launcher:
//!
//! | Setting              | Default               |
//! |----------------------|-----------------------|
//! | Claude CLI path      | `claude`              |
//! | Repos & worktrees root | `~/Exponential/repos` |
//! | Branch prefix        | `exp/`                |
//! | Personal API key     | STATUS row only (§7.2) |
//! | Tooling doctor       | "Check tools"         |
//!
//! Settings persist through [`crate::coding_flow::CodingHub`] to the local
//! per-install `settings.json` — never synced. Saving re-runs the doctor
//! against the new claude path, and the doctor's report is exactly what
//! gates the Start-coding button (§7.1 step 1 ANDs `claude.ok && git.ok`).
//! The doctor auto-runs when the hub first exists (the §7.7 onboarding rule:
//! clear errors BEFORE Start coding is usable — the red rows here carry the
//! actionable copy: "claude not found on PATH — set an absolute path" /
//! "git not found on PATH").
//!
//! **EXP-2a is enforced by construction: there is NO key value field, no
//! reveal, no copy, no manual entry.** The personal key renders as a status
//! row ("active · `<start>`…") with **Regenerate** as the only control —
//! mint-new-then-revoke-old, in that order (`api::users::
//! regenerate_personal_key` owns the ordering). A device without a key shows
//! "created automatically on your first coding session".

use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};

use api::token_store::{SecretKind, TokenStore};
use coding::{DoctorReport, Settings, ToolCheck};

use crate::coding_flow::CodingHub;
use crate::queries;
use crate::session::AuthContext;

use super::{card, card_header, error_notice};

// ---------------------------------------------------------------------------
// Personal-key status (§7.2 — display metadata only, never the secret)
// ---------------------------------------------------------------------------

/// What the status row renders. Built off the local token store (does a key
/// exist at all?) + `users.listPersonalApiKeys` (the non-secret `start`
/// prefix of OUR key row, matched by the stored row id).
#[derive(Clone, Debug)]
struct KeyStatus {
    present: bool,
    /// e.g. `expu_ab` — non-secret display prefix; `None` when the list call
    /// failed or the row is gone (still "active", just unadorned).
    start: Option<String>,
}

enum KeyLoad {
    Idle,
    Loading,
    Ready(KeyStatus),
    Error(String),
}

/// Blocking status read (background executor): local presence + listed start.
fn load_key_status(
    trpc: &api::TrpcClient,
    store: &TokenStore,
    account_id: &str,
) -> Result<KeyStatus, api::ApiError> {
    let present = store.get(account_id, SecretKind::PersonalApiKey).is_some();
    if !present {
        return Ok(KeyStatus { present: false, start: None });
    }
    let row_id = store.get(account_id, SecretKind::PersonalApiKeyId);
    // Best-effort: a failed list still renders "active" (the key itself is
    // local truth); only the pretty prefix is lost.
    let start = api::users::list_personal_api_keys(trpc)
        .ok()
        .and_then(|keys| {
            let row = match &row_id {
                Some(id) => keys.iter().find(|key| &key.id == id),
                None => None,
            };
            row.and_then(|key| key.start.clone())
        });
    Ok(KeyStatus { present: true, start })
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

pub struct CodingPane {
    claude_input: Entity<InputState>,
    repos_input: Entity<InputState>,
    prefix_input: Entity<InputState>,
    /// The hub settings the inputs were last synced from (dirty baseline).
    synced: Option<Settings>,
    save_error: Option<SharedString>,
    key: KeyLoad,
    key_generation: u64,
    regenerating: bool,
    _subscriptions: Vec<Subscription>,
}

impl CodingPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let claude_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_CLAUDE_PATH));
        let repos_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_REPOS_ROOT));
        let prefix_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(coding::settings::DEFAULT_BRANCH_PREFIX));

        // Creating the hub also kicks the FIRST doctor run (§7.7 onboarding).
        let hub = CodingHub::global(cx);
        let mut subscriptions = vec![
            // Doctor results / external settings changes re-render + resync.
            cx.observe_in(&hub, window, |this, _, window, cx| {
                this.resync(window, cx);
                cx.notify();
            }),
        ];
        for input in [&claude_input, &repos_input, &prefix_input] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify(); // live dirty tracking on the Save button
                }
            }));
        }

        let mut this = Self {
            claude_input,
            repos_input,
            prefix_input,
            synced: None,
            save_error: None,
            key: KeyLoad::Idle,
            key_generation: 0,
            regenerating: false,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Mirror the hub's settings into the inputs whenever they change out
    /// from under us (save from another pane instance, first build).
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        if self.synced.as_ref() == Some(&settings) {
            return;
        }
        self.claude_input.update(cx, |input, cx| {
            input.set_value(settings.claude_path.clone(), window, cx)
        });
        self.repos_input.update(cx, |input, cx| {
            input.set_value(settings.repos_root.clone(), window, cx)
        });
        self.prefix_input.update(cx, |input, cx| {
            input.set_value(settings.branch_prefix.clone(), window, cx)
        });
        self.synced = Some(settings);
        cx.notify();
    }

    /// The settings the inputs currently describe. Blank fields degrade to
    /// the §7.7 defaults — a hand-blanked pane can never produce an unusable
    /// launcher (mirrors `Settings::load`).
    fn drafted(&self, cx: &App) -> Settings {
        let defaults = Settings::default();
        let value = |input: &Entity<InputState>, default: &str| {
            let raw = input.read(cx).value().trim().to_string();
            if raw.is_empty() {
                default.to_string()
            } else {
                raw
            }
        };
        Settings {
            claude_path: value(&self.claude_input, &defaults.claude_path),
            repos_root: value(&self.repos_input, &defaults.repos_root),
            branch_prefix: value(&self.prefix_input, &defaults.branch_prefix),
        }
    }

    fn dirty(&self, cx: &App) -> bool {
        self.synced
            .as_ref()
            .map(|synced| *synced != self.drafted(cx))
            .unwrap_or(false)
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let drafted = self.drafted(cx);
        let hub = CodingHub::global(cx);
        self.save_error = CodingHub::save_settings(&hub, drafted.clone(), cx)
            .err()
            .map(SharedString::from);
        // `synced` follows the hub via the observer's resync; setting it here
        // too keeps the Save button honest when the observer coalesces.
        self.synced = Some(drafted);
        cx.notify();
    }

    // -- personal key -------------------------------------------------------

    fn ensure_key_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if !matches!(self.key, KeyLoad::Idle) {
            return;
        }
        self.reload_key(cx);
    }

    fn reload_key(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let data_dir = cx
            .try_global::<AuthContext>()
            .map(|auth| auth.data_dir.clone())
            .unwrap_or_else(api::default_data_dir);

        self.key = KeyLoad::Loading;
        self.key_generation += 1;
        let generation = self.key_generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let store = TokenStore::new(data_dir);
                    load_key_status(&trpc, &store, &account.id)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.key_generation != generation {
                    return;
                }
                this.key = match result {
                    Ok(status) => KeyLoad::Ready(status),
                    Err(err) => KeyLoad::Error(err.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// §7.2 Regenerate — the ONLY key control. Mint-new-then-revoke-old
    /// (ordering owned by `api::users::regenerate_personal_key`; a crash
    /// mid-way must never leave the device keyless). The next launch's
    /// `.mcp.json` picks the new key up from the token store.
    fn regenerate(&mut self, cx: &mut gpui::Context<Self>) {
        if self.regenerating {
            return;
        }
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let data_dir = cx
            .try_global::<AuthContext>()
            .map(|auth| auth.data_dir.clone())
            .unwrap_or_else(api::default_data_dir);
        self.regenerating = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let store = TokenStore::new(data_dir);
                    api::users::regenerate_personal_key(&trpc, &store, &account.id, None)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.regenerating = false;
                match result {
                    Ok(_) => this.reload_key(cx),
                    Err(err) => this.key = KeyLoad::Error(format!("Regenerate failed: {err}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- render pieces --------------------------------------------------------

    fn labeled_input(
        label: &'static str,
        hint: &'static str,
        input: &Entity<InputState>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
            .child(Input::new(input).small())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(hint),
            )
    }

    /// One doctor row: green check + version, or red X + the actionable error.
    fn doctor_row(check: &ToolCheck, cx: &App) -> impl IntoElement {
        let (icon, color, detail): (IconName, gpui::Hsla, SharedString) = if check.ok {
            (
                IconName::CircleCheck,
                theme::tokens::GREEN.to_hsla(),
                check.version.clone().unwrap_or_default().into(),
            )
        } else {
            (
                IconName::CircleX,
                cx.theme().danger,
                check
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("{} is not available", check.tool))
                    .into(),
            )
        };
        h_flex()
            .gap_2()
            .items_center()
            .child(Icon::new(icon).small().text_color(color))
            .child(
                div()
                    .w_16()
                    .flex_shrink_0()
                    .text_sm()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(check.tool.label())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(if check.ok {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().danger
                    })
                    .child(detail),
            )
    }

    fn render_doctor_card(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let hub = CodingHub::global(cx);
        let (report, running): (Option<DoctorReport>, bool) = {
            let hub = hub.read(cx);
            (hub.doctor.report.clone(), hub.doctor.running)
        };

        let mut body = card(cx).child(card_header(
            "Tooling doctor",
            "Start coding needs both tools. A red row blocks the button until it's fixed.",
            cx,
        ));
        match &report {
            None => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_64())
                        .child(Skeleton::new().h_4().w_56()),
                );
            }
            Some(report) => {
                body = body
                    .child(Self::doctor_row(&report.claude, cx))
                    .child(Self::doctor_row(&report.git, cx));
            }
        }
        body.child(
            h_flex().child(
                Button::new("doctor-check")
                    .outline()
                    .xsmall()
                    .label("Check tools")
                    .loading(running)
                    .disabled(running)
                    .on_click(cx.listener(|_, _, _, cx| {
                        let hub = CodingHub::global(cx);
                        CodingHub::refresh_doctor(&hub, cx);
                    })),
            ),
        )
    }

    fn render_key_card(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_key_loaded(cx);
        let mut body = card(cx).child(card_header(
            "Personal API key",
            "Authenticates the coding agent as you. Created and rotated automatically — \
             there is never a key to copy or paste.",
            cx,
        ));

        let status: gpui::AnyElement = match &self.key {
            KeyLoad::Idle | KeyLoad::Loading => {
                Skeleton::new().h_4().w_48().into_any_element()
            }
            KeyLoad::Error(message) => {
                error_notice(SharedString::from(message.clone()), cx).into_any_element()
            }
            KeyLoad::Ready(status) if status.present => {
                let label: SharedString = match &status.start {
                    Some(start) => format!("active · {start}…").into(),
                    None => "active".into(),
                };
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .text_sm()
                    .child(
                        div()
                            .size_2()
                            .rounded_full()
                            .bg(theme::tokens::GREEN.to_hsla()),
                    )
                    .child(label)
                    .into_any_element()
            }
            KeyLoad::Ready(_) => div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("No key yet — it's created automatically on your first coding session.")
                .into_any_element(),
        };

        let has_key = matches!(&self.key, KeyLoad::Ready(status) if status.present);
        body = body.child(
            h_flex()
                .gap_3()
                .items_center()
                .justify_between()
                .child(status)
                .child(
                    Button::new("key-regenerate")
                        .outline()
                        .xsmall()
                        .label("Regenerate")
                        .tooltip(
                            "Mint a fresh key, then revoke the old one. The next coding \
                             session uses the new key automatically.",
                        )
                        .loading(self.regenerating)
                        .disabled(self.regenerating || !has_key)
                        .on_click(cx.listener(|this, _, _, cx| this.regenerate(cx))),
                ),
        );
        body
    }
}

impl Render for CodingPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let dirty = self.dirty(cx);

        let mut settings_card = card(cx)
            .child(card_header(
                "Coding",
                "Local launcher settings for \u{201c}Start coding\u{201d} — per machine, never synced.",
                cx,
            ))
            .child(Self::labeled_input(
                "Claude CLI path",
                "Command name or absolute path — used verbatim to launch coding sessions.",
                &self.claude_input,
                cx,
            ))
            .child(Self::labeled_input(
                "Repos & worktrees root",
                "Where repositories are cloned and per-issue worktrees are created (~ works).",
                &self.repos_input,
                cx,
            ))
            .child(Self::labeled_input(
                "Branch prefix",
                "Prepended to the issue identifier for the coding branch (exp/EXP-42).",
                &self.prefix_input,
                cx,
            ));
        if let Some(error) = &self.save_error {
            settings_card = settings_card.child(error_notice(error.clone(), cx));
        }
        settings_card = settings_card.child(
            h_flex().justify_end().child(
                Button::new("coding-save")
                    .primary()
                    .small()
                    .label("Save changes")
                    .disabled(!dirty)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            ),
        );

        v_flex()
            .gap_4()
            .child(settings_card)
            .child(self.render_doctor_card(cx))
            .child(self.render_key_card(cx))
    }
}
