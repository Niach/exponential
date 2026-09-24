//! Settings → General + Danger Zone (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/general-section.tsx` (name `Input`,
//! dirty-gated Save; teams are always private — there is no visibility
//! setting) and the Danger Zone card of
//! `routes/t/$teamSlug/settings/general.tsx` (type-the-name-to-confirm
//! delete, gated owner + team-only).
//!
//! Local state mirrors the web's `useState` + resync-on-team-change
//! `useEffect`: an Electric echo that changes the synced row overwrites the
//! local draft (which is exactly how the post-save echo clears `dirty`).

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _,
};
use sync::Store;

use crate::controls::{glass_input, web_textarea, WebControl as _};
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::Navigation;

use super::{
    active_team, danger_zone, error_notice, is_owner, open_url, section,
    team_delete_error_message,
};
use crate::icons::registry;

/// Server fetch state for the read-only billing summary (EXP-288).
enum BillingLoad {
    Idle,
    Loading,
    Ready(api::billing::TeamPlanOut),
    /// Errors hide the section (logged) — billing must never block the
    /// name/danger-zone cards.
    Failed,
}

/// Snapshot of the synced fields the pane mirrors — resync happens whenever
/// this differs from the live row (the web `useEffect` dep list).
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    team_id: String,
    name: String,
}

// EXP-1025: the team prompt editor's copy — byte-identical to the web
// (`components/team/general-section.tsx`).
const TEAM_PROMPT_TITLE: &str = "Team prompt";
const TEAM_PROMPT_PLACEHOLDER: &str = "Rules every coding run of this team should follow, as \
markdown. Repo facts belong in CLAUDE.md; this is for team process, conventions and who to ask.";
const TEAM_PROMPT_HELP: &str = "Appended to the agent's system prompt after the run playbook, on \
every start and resume. It reaches every run on every member's machine, with the agent's full \
permissions. Applies to runs started or resumed from now on.";

/// `12.3k` / `840` — the counter's short form (web `formatByteCount`).
fn format_byte_count(bytes: usize) -> String {
    if bytes < 1000 {
        bytes.to_string()
    } else {
        format!("{:.1}k", bytes as f64 / 1024.0)
    }
}

/// The rough token readout beside the byte counter (web `approxTokens`):
/// chars÷4, labelled ≈ so nobody reads it as a measurement.
fn approx_tokens(bytes: usize) -> String {
    let tokens = (bytes as f64 / 4.0).round() as usize;
    if tokens < 1000 {
        format!("≈{tokens} tokens")
    } else {
        format!("≈{:.1}k tokens", tokens as f64 / 1000.0)
    }
}

/// The counter line: `1.2k / 12.0k bytes · ≈300 tokens`.
fn prompt_counter(bytes: usize) -> String {
    format!(
        "{} / {} bytes · {}",
        format_byte_count(bytes),
        format_byte_count(domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES),
        approx_tokens(bytes)
    )
}

/// EXP-1025: the team prompt's fetch state. The text is NOT on the synced
/// team row (server-only, like an action's body), so the pane loads it once
/// per team and the field stays disabled until it lands — a blur can never
/// save an empty draft over a prompt that has not arrived.
#[derive(Default)]
struct PromptState {
    /// The team the loaded text belongs to; a team switch reloads.
    team_id: Option<String>,
    loading: bool,
    /// What the server holds (trailing whitespace dropped, like it does).
    saved: String,
    /// ISO stamp of the last write, for the "Edited …" caption.
    updated_at: Option<String>,
    saving: bool,
    error: Option<SharedString>,
    /// Bumped per fetch so a slow reply for the previous team is dropped.
    generation: u64,
}

pub struct GeneralPane {
    nav: Entity<Navigation>,
    name_input: Entity<InputState>,
    delete_input: Entity<InputState>,
    /// EXP-1025: the team prompt — a RAW monospace field, never the markdown
    /// WYSIWYG: a prompt is read by a model, so the bytes shown must be the
    /// bytes it gets.
    prompt_input: Entity<TextareaState>,
    prompt: PromptState,
    snapshot: Option<Snapshot>,
    saving: bool,
    error: Option<SharedString>,
    /// REV2-55: the server can REFUSE a delete (a live subscription, a lost
    /// ownership race), so the Danger Zone shows why instead of leaving the
    /// confirm dialog looking like it worked.
    delete_error: Option<SharedString>,
    /// EXP-288: the read-only plan/usage summary between the name card and
    /// the Danger Zone. Refetched on team/account change; hidden while
    /// loading/failed and entirely on self-hosted (`plan == "unlimited"`).
    billing: BillingLoad,
    billing_team: Option<String>,
    billing_account: Option<String>,
    billing_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl GeneralPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Team name"));
        let delete_input = cx.new(|cx| InputState::new(window, cx));
        let prompt_input = cx.new(|cx| web_textarea(10, 40, window, cx).placeholder("Loading…"));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Resync needs the window (set_value) — window-aware observers.
            cx.observe_in(&nav, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe_in(&collections.teams, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // EXP-818: the name row saves ITSELF — on blur and on Enter, the
            // Linear way (web `GlassInputRow` twin). No Save button.
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } | InputEvent::Blur => this.save(cx),
                _ => {}
            }),
            // EXP-1025: the prompt saves itself on blur too (Enter is a
            // newline in a textarea); Change re-renders the counter.
            cx.subscribe(&prompt_input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::Blur => this.save_prompt(cx),
                _ => {}
            }),
        ];

        let mut this = Self {
            nav,
            name_input,
            delete_input,
            prompt_input,
            prompt: PromptState::default(),
            snapshot: None,
            saving: false,
            error: None,
            delete_error: None,
            billing: BillingLoad::Idle,
            billing_team: None,
            billing_account: None,
            billing_generation: 0,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Kick the `billing.teamPlan` fetch when the pane is shown or the
    /// team/account changed (render-time — a hidden pane never fetches;
    /// generation-guarded like the boards repo cache).
    fn ensure_billing(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.billing_account {
            self.billing_account = account_id;
            self.billing = BillingLoad::Idle;
        }
        if self.billing_team.as_deref() == Some(team_id)
            && !matches!(self.billing, BillingLoad::Idle)
        {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.billing = BillingLoad::Loading;
        self.billing_team = Some(team_id.to_string());
        self.billing_generation += 1;
        let generation = self.billing_generation;
        let team_id = team_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::billing::billing_team_plan(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.billing_generation != generation {
                    return; // superseded
                }
                this.billing = match result {
                    Ok(plan) => BillingLoad::Ready(plan),
                    Err(err) => {
                        log::warn!("[ui] billing.teamPlan failed: {err}");
                        BillingLoad::Failed
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Mirror the web `useEffect`: whenever the synced row (or the active
    /// team) changes, replace the local draft.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team) = active_team(cx, &self.nav) else {
            return;
        };
        let snapshot = Snapshot {
            team_id: team.id.clone(),
            name: team.name.clone(),
        };
        if self.snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.name_input.update(cx, |state, cx| {
            state.set_value(snapshot.name.clone(), window, cx);
        });
        let team_changed = self.snapshot.as_ref().map(|held| &held.team_id) != Some(&snapshot.team_id);
        self.snapshot = Some(snapshot);
        // A refused delete belonged to the team that was selected then.
        self.delete_error = None;
        if team_changed {
            self.fetch_prompt(window, cx);
        }
        cx.notify();
    }

    /// EXP-1025: the ONE read path for the team prompt (`teams.getAgentPrompt`;
    /// the shape excludes it). Clears the field first so the previous team's
    /// text never shows under the new team's name.
    fn fetch_prompt(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.snapshot.as_ref().map(|snapshot| snapshot.team_id.clone()) else {
            return;
        };
        self.prompt.generation += 1;
        let generation = self.prompt.generation;
        self.prompt.team_id = Some(team_id.clone());
        self.prompt.loading = true;
        self.prompt.saved.clear();
        self.prompt.updated_at = None;
        self.prompt.error = None;
        self.prompt_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.set_placeholder("Loading…", window, cx);
        });
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            self.prompt.loading = false;
            self.prompt.error = Some("Not signed in.".into());
            return;
        };
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::teams::teams_get_agent_prompt(&trpc, &team_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                if view.prompt.generation != generation {
                    return;
                }
                view.prompt.loading = false;
                match result {
                    Ok(prompt) => {
                        view.prompt.saved = prompt.agent_prompt.clone();
                        view.prompt.updated_at = prompt.agent_prompt_updated_at;
                        view.prompt_input.update(cx, |state, cx| {
                            state.set_value(prompt.agent_prompt, window, cx);
                            state.set_placeholder(TEAM_PROMPT_PLACEHOLDER, window, cx);
                        });
                    }
                    Err(err) => {
                        view.prompt.error =
                            Some(format!("Couldn't load the team prompt: {}", err.user_message()).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn prompt_bytes(&self, cx: &App) -> usize {
        self.prompt_input.read(cx).value().len()
    }

    fn prompt_dirty(&self, cx: &App) -> bool {
        !self.prompt.loading
            && self.prompt_input.read(cx).value().trim_end() != self.prompt.saved
    }

    /// EXP-1025: save on blur — the web `handleSavePrompt` twin. Over the
    /// cap nothing is sent (the counter is already red and the server would
    /// refuse it anyway).
    fn save_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.prompt.team_id.clone() else {
            return;
        };
        if self.prompt.loading || self.prompt.saving || !self.prompt_dirty(cx) {
            return;
        }
        if self.prompt_bytes(cx) > domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        let text = self.prompt_input.read(cx).value().to_string();
        let saved = text.trim_end().to_string();
        let mut input = api::teams::TeamsUpdateInput::new(team_id);
        input.agent_prompt = Some(text);

        self.prompt.saving = true;
        self.prompt.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::teams::teams_update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.prompt.saving = false;
                match result {
                    Ok(_) => {
                        this.prompt.saved = saved;
                        this.prompt.updated_at = Some(chrono::Utc::now().to_rfc3339());
                    }
                    Err(err) => {
                        this.prompt.error = Some(
                            format!("Couldn't save the team prompt: {}", err.user_message()).into(),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-1025: the team prompt section — header with the save caption, the
    /// raw monospace field in its own glass block, the help line and the
    /// byte/token counter under it. Owner-only, and HIDDEN from everyone
    /// else like every owner control (the web route hides the whole
    /// section too): a non-owner gets nothing here, not a disabled field.
    fn render_prompt_section(&self, owner: bool, cx: &mut gpui::Context<Self>) -> gpui::Div {
        if !owner {
            return div();
        }
        let bytes = self.prompt_bytes(cx);
        let over = bytes > domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES;
        let dirty = self.prompt_dirty(cx);
        let caption: Option<SharedString> = if self.prompt.saving {
            Some("Saving…".into())
        } else if dirty {
            Some("Unsaved".into())
        } else {
            self.prompt
                .updated_at
                .as_deref()
                .map(|stamp| format!("Edited {}", crate::inbox::relative_time(stamp)).into())
        };
        let trailing = caption.map(|caption| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption)
                .into_any_element()
        });
        let field = Textarea::new(&self.prompt_input)
            .appearance(false)
            .w_full()
            .px_4()
            .py_3()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_xs()
            .disabled(self.prompt.loading);
        let footer = h_flex()
            .w_full()
            .items_start()
            .justify_between()
            .gap_4()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(super::row_stroke(cx))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(div().min_w_0().flex_1().child(TEAM_PROMPT_HELP))
            .child(
                div()
                    .flex_shrink_0()
                    .when(over, |counter| counter.text_color(cx.theme().danger))
                    .child(prompt_counter(bytes)),
            );
        let mut body = section(cx)
            .child(crate::surface::glass_section_header(TEAM_PROMPT_TITLE, trailing, cx))
            .child(crate::surface::glass_group().child(field).child(footer));
        if let Some(error) = &self.prompt.error {
            body = body.child(error_notice(error.clone(), cx));
        }
        body
    }

    fn dirty(&self, cx: &App) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        self.name_input.read(cx).value().as_ref() != snapshot.name
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        if !self.dirty(cx) || self.saving {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        let typed = self.name_input.read(cx).value().trim().to_string();
        // Web: `name.trim() || team.name` — an emptied field falls back.
        let name = if typed.is_empty() {
            snapshot.name.clone()
        } else {
            typed
        };
        let mut input = api::teams::TeamsUpdateInput::new(snapshot.team_id.clone());
        input.name = Some(name);

        self.saving = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::teams::teams_update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                if let Err(err) = result {
                    this.error = Some(format!("Failed to save changes: {err}").into());
                }
                // Success needs no action: the Electric echo resyncs the
                // snapshot, which clears `dirty`.
                cx.notify();
            });
        })
        .detach();
    }

    /// The EXP-288 read-only billing summary: plan chip + usage bars + a
    /// "Manage on the web" hand-off. Returns `None` while loading/failed,
    /// on self-hosted (`unlimited`), or without a synced team slug.
    fn render_billing_section(
        &self,
        team: &domain::rows::Team,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Div> {
        let BillingLoad::Ready(plan) = &self.billing else {
            return None;
        };
        if plan.plan == "unlimited" {
            return None; // self-hosted: no billing surface at all (web parity)
        }
        // Web `PLAN_LABELS` (billing-section.tsx). EXP-771: this still named
        // the retired `pro`/`business` tiers, so the ONE paid tier fell to the
        // fallback arm and the chip read a raw lowercase "team".
        let plan_label: SharedString = match plan.plan.as_str() {
            "free" => "Free".into(),
            "team" => "Team".into(),
            "unlimited" => "Unlimited".into(),
            other => other.to_string().into(),
        };
        let plan_chip = div()
            .px_2()
            .py_0p5()
            .rounded_full()
            .border_1()
            .border_color(cx.theme().primary.opacity(0.4))
            .bg(cx.theme().primary.opacity(0.1))
            .text_xs()
            .child(plan_label);

        let fraction = |current: f64, limit: Option<f64>| {
            limit.filter(|limit| *limit > 0.).map(|limit| current / limit)
        };
        let seats = super::usage_bar(
            "Seats",
            format!("{}", plan.usage.members.round() as i64),
            plan.limits
                .seats
                .map(|seats| format!("{}", seats.round() as i64)),
            fraction(plan.usage.members, plan.limits.seats),
            cx,
        );
        let storage = super::usage_bar(
            "Attachment storage",
            super::format_storage(plan.usage.storage_mb),
            plan.limits.storage_mb.map(super::format_storage),
            fraction(plan.usage.storage_mb, plan.limits.storage_mb),
            cx,
        );
        let widgets = super::usage_bar(
            "Feedback widgets",
            format!("{}", plan.usage.widget_configs.round() as i64),
            plan.limits
                .widget_configs
                .map(|widgets| format!("{}", widgets.round() as i64)),
            fraction(plan.usage.widget_configs, plan.limits.widget_configs),
            cx,
        );

        let mut body = section(cx)
            .child(
                crate::surface::glass_section_header(
                    "Plan & Billing",
                    Some(plan_chip.into_any_element()),
                    cx,
                ),
            )
            .child(
                // EXP-862: the usage meters sit in the section's own glass
                // block, not in a bordered card inside a card.
                crate::surface::glass_group()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .child(seats)
                    .child(storage)
                    .child(widgets),
            );

        // Checkout/portal/seat changes are web flows (Creem runs in the
        // browser) — hand off to the team's billing settings page.
        if let Some(slug) = team.slug.clone() {
            if let Some(account) = crate::queries::active_account(cx) {
                let url = format!(
                    "{}/t/{slug}/settings/billing",
                    account.instance_url.trim_end_matches('/')
                );
                body = body.child(
                    h_flex().child(
                        Button::new("billing-manage")
                            .outline()
                            .web_sm()
                            .icon(registry::UI_EXTERNAL_LINK)
                            .label("Manage billing on the web")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                open_url(cx, url.clone());
                            })),
                    ),
                );
            }
        }
        Some(body)
    }

    fn open_delete_dialog(
        &mut self,
        team_id: String,
        team_name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.delete_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        // Re-opening the confirm clears the previous refusal.
        self.delete_error = None;
        cx.notify();
        let pane = cx.entity().downgrade();
        let content_input = self.delete_input.clone();
        let ok_input = self.delete_input.clone();
        let confirm_name = team_name.clone();
        let prompt = format!("Type {team_name} to confirm");
        // The typed-confirm block rides as extra content between the
        // description and the ok/cancel footer.
        let spec = AlertSpec::new(
            "Delete team",
            format!(
                "This will permanently delete {team_name} and all its boards, \
                 issues, and data. This cannot be undone."
            ),
            "Delete team",
        )
        .ok_variant(ButtonVariant::Danger)
        .height(gpui::px(320.))
        .content(move |window, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(prompt.clone())),
                )
                .child(glass_input(&content_input, window, cx).web_input_sm())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let typed = ok_input.read(cx).value().trim().to_string();
            if typed != confirm_name {
                // Mismatch keeps the dialog open (web disables the button
                // until it matches).
                return false;
            }
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                log::warn!("[ui] teams.delete skipped: no signed-in account");
                return true;
            };
            let team_id = team_id.clone();
            let pane = pane.clone();
            // Not fire-and-forget like the other Danger Zone mutations: the
            // server REFUSES a team whose subscription is still live (REV2-55,
            // PRECONDITION_FAILED), so the failure has to land on the screen.
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::teams::teams_delete(&trpc, &team_id) })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    if let Err(err) = &result {
                        log::warn!("[ui] teams.delete failed: {err}");
                        this.delete_error = Some(team_delete_error_message(err));
                        cx.notify();
                    }
                    // Success needs no action: the Electric echo drops the
                    // team and navigation re-scopes.
                });
            })
            .detach();
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }
}

impl Render for GeneralPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team) = active_team(cx, &self.nav) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        // Team visibility is deliberately not configurable (v6) — the pane
        // is just the name card, billing summary and Danger Zone.
        let owner = is_owner(cx, &team.id);
        let dirty = self.dirty(cx);
        let saving = self.saving;

        // EXP-818: label left, value right, saves on blur/Enter — the row
        // vocabulary the device editor's Name row wears; a "Saving…" caption
        // trails while the write is out.
        let name_row = crate::surface::glass_input_row(
            "Name",
            crate::surface::glass_row_input(
                glass_input(&self.name_input, window, cx).disabled(!owner),
            )
            .into_any_element(),
            cx,
        )
        .when(saving || dirty, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if saving { "Saving…" } else { "Unsaved" }),
            )
        });
        let mut general = section(cx)
            .child(crate::surface::glass_section_header("General", None, cx))
            .child(crate::surface::glass_group_rows(vec![name_row]));

        if let Some(error) = &self.error {
            general = general.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        let mut pane = v_flex().gap_4().child(general);

        // EXP-1025: the team prompt, right under the name (web parity).
        pane = pane.child(self.render_prompt_section(owner, cx));

        // EXP-288: read-only Plan & Billing between the name card and the
        // Danger Zone (fetch kicked at render like the boards repo cache).
        self.ensure_billing(&team.id, cx);
        if let Some(billing) = self.render_billing_section(&team, cx) {
            pane = pane.child(billing);
        }

        // Danger zone (web settings/general.tsx): owner-only. EXP-720: the
        // shared `danger_zone` recipe, same as Tools' "Reset IDE data".
        if owner {
            let team_id = team.id.clone();
            let team_name = team.name.clone();
            pane = pane.child(
                danger_zone(
                    "Permanently delete this team and all its data.",
                    Button::new("team-delete")
                        .danger()
                        .web_sm()
                        .label("Delete team")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_delete_dialog(
                                team_id.clone(),
                                team_name.clone(),
                                window,
                                cx,
                            );
                        })),
                    cx,
                )
                // Web parity (settings/general.tsx): a refused delete —
                // the REV2-55 billing gate above all — is shown, never
                // swallowed.
                .when_some(self.delete_error.clone(), |zone, message| {
                    zone.child(error_notice(message, cx))
                }),
            );
        }

        pane.when(!owner, |pane| {
            pane.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Only team owners can change these settings."),
            )
        })
    }
}
