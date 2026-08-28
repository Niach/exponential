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
//!   REV2-49 deleted the time-of-day fields, §4.2) + the right-aligned
//!   primary submit button (EXP-586: no "Create more", no image chips —
//!   images live inline in the description only)
//! - footer: ONLY when there are queued non-image files or an error — the
//!   file chips / the error line; otherwise absent.
//!
//! Submit (§4.1): `issues.create` on a background thread; the close+navigate
//! is **gated** on the created row becoming visible in the synced `issues`
//! collection (the desktop's awaitTxId analog).

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
    v_flex, ActiveTheme as _, Icon, Sizable as _,
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
use crate::navigation::{active_board_id, nav_for_window};
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
        let team_id = board.team_id.clone();
        let view = cx.new(|cx| {
            CreateIssueDialogView::new(board.id.clone(), board.team_id.clone(), max_height, window, cx)
        });
        let busy = view.clone();
        let submit = view.clone();
        // WEAK on purpose: the title closure is an `Rc` the shell holds for
        // the window's whole life, so a strong handle would keep the content
        // view alive past the dialog's close.
        let title_view = view.downgrade();
        DialogContent::new(view.clone())
            .padless()
            // EXP-287: the board-pill breadcrumb rides the window's titlebar
            // strip (the shell owns the chrome now — see `native_dialog`).
            .title_content(move |_, cx| {
                let Some(view) = title_view.upgrade() else {
                    return "New issue".into_any_element();
                };
                let board_id = view.read(cx).board_id.clone();
                let board = Store::global(cx)
                    .collections()
                    .boards
                    .read(cx)
                    .get(&board_id)
                    .cloned();
                match board {
                    Some(board) => title_board_select(&view, &team_id, &board, cx),
                    None => "New issue".into_any_element(),
                }
            })
            // EXP-449: the pill is a live board SELECT now — without this the
            // shell would never repaint it after a pick.
            .title_follows(&view)
            .can_close(move |cx| !busy.read(cx).submitting)
            // Enter anywhere in the dialog submits (web form submit).
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

/// EXP-287: the titlebar label — board chip · › · "New issue". Lives in the
/// window's `TitleBar` strip now that the shell owns the dialog chrome.
///
/// EXP-449: the chip is a live board SELECT — the board's own glyph tinted
/// with its color (the EXP-282 treatment that replaced the anonymous color
/// dot everywhere else) plus its prefix, over the team's boards. A
/// single-board team keeps a static glass chip. Nothing else in the dialog
/// resets on a pick: status/assignee/label options are team-scoped, the menu
/// only offers same-team boards, and `submit` reads `board_id` live.
fn title_board_select(
    view: &Entity<CreateIssueDialogView>,
    team_id: &str,
    board: &domain::rows::Board,
    cx: &App,
) -> AnyElement {
    let tint = board
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    let icon = crate::icons::board_icon(board).xsmall().text_color(tint);
    let prefix = SharedString::from(board.prefix.clone().unwrap_or_default());

    // Same "is there anywhere else to go" rule the issue header's Board chip
    // uses (EXP-57 `move_target_boards`): a single-board team gets a static
    // chip instead of a one-entry menu.
    let chip: AnyElement = if crate::issue_list::move_target_boards(cx, &board.id).is_empty() {
        crate::surface::glass_chip()
            .child(icon)
            .child(crate::pickers::chip_label(prefix, false, cx))
            .into_any_element()
    } else {
        let current_id = board.id.clone();
        let team_id = team_id.to_string();
        let view = view.clone();
        chip_button("create-board-chip", cx)
            .icon(icon)
            .child(crate::pickers::chip_label(prefix, false, cx))
            .child(
                Icon::new(registry::UI_CHEVRON_DOWN)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            // A plain dropdown menu, not the searchable `board_picker_popover`
            // — that one needs a host-owned `Entity<InputState>` for its query,
            // which this `Fn` title closure has nowhere to keep.
            .dropdown_menu(move |mut menu, _window, cx| {
                menu = menu.check_side(gpui_component::Side::Right);
                for board in Store::global(cx).collections().boards_in_team(&team_id, cx) {
                    let is_current = board.id == current_id;
                    let tint = board
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(gpui::opaque_grey(0.5, 1.0));
                    let icon = crate::icons::board_icon(&board).xsmall().text_color(tint);
                    let name = SharedString::from(board.name.clone());
                    let picked = board.id.clone();
                    let view = view.clone();
                    menu = menu.item(
                        gpui_component::menu::PopupMenuItem::element(move |_, cx| {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(icon.clone())
                                .child(
                                    div()
                                        .text_color(cx.theme().popover_foreground)
                                        .child(name.clone()),
                                )
                        })
                        .checked(is_current)
                        .disabled(is_current)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.board_id = picked.clone();
                                cx.notify();
                            });
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    };

    h_flex()
        .gap_1p5()
        .items_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(chip)
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
        // EXP-335: the rail's attach button — image picks embed inline via
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
        // Re-render on every editor change so the submit gating and the
        // grow-with-content sensor track the live description.
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
            status: default_status,
            priority: IssuePriority::None,
            assignee_id: solo_member_id.clone(),
            solo_member_id,
            selected_label_ids: Vec::new(),
            label_query,
            due_date: None,
            due_calendar,
            staged_files: Vec::new(),
            next_staged_file_key: 0,
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
                                    if let Err(err) = transport.upload(
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
                    // Gated path (§4.1): close + navigate only once the row
                    // is visible in the synced collection.
                    let issues = window
                        .update(|_, cx| Store::global(cx).collections().issues.clone())
                        .ok();
                    if let Some(issues) = issues {
                        queries::await_row_visible(&issues, &issue_id, window).await;
                    }
                    let _ = this.update_in(window, |this, window, cx| {
                        // EXP-288/EXP-510: the rail may point anywhere while
                        // the dialog is up — land fully scoped on the ISSUE's
                        // board (rail tool + active board + tab origin).
                        let board_id = this.board_id.clone();
                        native_dialog::close_then(window, cx, move |window, cx| {
                            crate::navigation::open_issue_scoped(window, cx, issue_id, board_id);
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
            .child(crate::pickers::chip_label(resolved.name.clone(), false, cx))
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
            .child(crate::pickers::chip_label(config.label, false, cx))
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
            .child(crate::pickers::chip_label(label, selected.is_none(), cx))
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
        let unset = selected.is_empty();
        let label: SharedString = if unset {
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
            .child(crate::pickers::chip_label(label, unset, cx));
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
                .child(crate::pickers::chip_label(
                    label,
                    self.due_date.is_none(),
                    cx,
                )),
            self.due_calendar.clone(),
            Some(px(280.)),
            None,
        )
    }

    // -- footer ----------------------------------------------------------------

    /// Web `IssueEditorAttachmentRail` (EXP-586 shape): one chip per queued
    /// non-image draft file (web `issue-attachment-file-chip-*` parity).
    /// Images are NOT listed — they render inline in the description and are
    /// removed there.
    fn attachment_rail(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let removable = !self.submitting;

        h_flex()
            .min_w_0()
            .flex_1()
            .gap_1p5()
            .items_center()
            .overflow_hidden()
            .children(self.staged_files.iter().map(|file| {
                let remove: Option<attachments_row::ChipRemove> = removable.then(|| {
                    let view = cx.entity().clone();
                    let key = file.key;
                    let on_click = Box::new(
                        move |_: &gpui::ClickEvent, _window: &mut Window, cx: &mut App| {
                            view.update(cx, |this, cx| {
                                this.staged_files.retain(|staged| staged.key != key);
                                cx.notify();
                            });
                        },
                    )
                        as Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
                    (
                        SharedString::from(format!("create-attachment-file-remove-{}", file.key)),
                        on_click,
                    )
                });
                attachments_row::file_chip(
                    gpui::ElementId::from(("create-attachment-file-chip", file.key as usize)),
                    file.filename.clone(),
                    Some(file.content_type.as_str()),
                    file.bytes.len() as i64,
                    remove,
                    cx,
                )
            }))
    }

    /// The chip row's right-aligned submit (web `chipRowAction`): SOLID
    /// primary, label swaps while creating, no spinner.
    fn submit_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let title_empty = self.title.read(cx).value().trim().is_empty();
        let submit_disabled = title_empty || self.submitting;
        let submit_label: &'static str = if self.submitting {
            "Creating..."
        } else {
            "Create issue"
        };

        primary_button("create-issue-submit", submit_disabled, cx)
            .child(SharedString::from(submit_label))
            .when(!submit_disabled, |button| {
                button.on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
            })
    }

    /// EXP-586: the footer exists only for an error line or queued non-image
    /// files; with neither it is not rendered at all.
    fn footer(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let content: AnyElement = match &self.error {
            Some(error) => div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(error.clone())
                .into_any_element(),
            None if !self.staged_files.is_empty() => self.attachment_rail(cx).into_any_element(),
            None => return None,
        };

        Some(
            h_flex()
                .px_4()
                .py_3()
                .items_center()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(div().min_w_0().flex_1().child(content))
                .into_any_element(),
        )
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
            .child(self.due_chip(cx))
            // EXP-586: submit rides the chip row, right-aligned.
            .child(
                div()
                    .ml_auto()
                    .flex_shrink_0()
                    .pl_2()
                    .child(self.submit_button(cx)),
            );

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
            .children(self.footer(cx))
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// The web's solid primary-action button (`bg-primary text-primary-foreground
/// hover:bg-primary/90 text-xs font-medium rounded-md`, size xs — EXP-594:
/// near-white, the retired indigo accent's replacement) — used by the board
/// "New Issue" button and the create-dialog submit.
///
/// Hand-rolled `div` on purpose: the pinned gpui-component
/// `ButtonCustomVariant` ignores its `.foreground()` (labels render in the
/// fill color) and washes the fill toward transparent
/// (`mix_oklab(transparent, 0.2..0.4)` in `button.rs`), so it cannot produce
/// this solid fill. Callers add label/icon children and — when not disabled —
/// an `.on_click`.
pub(crate) fn primary_button(
    id: impl Into<gpui::ElementId>,
    disabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (fill, fill_hover, fill_active, label) = (
        theme.primary,
        theme.primary_hover,
        theme.primary_active,
        theme.primary_foreground,
    );
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap_1()
        .h_6()
        .px_2p5()
        .rounded(theme.radius)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(label)
        .bg(fill)
        .cursor_default();
    if disabled {
        // Web `disabled:opacity-50 disabled:pointer-events-none` (callers
        // skip `.on_click` while disabled).
        base.opacity(0.5)
    } else {
        base.hover(move |style| style.bg(fill_hover))
            .active(move |style| style.bg(fill_active))
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
