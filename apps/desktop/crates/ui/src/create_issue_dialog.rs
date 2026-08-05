//! Create-issue dialog (masterplan-v3 §4.2 — layout matches
//! `apps/web/src/components/create-issue-dialog.tsx` +
//! `issue-editor/dialog-shell.tsx` field-for-field).
//!
//! Structure (the web desktop branch shape: pinned header/footer,
//! scrollable body — never the old all-scrolling dialog):
//!
//! - titlebar strip: board pill (color dot + prefix) · `›` · "New issue"
//!   (EXP-287 — the window's own chrome carries it; see `native_dialog`)
//! - borderless title `Input` (text-lg, web `border-none focus-visible:ring-0`)
//! - the §4.5 [`crate::markdown::MarkdownEditor`] in a `flex_1` scroll region
//!   — clipboard-image paste stages `draft://` blocks; submit
//!   mirrors the web flow: create with the drafts **stripped**, upload each
//!   staged image, rewrite the URLs and `issues.update` the final description
//! - chip row: status / priority / assignee / labels / due-date (date only —
//!   REV2-49 deleted the time-of-day fields, §4.2)
//! - footer: "Create more" `Switch` (web uses a Switch, not a checkbox —
//!   L488-494) + the indigo submit button.
//!
//! Submit (§4.1): `issues.create` on a background thread; when "Create more"
//! is off the close+navigate is **gated** on the created row becoming visible
//! in the synced `issues` collection (the desktop's awaitTxId analog); when
//! on, fields reset immediately (web parity) and the Electric echo fills the
//! board.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, AnyElement, App, AppContext as _, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    calendar::{CalendarEvent, CalendarState, Date},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::DropdownMenu as _,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use domain::rows::{Label, User};
use domain::{IssuePriority, IssueStatus};

use crate::actions::NewIssue;
use crate::attachments_row;
use crate::icons::{option_icon, registry, ExpIcon};
use crate::markdown::image_paste::strip_draft_images;
use crate::markdown::{self};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::pickers::chip_button;
use crate::wysiwyg::WysiwygDescription;
use crate::navigation::{active_board_id, nav_for_window, navigate_from, Screen, TabOrigin};
use crate::queries;

/// Register the App-global [`NewIssue`] handler (call once from `ui::init`).
/// The action is the §3.6 unit action the filter bar dispatches; the target
/// board is the window's active board (the top-bar picker scope — the
/// All Issues tool window's list).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &NewIssue, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = nav_for_window(window, cx);
            let Some(board_id) = active_board_id(&nav, cx) else {
                return; // no board in scope — nothing to create into
            };
            open(window, cx, board_id);
        });
    });
}

/// Open the dialog. Resolves the board row (prefix, color, team) off
/// the synced collections; a no-op when the board is unknown (racing a
/// delete).
pub fn open(window: &mut Window, cx: &mut App, board_id: String) {
    let collections = Store::global(cx).collections();
    let Some(board) = collections.boards.read(cx).get(&board_id).cloned() else {
        log::warn!("[ui] NewIssue for unknown board {board_id}");
        return;
    };

    // Web: sm:max-w-[40rem] p-0 max-h-[85vh]. EXP-288: the dialog OPENS
    // compact (~3 description rows) and GROWS with the content up to the
    // pre-EXP-288 height (460 / 85% viewport — computed from the OPENER's
    // viewport now, the dialog can't read it later); still user-resizable
    // with a floor matching the compact start. Past the cap the editor
    // region scrolls with caret-follow, header/chips/footer pinned.
    let max_height = (window.viewport_size().height * 0.85).min(px(460.));
    let height = px(300.).min(max_height);
    let spec = DialogSpec::new("New issue", size(px(640.), height))
        .resizable(size(px(560.), px(300.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let bar_prefix = board.prefix.clone().unwrap_or_default();
        let bar_color = board.color.clone();
        let view = cx.new(|cx| {
            CreateIssueDialogView::new(board.id.clone(), board.team_id.clone(), max_height, window, cx)
        });
        let busy = view.clone();
        let submit = view.clone();
        DialogContent::new(view)
            .padless()
            // EXP-287: the board-pill breadcrumb rides the window's titlebar
            // strip (the shell owns the chrome now — see `native_dialog`).
            .title_content(move |_, cx| title_breadcrumb(&bar_prefix, bar_color.as_deref(), cx))
            .can_close(move |cx| !busy.read(cx).submitting)
            // Enter anywhere in the dialog submits (web form submit).
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

/// EXP-287: the titlebar label — board pill · › · "New issue". Lives in the
/// window's `TitleBar` strip now that the shell owns the dialog chrome; the
/// pill markup is unchanged from the header row it replaced.
fn title_breadcrumb(prefix: &str, color: Option<&str>, cx: &App) -> AnyElement {
    let pill_color = color
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    h_flex()
        .gap_1p5()
        .items_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(
            h_flex()
                .gap_1p5()
                .items_center()
                .rounded(cx.theme().radius)
                .bg(cx.theme().accent.opacity(0.5))
                .px_2()
                .py_0p5()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(div().size_2p5().rounded_full().bg(pill_color))
                .child(SharedString::from(prefix.to_string())),
        )
        .child(Icon::new(registry::UI_CHEVRON_RIGHT).xsmall())
        .child("New issue")
        .into_any_element()
}

/// EXP-335: one NON-image file queued via the toolbar's attach button — web
/// `draftFiles` parity. Bytes are read (and size-capped) at pick time, then
/// uploaded to the created issue right after `issues.create`.
struct StagedDraftFile {
    /// Process-local chip key (element ids + removal).
    key: u64,
    filename: String,
    content_type: String,
    bytes: std::sync::Arc<Vec<u8>>,
}

pub struct CreateIssueDialogView {
    board_id: String,
    team_id: String,

    title: Entity<InputState>,
    /// The §4.5 block editor in create-dialog (staging) mode: pasted images
    /// stay `draft://` blocks until submit resolves them.
    description: Entity<WysiwygDescription>,
    /// EXP-314: the picked status as a wire-ready pick (a synced row id, or
    /// the enum anchor of a constructed `builtin:<key>` fallback).
    status: crate::pickers::StatusPick,
    /// The team's backlog builtin, re-applied by "Create more" resets.
    default_status: crate::pickers::StatusPick,
    priority: IssuePriority,
    /// EXP-50: `Some(member)` when the team has exactly one human
    /// member at dialog open — the assignee chip hides and `assignee_id`
    /// defaults (and resets) to that member so the created issue is
    /// optimistically correct.
    solo_member_id: Option<String>,
    assignee_id: Option<String>,
    selected_label_ids: Vec<String>,
    /// EXP-288: the shared label picker's search input (host-owned).
    label_query: Entity<InputState>,
    due_date: Option<chrono::NaiveDate>,
    due_calendar: Entity<CalendarState>,
    /// EXP-335: non-image files queued for the post-create upload.
    staged_files: Vec<StagedDraftFile>,
    next_staged_file_key: u64,
    create_more: bool,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    /// EXP-288: the description scroll container's tracked handle — handed
    /// to the vendored editor for caret-follow, and read in render as the
    /// content-overflow sensor for the grow-while-typing window resize.
    desc_scroll: gpui::ScrollHandle,
    /// The height cap the dialog grows toward (the pre-EXP-288 fixed open
    /// height, computed from the OPENER's viewport at open time).
    max_height: gpui::Pixels,
    /// Grow-only bookkeeping: the last height this dialog requested via
    /// `window.resize`. An observed height meaningfully BELOW it means the
    /// user resized by hand — auto-grow then stops fighting them.
    last_requested_height: Option<gpui::Pixels>,
    user_resized: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateIssueDialogView {
    fn new(
        board_id: String,
        team_id: String,
        max_height: gpui::Pixels,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Issue title"));
        // Shared configured-editor constructor (§4.5): completion + pills
        // scoped to this team, upload staged (`upload_issue = None`).
        let description = crate::description_editor::build_wysiwyg_editor(
            Some(team_id.clone()),
            None,
            "Add description...",
            "",
            None,
            window,
            cx,
        );
        // EXP-285: the dialog pins the formatting bar ABOVE its scroll
        // region (a long description must not scroll the toolbar away).
        description.update(cx, |description, _| description.use_external_toolbar());
        // EXP-335: the toolbar's attach button — image picks embed inline via
        // the editor itself; non-image picks land here and queue for the
        // post-create upload (web draftFiles parity).
        {
            let dialog = cx.entity().downgrade();
            description.update(cx, |description, _| {
                description.set_attach_handler(Rc::new(move |paths, window, cx| {
                    let Some(dialog) = dialog.upgrade() else {
                        return;
                    };
                    dialog.update(cx, |this, cx| this.stage_files(paths, window, cx));
                }));
            });
        }
        // EXP-288: hand the scroll container's handle to the editor so the
        // caret stays visible while typing/pasting ("we always wanna see
        // what we type").
        let desc_scroll = gpui::ScrollHandle::new();
        description.update(cx, |description, cx| {
            description.set_scroll_handle(desc_scroll.clone(), cx);
        });
        let due_calendar = cx.new(|cx| CalendarState::new(window, cx));
        let label_query = cx.new(|cx| InputState::new(window, cx).placeholder("Filter labels…"));

        let mut subscriptions = Vec::new();
        // Enter in the (single-line) title submits, like the web form.
        subscriptions.push(cx.subscribe_in(
            &title,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(window, cx);
                }
            },
        ));
        // Title emptiness drives the submit button's disabled state.
        subscriptions.push(cx.subscribe(&title, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        }));
        // Due-date picks mirror into our state (web `onDueDateSelect`).
        subscriptions.push(cx.subscribe(
            &due_calendar,
            |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(date)) = event else {
                    return;
                };
                this.due_date = *date;
                cx.notify();
            },
        ));
        // The footer attachment rail mirrors the live description (web
        // `imageOccurrences` over the current markdown) — re-render on every
        // editor change (pastes, deletions, chip removals).
        subscriptions.push(cx.observe(&description, |_, _, cx| cx.notify()));

        // EXP-50: exactly one human member ⇒ no assignment choice — hide the
        // chip and pre-assign them (0 members = membership not synced yet,
        // keep the picker).
        let members = queries::team_users(cx, &team_id);
        let solo_member_id = match members.as_slice() {
            [only] => Some(only.id.clone()),
            _ => None,
        };

        // EXP-314: the default is the team's BACKLOG BUILTIN row (web parity:
        // new issues start in backlog). Before the statuses shape syncs this
        // is the constructed `builtin:backlog` fallback, which writes the enum
        // anchor instead of a `statusId`.
        let default_status = crate::pickers::StatusPick::from_resolved(
            &queries::team_status_options(cx, &team_id)
                .into_iter()
                .find(|status| status.builtin_key.as_deref() == Some("backlog"))
                .unwrap_or_else(|| {
                    domain::statuses::constructed_default(IssueStatus::Backlog)
                }),
        );

        Self {
            board_id,
            team_id,
            title,
            description,
            status: default_status.clone(),
            default_status,
            priority: IssuePriority::None,
            assignee_id: solo_member_id.clone(),
            solo_member_id,
            selected_label_ids: Vec::new(),
            label_query,
            due_date: None,
            due_calendar,
            staged_files: Vec::new(),
            next_staged_file_key: 0,
            create_more: false,
            submitting: false,
            error: None,
            focused_once: false,
            desc_scroll,
            max_height,
            last_requested_height: None,
            user_resized: false,
            _subscriptions: subscriptions,
        }
    }

    /// EXP-288: grow the dialog window with the description content, up to
    /// [`Self::max_height`] — the dialog opens compact and expands while
    /// typing/pasting instead of starting tall. Grow-only (never shrinks on
    /// deletion), and a manual shrink by the user latches auto-grow off.
    /// The overflow sensor is the description scroll container's last-frame
    /// layout, so this runs at render time with a one-frame lag.
    fn grow_with_content(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.user_resized {
            return;
        }
        let current = window.viewport_size();
        if let Some(requested) = self.last_requested_height {
            if current.height < requested - px(2.) {
                // The user dragged the window smaller than we grew it —
                // stop fighting them for this dialog's lifetime.
                self.user_resized = true;
                return;
            }
        }
        let overflow = self.desc_scroll.max_offset().y;
        if overflow <= px(1.) || current.height >= self.max_height - px(1.) {
            return;
        }
        let target = (current.height + overflow).min(self.max_height);
        if target <= current.height + px(1.) {
            return;
        }
        self.last_requested_height = Some(target);
        let new_size = gpui::size(current.width, target);
        // Deferred: resizing mid-render would re-enter the platform path
        // (the window_size.rs EXP-263 precedent).
        window.defer(cx, move |window, _cx| window.resize(new_size));
    }

    /// EXP-335: read picked non-image files off the foreground (read_any_file
    /// enforces the 50 MB cap) and queue them for the post-create upload; an
    /// unreadable/oversize pick surfaces in the footer's error slot.
    fn stage_files(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        for path in paths {
            cx.spawn_in(window, async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { crate::markdown::read_any_file(&path) })
                    .await;
                this.update_in(cx, |this, _window, cx| {
                    match result {
                        Ok((filename, content_type, bytes)) => {
                            let key = this.next_staged_file_key;
                            this.next_staged_file_key += 1;
                            this.staged_files.push(StagedDraftFile {
                                key,
                                filename,
                                content_type,
                                bytes: std::sync::Arc::new(bytes),
                            });
                        }
                        Err(error) => {
                            this.error = Some(format!("{error}").into());
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Web `resetFields`: clear everything except "Create more".
    fn reset_fields(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.title.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.description.update(cx, |editor, cx| {
            editor.set_markdown("", window, cx);
        });
        self.status = self.default_status.clone();
        self.priority = IssuePriority::None;
        // EXP-50: the solo-member default survives "Create more" resets.
        self.assignee_id = self.solo_member_id.clone();
        self.selected_label_ids.clear();
        self.label_query
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.due_date = None;
        self.due_calendar.update(cx, |state, cx| {
            state.set_date(Date::Single(None), window, cx);
        });
        self.staged_files.clear();
        self.error = None;
        self.submitting = false;
        self.title.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty() || self.submitting {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.submitting = true;
        cx.notify();

        // Build the exact web mutation input (`create-issue-dialog.tsx`
        // handleSubmit).
        let mut input = api::issues::IssuesCreateInput::new(self.board_id.clone(), title);
        self.status.apply_to_create(&mut input);
        input.priority = Some(self.priority);
        input.assignee_id = self.assignee_id.clone();
        // Web submit flow (create-issue-dialog.tsx): create with the staged
        // `draft://` images STRIPPED, upload them post-create, then update
        // the description with the canonical attachment URLs.
        let markdown = self.description.read(cx).markdown(cx);
        let staged = self.description.read(cx).staged_images(cx);
        let stripped_description = strip_draft_images(&markdown);
        if !stripped_description.is_empty() {
            input.description = Some(stripped_description.clone());
        }
        let transport = queries::attachment_transport(cx);
        // EXP-335: queued non-image draft files ride the same post-create
        // window (cheap Arc clones — the bytes are shared, not copied).
        let staged_files: Vec<(String, String, std::sync::Arc<Vec<u8>>)> = self
            .staged_files
            .iter()
            .map(|file| {
                (
                    file.filename.clone(),
                    file.content_type.clone(),
                    file.bytes.clone(),
                )
            })
            .collect();
        let transport_files = transport.clone();
        // `TrpcClient` is not `Clone` — a second one for the post-create
        // description update (cheap: an `Agent` + two `Arc`s, §5.7).
        let trpc_update = queries::trpc_client(cx);
        input.due_date = self.due_date.map(|date| date.format("%Y-%m-%d").to_string());
        if !self.selected_label_ids.is_empty() {
            input.label_ids = Some(self.selected_label_ids.clone());
        }

        let create_more = self.create_more;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::issues::issues_create(&trpc, &input) })
                .await;

            match result {
                Ok(output) => {
                    let issue_id = output.issue.id.clone();

                    // Post-create image resolution (web parity: per-image
                    // failures are tolerated — failed drafts drop out of the
                    // final description).
                    if let (false, Some(transport), Some(trpc_update)) =
                        (staged.is_empty(), transport, trpc_update)
                    {
                        let upload_issue = issue_id.clone();
                        let full_markdown = markdown.clone();
                        let stripped = stripped_description.clone();
                        let final_description = window
                            .background_executor()
                            .spawn(async move {
                                let mut resolved = std::collections::HashMap::new();
                                for image in &staged {
                                    match transport.upload(
                                        &upload_issue,
                                        &image.filename,
                                        &image.content_type,
                                        &image.bytes,
                                    ) {
                                        Ok(uploaded) => {
                                            resolved.insert(
                                                image.draft_url.clone(),
                                                uploaded.url,
                                            );
                                        }
                                        Err(err) => {
                                            log::warn!(
                                                "[ui] create-dialog image upload failed: {err}"
                                            );
                                        }
                                    }
                                }
                                // Rewrite the uploads in, drop the failures.
                                let rewritten = markdown::image_paste::rewrite_image_urls(
                                    &full_markdown,
                                    &resolved,
                                );
                                let final_description = strip_draft_images(&rewritten);
                                if final_description == stripped {
                                    return None; // nothing survived — no update
                                }
                                let mut update = api::issues::IssuesUpdateInput::new(
                                    upload_issue.clone(),
                                );
                                update.description = if final_description.is_empty() {
                                    api::Patch::Null
                                } else {
                                    api::Patch::Set(final_description.clone())
                                };
                                if let Err(err) =
                                    api::issues::issues_update(&trpc_update, &update)
                                {
                                    log::warn!(
                                        "[ui] create-dialog description update failed: {err}"
                                    );
                                }
                                Some(final_description)
                            })
                            .await;
                        let _ = final_description; // board renders off the echo
                    }
                    // EXP-335: upload the queued non-image files (web
                    // draftFiles parity — per-file failures are logged and
                    // tolerated, they never block the created issue).
                    if let (false, Some(transport)) =
                        (staged_files.is_empty(), transport_files)
                    {
                        let upload_issue = issue_id.clone();
                        window
                            .background_executor()
                            .spawn(async move {
                                for (filename, content_type, bytes) in &staged_files {
                                    if let Err(err) = transport.upload_file(
                                        &upload_issue,
                                        filename,
                                        content_type,
                                        bytes,
                                    ) {
                                        log::warn!(
                                            "[ui] create-dialog file upload failed: {err}"
                                        );
                                    }
                                }
                            })
                            .await;
                    }
                    if create_more {
                        // Web parity: reset immediately, keep the dialog open,
                        // let the Electric echo fill the board.
                        let _ = this.update_in(window, |this, window, cx| {
                            this.reset_fields(window, cx);
                        });
                        return;
                    }
                    // Gated path (§4.1): close + navigate only once the row
                    // is visible in the synced collection.
                    let issues = window
                        .update(|_, cx| Store::global(cx).collections().issues.clone())
                        .ok();
                    if let Some(issues) = issues {
                        queries::await_row_visible(&issues, &issue_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        // EXP-288: the rail may point anywhere while the
                        // dialog is up — the new tab's origin is the ISSUE's
                        // board, explicitly.
                        let origin = TabOrigin {
                            tool: crate::sidebar::ToolWindow::BoardIssues,
                            board_id: Some(this.board_id.clone()),
                        };
                        native_dialog::close_then(window, cx, move |window, cx| {
                            navigate_from(window, cx, Screen::IssueDetail { issue_id }, origin);
                        });
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    // -- chips (web `issue-editor/chips.tsx`) --------------------------------

    fn status_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-314: the chip renders the picked TEAM status; the menu lists the
        // team's own vocabulary (constructed defaults before the shape syncs).
        let statuses = crate::queries::team_status_options(cx, &self.team_id);
        let current = self.status.clone();
        let resolved = statuses
            .iter()
            .find(|status| {
                status.row_id.is_some() && status.row_id == current.status_id
                    || (current.status_id.is_none() && status.anchor() == current.anchor)
            })
            .cloned()
            .unwrap_or_else(|| domain::statuses::constructed_default(current.anchor));
        let current_key = resolved.group_key.clone();
        let view = cx.entity().clone();
        chip_button("create-status-chip", cx)
            .icon(crate::icons::resolved_status_icon(&resolved, cx))
            .label(SharedString::from(resolved.name.clone()))
            .dropdown_menu(move |menu, _window, cx| {
                let view = view.clone();
                let statuses = crate::queries::team_status_options(cx, &view.read(cx).team_id);
                crate::pickers::status_menu(
                    menu,
                    &statuses,
                    &current_key,
                    // A brand-new issue can't be a duplicate of anything yet —
                    // no duplicate row here (web `creatableStatusOptions`).
                    crate::pickers::StatusMenuScope::Assignable,
                    Rc::new(move |pick, _window, cx| {
                        view.update(cx, |this, cx| {
                            this.status = pick;
                            cx.notify();
                        });
                    }),
                    cx,
                )
            })
    }

    fn priority_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let config = domain::options::get_issue_priority_config(self.priority);
        let current = self.priority;
        let view = cx.entity().clone();
        chip_button("create-priority-chip", cx)
            .icon(option_icon(config, cx))
            .label(SharedString::from(config.label))
            .dropdown_menu(move |menu, _window, cx| {
                let view = view.clone();
                crate::pickers::priority_menu(
                    menu,
                    current,
                    Rc::new(move |value, _window, cx| {
                        view.update(cx, |this, cx| {
                            this.priority = value;
                            cx.notify();
                        });
                    }),
                    cx,
                )
            })
    }

    /// Web `AssigneePicker`: "Assignee" or the selected member's name;
    /// options = team members + Unassign.
    fn assignee_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let users = queries::team_users(cx, &self.team_id);
        let selected = self
            .assignee_id
            .as_deref()
            .and_then(|id| users.iter().find(|user| user.id == id));
        let label: SharedString = selected
            .map(|user| SharedString::from(display_name(user)))
            .unwrap_or_else(|| "Assignee".into());
        let current = self.assignee_id.clone();
        let view = cx.entity().clone();

        chip_button("create-assignee-chip", cx)
            .icon(
                Icon::new(registry::UI_ASSIGNEE)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .label(label)
            .dropdown_menu(move |menu, _window, _cx| {
                let view = view.clone();
                crate::pickers::assignee_menu(
                    menu,
                    &users,
                    current.as_deref(),
                    Rc::new(move |picked, _window, cx| {
                        view.update(cx, |this, cx| {
                            this.assignee_id = picked;
                            cx.notify();
                        });
                    }),
                )
            })
    }

    /// Web `LabelPicker` trigger: "Label" or the joined selected names.
    /// EXP-288: the shared SEARCHABLE multi-toggle popover (the properties
    /// panel recipe) — toggles no longer close/reopen the picker per pick.
    fn labels_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let labels = queries::team_labels(cx, &self.team_id);
        let selected: Vec<&Label> = labels
            .iter()
            .filter(|label| self.selected_label_ids.contains(&label.id))
            .collect();
        let label: SharedString = if selected.is_empty() {
            "Label".into()
        } else {
            selected
                .iter()
                .map(|label| label.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
                .into()
        };
        let view = cx.entity().clone();

        let trigger = chip_button("create-labels-chip", cx)
            .icon(
                Icon::from(ExpIcon::Tag)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .label(label);
        crate::pickers::label_picker_popover(
            "create-labels-popover",
            trigger,
            crate::pickers::LabelPickerParams {
                labels,
                selected_ids: self.selected_label_ids.clone(),
                query: self.label_query.clone(),
                on_toggle: Rc::new(move |label_id, was_selected, _window, cx| {
                    let label_id = label_id.to_string();
                    view.update(cx, |this, cx| {
                        if was_selected {
                            this.selected_label_ids.retain(|existing| existing != &label_id);
                        } else {
                            this.selected_label_ids.push(label_id);
                        }
                        cx.notify();
                    });
                }),
                width: Some(px(crate::pickers::PICKER_SEARCH_WIDTH)),
            },
        )
    }

    /// Web due chip: `CalendarDays` + "Jul 3" or "Due date"; the popover hosts
    /// the calendar (date only — REV2-49 deleted the time-of-day fields, §4.2).
    fn due_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let label: SharedString = match self.due_date {
            Some(date) => {
                domain::board::format_short_date(&date.format("%Y-%m-%d").to_string()).into()
            }
            None => "Due date".into(),
        };

        crate::pickers::due_date_popover(
            "create-due-popover",
            chip_button("create-due-chip", cx)
                .icon(
                    Icon::from(ExpIcon::CalendarDays)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .label(label),
            self.due_calendar.clone(),
            Some(px(280.)),
            None,
        )
    }

    // -- footer ----------------------------------------------------------------

    /// Web `IssueEditorAttachmentRail` (the dialog footer's left slot): one
    /// chip per image occurrence in the live description + the trailing
    /// "N images" count (always shown — "0 images" included); each chip
    /// carries the web's remove ✕, dropping that occurrence from the
    /// markdown (`removeMarkdownImageByOccurrence`).
    fn attachment_rail(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let markdown = self.description.read(cx).markdown(cx);
        let occurrences = attachments_row::extract_image_occurrences(&markdown);
        let count = occurrences.len();
        let editor = self.description.clone();
        let removable = !self.submitting;

        h_flex()
            .min_w_0()
            .flex_1()
            .gap_2()
            .items_center()
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1p5()
                    .overflow_hidden()
                    .children(occurrences.iter().enumerate().map(|(ix, occurrence)| {
                        let remove: Option<attachments_row::ChipRemove> =
                            removable.then(|| {
                                let editor = editor.clone();
                                let markdown = markdown.clone();
                                let on_click = Box::new(
                                    move |_: &gpui::ClickEvent,
                                          window: &mut Window,
                                          cx: &mut App| {
                                        let next = attachments_row::remove_image_occurrence(
                                            &markdown, ix,
                                        );
                                        editor.update(cx, |editor, cx| {
                                            editor.set_markdown(&next, window, cx);
                                        });
                                    },
                                )
                                    as Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
                                (
                                    SharedString::from(format!("create-attachment-remove-{ix}")),
                                    on_click,
                                )
                            });
                        attachments_row::image_chip(
                            gpui::ElementId::from(("create-attachment-chip", ix)),
                            attachments_row::occurrence_label(occurrence, ix),
                            &occurrence.url,
                            remove,
                            cx,
                        )
                    }))
                    // EXP-335: queued non-image draft files, one chip each
                    // (web `issue-attachment-file-chip-*` parity).
                    .children(self.staged_files.iter().map(|file| {
                        let remove: Option<attachments_row::ChipRemove> =
                            removable.then(|| {
                                let view = cx.entity().clone();
                                let key = file.key;
                                let on_click = Box::new(
                                    move |_: &gpui::ClickEvent,
                                          _window: &mut Window,
                                          cx: &mut App| {
                                        view.update(cx, |this, cx| {
                                            this.staged_files
                                                .retain(|staged| staged.key != key);
                                            cx.notify();
                                        });
                                    },
                                )
                                    as Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
                                (
                                    SharedString::from(format!(
                                        "create-attachment-file-remove-{}",
                                        file.key
                                    )),
                                    on_click,
                                )
                            });
                        attachments_row::file_chip(
                            gpui::ElementId::from((
                                "create-attachment-file-chip",
                                file.key as usize,
                            )),
                            file.filename.clone(),
                            Some(file.content_type.as_str()),
                            file.bytes.len() as i64,
                            remove,
                            cx,
                        )
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(
                        attachments_row::attachment_count_label(
                            count,
                            self.staged_files.len(),
                        ),
                    )),
            )
    }

    fn footer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let title_empty = self.title.read(cx).value().trim().is_empty();
        let submit_disabled = title_empty || self.submitting;
        let submit_label: &'static str = if self.submitting {
            "Creating..."
        } else {
            "Create issue"
        };
        let view = cx.entity().clone();

        let right = h_flex()
            .gap_3()
            .items_center()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Switch::new("create-more")
                            .small()
                            .checked(self.create_more)
                            .disabled(self.submitting)
                            .on_click({
                                let view = view.clone();
                                move |checked: &bool, _, cx| {
                                    let checked = *checked;
                                    view.update(cx, |this, cx| {
                                        this.create_more = checked;
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Create more"),
                    ),
            )
            .child(
                // Web submit: bg-indigo-600 hover:bg-indigo-700 text-white
                // h-7 text-xs — SOLID indigo (label swaps while creating, no
                // spinner, web parity).
                indigo_button("create-issue-submit", submit_disabled, cx)
                    .child(SharedString::from(submit_label))
                    .when(!submit_disabled, |button| {
                        button.on_click(cx.listener(|this, _, window, cx| {
                            this.submit(window, cx)
                        }))
                    }),
            );

        let left: gpui::AnyElement = match &self.error {
            Some(error) => div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(error.clone())
                .into_any_element(),
            // Web `IssueEditorAttachmentRail` — always rendered ("0 images").
            None => self.attachment_rail(cx).into_any_element(),
        };

        h_flex()
            .px_4()
            .py_3()
            .gap_3()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().min_w_0().flex_1().child(left))
            .child(right)
    }
}

impl Render for CreateIssueDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Web `autoFocus` on the title input (once, after the dialog mounts).
        if !self.focused_once {
            self.focused_once = true;
            self.title.update(cx, |state, cx| state.focus(window, cx));
        }
        // EXP-288: expand the window with the description content (up to
        // the cap) before the caret-follow scrolling takes over.
        self.grow_with_content(window, cx);

        // Chip row (web px-4 py-2 border-t): status · priority · assignee ·
        // labels · due.
        let chips = h_flex()
            .px_4()
            .py_2()
            .gap_1()
            .items_center()
            .flex_wrap()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(self.status_chip(cx))
            .child(self.priority_chip(cx))
            // EXP-50: single-member teams have no assignment choice —
            // the chip hides and the solo member is pre-assigned.
            .when(self.solo_member_id.is_none(), |this| {
                this.child(self.assignee_chip(cx))
            })
            .child(self.labels_chip(cx))
            .child(self.due_chip(cx));

        v_flex()
            .size_full()
            .child(
                // Borderless title (web text-lg font-medium px-5).
                //
                // TAB jumps straight into the description editor (EXP-68) —
                // without this the single-line input propagates the `tab →
                // IndentInline` binding and Root's fallback `tab → Tab`
                // cycles focus through the description toolbar's formatting
                // buttons first. Handling IndentInline here (the bubble
                // reaches this wrapper right after the input propagates it)
                // consumes the keystroke before Root's binding runs.
                div()
                    .px_3()
                    // EXP-287: the deleted header row's `pb_2` used to supply
                    // this gap under the chrome.
                    .pt_2()
                    .on_action(cx.listener(
                        |this, _: &gpui_component::input::IndentInline, window, cx| {
                            this.description
                                .update(cx, |editor, cx| editor.focus(window, cx));
                        },
                    ))
                    .child(
                        Input::new(&self.title)
                            .appearance(false)
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM),
                    ),
            )
            .child(
                // EXP-285: the formatting bar sits ABOVE the scroll region so
                // a long description never scrolls it away. Same px_3 as the
                // slot: 12 + the toolbar's own glyph-align inset lands the
                // first glyph on the shared 24px text edge.
                div().px_3().children(
                    self.description
                        .update(cx, |description, cx| description.toolbar_row(window, cx)),
                ),
            )
            .child(
                // Only this region scrolls; header/chips/footer pinned.
                // The 96px floor is ~3 text rows (EXP-288 — the compact
                // dialog opens with a real textarea, not a single line).
                // EXP-288: `track_scroll` powers the editor's caret-follow
                // AND the grow-while-typing sensor; the `min_h_full` inner
                // column makes the editor view's trailing click-filler
                // stretch to the container bottom even inside the scroll
                // container (children of a scroll area lay out against
                // content height) — clicking anywhere below the last line
                // places the caret at the end (textarea behavior).
                div()
                    .id("create-issue-description")
                    .flex_1()
                    .min_h(px(96.))
                    .px_3()
                    .overflow_y_scroll()
                    .track_scroll(&self.desc_scroll)
                    .child(
                        // EXP-421: the filler reads as text, so it carries the
                        // text cursor — the caret lands here on click.
                        div()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .cursor(gpui::CursorStyle::IBeam)
                            .child(self.description.clone()),
                    ),
            )
            .child(chips)
            .child(self.footer(cx))
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Web `bg-indigo-600` (a literal Tailwind color on web, not a theme token).
fn indigo_600() -> gpui::Hsla {
    rgb_hsla(0x4f, 0x46, 0xe5)
}

/// Web `hover:bg-indigo-700`.
fn indigo_700() -> gpui::Hsla {
    rgb_hsla(0x43, 0x38, 0xca)
}

/// Pressed state: indigo-800.
fn indigo_800() -> gpui::Hsla {
    rgb_hsla(0x37, 0x30, 0xa3)
}

/// The web's solid primary-action button (`bg-indigo-600 hover:bg-indigo-700
/// text-white text-xs font-medium rounded-md`, size xs) — used by the board
/// "New Issue" button and the create-dialog submit.
///
/// Hand-rolled `div` on purpose: the pinned gpui-component
/// `ButtonCustomVariant` ignores its `.foreground()` (labels render in the
/// fill color) and washes the fill toward transparent
/// (`mix_oklab(transparent, 0.2..0.4)` in `button.rs`), so it cannot produce
/// this solid fill. Callers add label/icon children and — when not disabled —
/// an `.on_click`.
pub(crate) fn indigo_button(
    id: impl Into<gpui::ElementId>,
    disabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap_1()
        .h_6()
        .px_2p5()
        .rounded(cx.theme().radius)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(gpui::white())
        .bg(indigo_600())
        .cursor_default();
    if disabled {
        // Web `disabled:opacity-50 disabled:pointer-events-none` (callers
        // skip `.on_click` while disabled).
        base.opacity(0.5)
    } else {
        base.hover(|style| style.bg(indigo_700()))
            .active(|style| style.bg(indigo_800()))
    }
}

fn rgb_hsla(r: u8, g: u8, b: u8) -> gpui::Hsla {
    gpui::Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: 1.0,
    }
    .into()
}

/// `#rrggbb` → Hsla (board/label colors are hex strings).
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(rgb_hsla(r, g, b))
}

/// A user's display name (name, else email, else the `Member <LAST4>` fallback
/// — web shows `user.name`; a row with neither field is a co-member whose PII
/// didn't sync).
fn display_name(user: &User) -> String {
    // Filter blanks before each fallback: a name-less Apple-ID user has
    // `name = ""` (Better Auth stores empty, not null), which must fall through
    // to the email rather than render as an empty label (EXP-228).
    user.name
        .clone()
        .filter(|name| !name.is_empty())
        .or_else(|| user.email.clone().filter(|email| !email.is_empty()))
        .unwrap_or_else(|| domain::member_fallback_label(&user.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_parse_and_reject_bad_input() {
        assert!(parse_hex_color("#6366f1").is_some());
        assert!(parse_hex_color("6366f1").is_none());
        assert!(parse_hex_color("#66f1").is_none());
        assert!(parse_hex_color("#zzzzzz").is_none());
    }

    #[test]
    fn strip_draft_images_removes_only_staged_images() {
        let markdown = "Intro text\n\n![shot](draft://abc-1)\n\n![kept](/api/attachments/xyz)\n\nOutro";
        assert_eq!(
            strip_draft_images(markdown),
            "Intro text\n\n![kept](/api/attachments/xyz)\n\nOutro"
        );
        assert_eq!(strip_draft_images("![only](draft://a)"), "");
        assert_eq!(strip_draft_images(""), "");
    }
}
