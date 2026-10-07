//! The Prompt part of an action's page (EXP-467 as the "Edit action" dialog;
//! SLOP-2 folded it into [`crate::action_view`]) — the web `ActionPromptForm`
//! 1:1. Two columns like the web: metadata left (icon + Name / Description /
//! composer hint / Repository), the prompt right as a plain monospace
//! multiline editor (the web textarea — NOT the WYSIWYG), one batched
//! `actions.update` on Save. Writes are owner-only (the server enforces it),
//! so a NON-owner gets the same form read-only, with no Save.
//!
//! Synced rows carry no body (EXP-268), so the prompt is fetched via
//! `actions.get` when the form is built and the field stays disabled until it
//! lands — a save can never blank it. A duplicate-name CONFLICT (HTTP 409)
//! renders inline under the Name field, everything else beside Save. There is
//! deliberately NO inputs editor — the web form has none; input definitions
//! are authored by the "Create action" builtin run, and the batched update
//! omits `inputs` (and `triggers`, which the page's Triggers section owns),
//! leaving both untouched.
//!
//! EXP-694: the fields wear the inset-grouped stack every client's editors
//! wear ([`crate::surface::glass_group`]) — icon + Name are ONE row, the
//! description and the prompt are chrome-less textareas whose PLACEHOLDER is
//! their title (no label above), and the repository is a picker row.
//!
//! FEED-73: the action's MCP servers ride the same fetch/save path — every
//! run of a real action connects to THIS list (the server sets the run's
//! pick from it), and only TEAM MCPs ([`team_mcps`]: no sign-in, or a
//! member's shared connection) are offered. Ids the picker no longer lists
//! are kept, never dropped; the save sends the list only when it changed.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AppContext as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::controls::{glass_input, WebControl as _};
use crate::queries;

/// EXP-825: the composer-hint field's own placeholder — byte-identical to
/// the web form's (`action-prompt-form.tsx`).
const PROMPT_PLACEHOLDER_HINT: &str = "Composer hint, e.g. Scope: which platforms, which version";

/// The metadata column's width — the old dialog's left column.
const META_COLUMN_W: f32 = 400.;

pub(crate) struct ActionPromptForm {
    action_id: String,
    team_id: String,
    name: gpui::Entity<InputState>,
    /// EXP-530: a TEXTAREA like the web dialog's — action descriptions are
    /// the Suggestions-tab paragraphs, not one-liners.
    description: gpui::Entity<TextareaState>,
    /// EXP-825: the composer's field hint while this action is picked
    /// (`actions.prompt_placeholder`, ≤200 chars; "" clears). A single
    /// line like the web `Input`, capped at the server's limit.
    prompt_placeholder: gpui::Entity<InputState>,
    /// The curated registry glyph (`actionIconSchema` — the boards set).
    icon: String,
    /// `None` = repo-less scratch run (the web select's "None").
    repo_id: Option<String>,
    /// `repositories.list` rows for the picker; `None` = still loading.
    repos: Option<Vec<crate::action_run::ActionRepoRow>>,
    /// The prompt — a plain multiline editor (web textarea parity).
    body: gpui::Entity<TextareaState>,
    /// FEED-73: the team's MCP servers that may go on the action
    /// ([`team_mcps`]); empty hides the row (also while loading).
    team_mcps: Vec<api::mcp_servers::McpServerListEntry>,
    /// FEED-73: the action's MCP list (row ids) as edited, and as last
    /// loaded or saved — the save sends it only when the two differ.
    mcp_server_ids: Vec<String>,
    saved_mcp_server_ids: Vec<String>,
    /// `actions.get` in flight — the prompt and Save stay disabled so a
    /// save can never blank the body.
    body_loading: bool,
    submitting: bool,
    /// Duplicate-name CONFLICT, rendered inline under Name.
    name_error: Option<SharedString>,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl ActionPromptForm {
    /// The form over one synced (non-builtin) action row.
    pub(crate) fn new(
        action: api::actions::Action,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // EXP-694: the placeholders ARE the field titles — same three strings
        // on every client (web `action-editor-dialog.tsx`).
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        name.update(cx, |state, cx| {
            state.set_value(action.name.clone(), window, cx);
        });
        let description =
            cx.new(|cx| crate::controls::web_textarea(2, 8, window, cx).placeholder("Description"));
        description.update(cx, |state, cx| {
            state.set_value(action.description.clone().unwrap_or_default(), window, cx);
        });
        // EXP-825: the composer hint — the web field's placeholder and
        // `maxLength` (the server's `actionPromptPlaceholderSchema` cap), so
        // a save can never 400 on length.
        let prompt_placeholder = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(PROMPT_PLACEHOLDER_HINT)
                .validate(|text, _| {
                    text.chars().count() <= api::actions::MAX_PROMPT_PLACEHOLDER_CHARS
                })
        });
        prompt_placeholder.update(cx, |state, cx| {
            state.set_value(
                action.prompt_placeholder.clone().unwrap_or_default(),
                window,
                cx,
            );
        });
        // Swapped for "Prompt" the moment `actions.get` lands (web parity).
        // The prompt AUTO-GROWS: the page is the one scroller (SLOP-2), so
        // the field is as tall as its text, inside a sane band.
        let body = cx.new(|cx| {
            crate::controls::web_textarea(12, 40, window, cx).placeholder("Loading prompt…")
        });

        // Enter submits from the one-line fields; in the prompt it inserts a
        // newline (hence no shell-level `on_enter`).
        let mut subscriptions = Vec::new();
        for field in [&name, &prompt_placeholder] {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                },
            ));
        }
        // Save-gating (empty body) follows the editors live. Both are
        // textareas, so Enter inserts a newline instead of submitting.
        for field in [&description, &body] {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                |_, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                },
            ));
        }

        let mut this = Self {
            action_id: action.id.clone(),
            team_id: action.team_id.clone(),
            name,
            description,
            prompt_placeholder,
            icon: action
                .icon
                .clone()
                .unwrap_or_else(|| domain::contract::BOARD_ICON_VALUES[0].to_string()),
            repo_id: action.repository_id.clone(),
            repos: None,
            team_mcps: Vec::new(),
            mcp_server_ids: Vec::new(),
            saved_mcp_server_ids: Vec::new(),
            body,
            body_loading: true,
            submitting: false,
            name_error: None,
            error: None,
            _subscriptions: subscriptions,
        };
        this.fetch_body(window, cx);
        this.fetch_repos(window, cx);
        this.fetch_mcp_servers(window, cx);
        this
    }

    /// The ONLY body path (EXP-268): synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            self.body_loading = false;
            return;
        };
        let action_id = self.action_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::actions::get(&trpc, &action_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                match result {
                    Ok(action) => {
                        view.body.update(cx, |state, cx| {
                            state.set_value(action.body, window, cx);
                        });
                        view.mcp_server_ids = action.mcp_server_ids.clone();
                        view.saved_mcp_server_ids = action.mcp_server_ids;
                    }
                    Err(err) => view.error = Some(err.user_message().into()),
                }
                view.body.update(cx, |state, cx| {
                    state.set_placeholder("Prompt", window, cx);
                });
                view.body_loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn fetch_repos(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team_id = self.team_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { crate::action_run::fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = this.update_in(window, |view, _, cx| {
                match result {
                    Ok(rows) => view.repos = Some(rows),
                    // The picker degrades to the seeded choice — the save
                    // still round-trips it untouched.
                    Err(err) => log::warn!("actions: repositories.list failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// FEED-73: `mcpServers.list` (server-only, never synced), narrowed to
    /// the team MCPs an action may carry.
    fn fetch_mcp_servers(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team_id = self.team_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::mcp_servers::list(&trpc, &team_id) })
                .await;
            let _ = this.update_in(window, |view, _, cx| {
                match result {
                    Ok(entries) => view.team_mcps = team_mcps(&entries),
                    // No row then — the save leaves the list untouched.
                    Err(err) => log::warn!("actions: mcpServers.list failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ONE batched `actions.update` — the web dialog's submit. `inputs`
    /// stays omitted (untouched).
    /// Writes are owner-only: everyone else reads the form.
    fn read_only(&self, cx: &gpui::App) -> bool {
        !crate::settings::is_owner(cx, &self.team_id)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting || self.body_loading || self.read_only(cx) {
            return;
        }
        // A name is one logical line; pasted newlines collapse (EXP-230).
        let name = self
            .name
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        let body = self.body.read(cx).value().to_string();
        if name.is_empty() || body.trim().is_empty() {
            return;
        }
        // EXP-530: the description is a textarea now — newlines are the
        // author's, so only the outer whitespace goes.
        let description = self.description.read(cx).value().trim().to_string();
        // EXP-825: one logical line, outer whitespace off; "" clears (the
        // server nulls it — web `promptPlaceholder.trim() === "" ? null`).
        let prompt_placeholder: String = self
            .prompt_placeholder
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .chars()
            .take(api::actions::MAX_PROMPT_PLACEHOLDER_CHARS)
            .collect();
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.submitting = true;
        self.name_error = None;
        self.error = None;
        cx.notify();

        let mut input = api::actions::ActionUpdate::new(self.action_id.clone());
        input.name = Some(name);
        // "" clears — the existing desktop convention (the server nulls it).
        input.description = Some(description);
        input.prompt_placeholder = Some(prompt_placeholder);
        input.icon = Some(self.icon.clone());
        input.repository_id = api::Patch::set_or_null(self.repo_id.clone());
        input.body = Some(body);
        // FEED-73: only a changed list rides the save (web `mcpDirty`).
        let mcp_sent = mcp_ids_changed(&self.mcp_server_ids, &self.saved_mcp_server_ids)
            .then(|| self.mcp_server_ids.clone());
        input.mcp_server_ids = mcp_sent.clone();

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::actions::update(&trpc, &input).map(|_| ()) })
                .await;
            let _ = this.update_in(window, |view, _, cx| {
                view.submitting = false;
                match result {
                    // The synced echo repaints the header and the list —
                    // nothing to gate on; the form stays where it is.
                    Ok(()) => {
                        if let Some(sent) = mcp_sent {
                            view.saved_mcp_server_ids = sent;
                        }
                    }
                    Err(err) => {
                        match err {
                            // The (teamId, name) CONFLICT — inline, web parity.
                            api::ApiError::Http { status: 409, message } => {
                                view.name_error = Some(message.into());
                            }
                            other => view.error = Some(format!("{other}").into()),
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ActionPromptForm {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let read_only = self.read_only(cx);
        let name_empty = self.name.read(cx).value().trim().is_empty();
        let body_empty = self.body.read(cx).value().trim().is_empty();
        let disabled = name_empty || body_empty || self.body_loading || self.submitting;
        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;

        // -- left column: the metadata form ---------------------------------
        // EXP-642: the icon picker sits LEFT of the name input, one row
        // (create_board_dialog's `name_field` shape; web parity). A reader
        // sees the glyph, not a picker.
        let icon: gpui::AnyElement = if read_only {
            crate::icons::action_icon(Some(&self.icon))
                .text_color(muted)
                .into_any_element()
        } else {
            let icon_view = cx.entity().clone();
            crate::board_form::icon_picker(
                "action-edit",
                crate::icons::registry::PICKABLE_ICONS,
                Some(&self.icon),
                None,
                false,
                move |name, _, cx| {
                    let Some(name) = name else { return };
                    icon_view.update(cx, |this, cx| {
                        this.icon = name.to_string();
                        cx.notify();
                    });
                },
                cx,
            )
            .into_any_element()
        };
        // The glyph leads the row, the name types straight into it — the
        // group's fill and hairlines ARE the field chrome.
        let name_row = crate::surface::glass_row_shell()
            .child(icon)
            .child(
                div().flex_1().min_w_0().child(
                    glass_input(&self.name, window, cx)
                        .appearance(false)
                        .h_auto()
                        .px_0()
                        .py_0()
                        .disabled(read_only),
                ),
            );
        let description_row = div().w_full().child(
            Textarea::new(&self.description)
                .appearance(false)
                .w_full()
                .px_4()
                .py_3()
                .disabled(read_only),
        );
        // EXP-825: the composer hint — the third row of the metadata group,
        // its placeholder the title like the name's.
        let prompt_placeholder_row = crate::surface::glass_row_shell().child(
            div().flex_1().min_w_0().child(
                glass_input(&self.prompt_placeholder, window, cx)
                    .appearance(false)
                    .h_auto()
                    .px_0()
                    .py_0()
                    .disabled(read_only),
            ),
        );
        let mut form = v_flex()
            .gap_2()
            .child(crate::surface::glass_group_rows(vec![
                name_row,
                description_row,
                prompt_placeholder_row,
            ]));
        if let Some(name_error) = self.name_error.clone() {
            form = form.child(div().px_1().text_xs().text_color(danger).child(name_error));
        }
        let left = form
            .w(px(META_COLUMN_W))
            .flex_shrink_0()
            .child(crate::surface::glass_group_rows(vec![
                crate::surface::glass_picker_row(
                    "Repository",
                    None,
                    self.render_repo_picker(read_only, cx),
                    cx,
                ),
            ]))
            .child(div().px_1().text_xs().text_color(muted).child(
                "With a repository the run clones it first; without one the agent \
                 works in a scratch directory.",
            ))
            // FEED-73: the action's MCP servers — team MCPs only, hidden
            // when the team has none.
            .when(!self.team_mcps.is_empty(), |this| {
                let picker = self.render_mcp_picker(read_only, window, cx);
                this.child(crate::surface::glass_group_rows(vec![
                    crate::surface::glass_picker_row("MCP servers", None, picker, cx),
                ]))
            });

        // -- right column: the prompt ---------------------------------------
        // One group, one field: the placeholder is the title here too.
        let right = v_flex().flex_1().min_w_0().child(
            crate::surface::glass_group().child(
                Textarea::new(&self.body)
                    .appearance(false)
                    .w_full()
                    .px_4()
                    .py_3()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .disabled(self.body_loading || read_only),
            ),
        );

        // The one write of the section: owners only (a reader has no Save).
        let footer = (!read_only).then(|| {
            h_flex()
                .min_w_0()
                .justify_end()
                .items_center()
                .gap_3()
                .when_some(self.error.clone(), |this, error| {
                    this.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(danger)
                            .child(error),
                    )
                })
                .child(
                    Button::new("action-edit-save")
                        .primary()
                        .cursor_pointer()
                        .web_sm()
                        .label(if self.submitting {
                            "Saving…"
                        } else {
                            "Save changes"
                        })
                        .disabled(disabled)
                        .loading(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                )
        });

        v_flex()
            .min_w_0()
            .gap_3()
            .child(
                h_flex()
                    .min_w_0()
                    .items_start()
                    .gap_6()
                    .child(left)
                    .child(right),
            )
            // A reader still sees why the prompt did not load.
            .when(read_only, |this| {
                this.when_some(self.error.clone(), |this, error| {
                    this.child(div().text_sm().text_color(danger).child(error))
                })
            })
            .children(footer)
    }
}

impl ActionPromptForm {
    /// FEED-73 — the shared MCP multi picker over the team MCPs, dressed as
    /// the trailing value of a grouped picker row like the repository's.
    fn render_mcp_picker(
        &self,
        read_only: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let listed: Vec<String> =
            self.team_mcps.iter().map(|entry| entry.config.id.clone()).collect();
        let picked: Vec<String> = listed
            .iter()
            .filter(|id| self.mcp_server_ids.contains(id))
            .cloned()
            .collect();
        let options: Vec<crate::launch_options::McpServerOption> = self
            .team_mcps
            .iter()
            .map(|entry| crate::launch_options::McpServerOption {
                id: entry.config.id.clone(),
                name: entry.config.name.clone(),
                ..Default::default()
            })
            .collect();
        let label = crate::launch_options::mcp_pick_summary(&options, &picked);
        let servers: Vec<crate::picker::mcp_server_picker::McpPickerServer> = self
            .team_mcps
            .iter()
            .map(|entry| crate::picker::mcp_server_picker::McpPickerServer {
                id: entry.config.id.clone(),
                name: entry.config.name.clone(),
                url: entry.config.url.clone(),
                command: entry.config.command.clone(),
                description: None,
                disabled: false,
            })
            .collect();
        let trigger = Button::new("action-edit-mcp")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(cx.theme().foreground.opacity(0.7))
            .dropdown_caret(true)
            .child(crate::surface::picker_value_label(SharedString::from(label)))
            .into_any_element();
        let view = cx.entity().downgrade();
        crate::picker::mcp_server_picker::mcp_server_picker(
            &servers,
            picked,
            trigger,
            std::rc::Rc::new(move |next: Vec<String>, _window: &mut Window, cx: &mut gpui::App| {
                if let Some(view) = view.upgrade() {
                    let listed = listed.clone();
                    view.update(cx, |this, cx| {
                        this.mcp_server_ids = merge_mcp_pick(&this.mcp_server_ids, &listed, next);
                        cx.notify();
                    });
                }
            }),
        )
        .id("action-edit-mcp-picker")
        .disabled(read_only || self.body_loading)
        .render(window, cx)
    }

    /// The web repository `Select`: "None" + one entry per connected repo,
    /// dressed as the trailing VALUE of a grouped picker row (EXP-694) — no
    /// fill, no border, the caret the group's chevron.
    fn render_repo_picker(&self, read_only: bool, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let label: SharedString = match (&self.repos, &self.repo_id) {
            (None, _) => "Loading repositories…".into(),
            (Some(rows), Some(repo_id)) => rows
                .iter()
                .find(|row| &row.id == repo_id)
                .map(|row| SharedString::from(row.full_name.clone()))
                .unwrap_or_else(|| "Repository".into()),
            (Some(_), None) => "None".into(),
        };
        let trigger = Button::new("action-edit-repo")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(cx.theme().foreground.opacity(0.7))
            .dropdown_caret(true)
            // EXP-697: NOT `.label()` — upstream draws that in a `flex_none`
            // box, so a long `owner/repo` wraps onto a second line.
            .child(crate::surface::picker_value_label(label))
            .disabled(self.repos.is_none() || read_only);
        let Some(rows) = self.repos.clone() else {
            return trigger.into_any_element();
        };
        let view = cx.entity().downgrade();
        trigger
            .dropdown_menu(move |mut menu, _window, _cx| {
                let none_view = view.clone();
                menu = menu.item(PopupMenuItem::new("None").on_click(move |_, _, cx| {
                    if let Some(view) = none_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.repo_id = None;
                            cx.notify();
                        });
                    }
                }));
                for repo in &rows {
                    let view = view.clone();
                    let repo_id = repo.id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(repo.full_name.clone())).on_click(
                            move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    let repo_id = repo_id.clone();
                                    view.update(cx, |this, cx| {
                                        this.repo_id = Some(repo_id);
                                        cx.notify();
                                    });
                                }
                            },
                        ),
                    );
                }
                menu
            })
            .into_any_element()
    }
}

/// FEED-73 — a "team MCP" (web `isTeamMcp`): one any run of the team can
/// use whoever starts it — nothing to sign in to, or at least one member
/// SHARES their connection. Only these go on an action's MCP list.
pub(crate) fn is_team_mcp(entry: &api::mcp_servers::McpServerListEntry) -> bool {
    entry.config.auth == "none" || entry.shared_count > 0
}

/// The team MCPs of a `mcpServers.list`, in registry order.
pub(crate) fn team_mcps(
    entries: &[api::mcp_servers::McpServerListEntry],
) -> Vec<api::mcp_servers::McpServerListEntry> {
    entries.iter().filter(|entry| is_team_mcp(entry)).cloned().collect()
}

/// The picker's new set folded into the action's list: ids the picker does
/// not list (a server no longer shared) ride along, the listed ones are the
/// picker's (web `[...ids.filter(unlisted), ...next]`).
fn merge_mcp_pick(current: &[String], listed: &[String], next: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> =
        current.iter().filter(|id| !listed.contains(id)).cloned().collect();
    merged.extend(next);
    merged
}

/// Whether the list differs from the saved one as a SET (web `mcpDirty`).
fn mcp_ids_changed(current: &[String], saved: &[String]) -> bool {
    current.len() != saved.len() || current.iter().any(|id| !saved.contains(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::mcp_servers::{McpServerConfig, McpServerListEntry};

    fn server(id: &str, auth: &str, shared_count: u32) -> McpServerListEntry {
        McpServerListEntry {
            config: McpServerConfig {
                id: id.into(),
                name: id.into(),
                auth: auth.into(),
                ..Default::default()
            },
            shared_count,
            ..Default::default()
        }
    }

    #[test]
    fn team_mcps_keep_no_sign_in_and_shared_servers() {
        let entries = vec![
            server("docs", "none", 0),
            server("linear", "oauth", 0),
            server("sentry", "oauth", 2),
            server("stripe", "secret", 1),
            server("github", "secret", 0),
        ];
        let ids: Vec<String> = team_mcps(&entries).into_iter().map(|e| e.config.id).collect();
        assert_eq!(ids, vec!["docs", "sentry", "stripe"]);
    }

    #[test]
    fn a_pick_keeps_ids_the_picker_does_not_list() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            merge_mcp_pick(&ids(&["gone", "a"]), &ids(&["a", "b"]), ids(&["b"])),
            ids(&["gone", "b"])
        );
        assert!(!mcp_ids_changed(&ids(&["a", "b"]), &ids(&["b", "a"])));
        assert!(mcp_ids_changed(&ids(&["a"]), &ids(&["a", "b"])));
        assert!(mcp_ids_changed(&ids(&["a", "c"]), &ids(&["a", "b"])));
    }
}
