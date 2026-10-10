//! Settings → Storage (EXP-297 team file manager).
//!
//! Web parity: `components/team/storage-section.tsx`. Owner-only, like the
//! router behind it: every attachment of the team from tRPC
//! `attachments.listForTeam` — NOT the synced collection, because the tRPC
//! list includes trashed-board rows plus the server-computed
//! `referenced`/`isImage` flags and `totalBytes` — with per-file delete and
//! the bulk "Sweep unreferenced images" reclaim.
//!
//! Reads are fetch-on-open + refetch after every mutation (never optimistic:
//! the list is a server aggregate the Electric echo can't rebuild). The
//! owning issue identifier and the uploader are joined CLIENT-side from the
//! synced issues/users collections; a trashed/unknown issue and an absent
//! uploader (widget-filed screenshots have none) fall back to a dash,
//! exactly like the web table.
//!
//! The plan usage meter mirrors the web section's `UsageBar` off
//! `billing.teamPlan`, fetched alongside the list (and re-fetched with it
//! after every mutation). It is DISPLAY-only — no upgrade/purchase
//! affordances (billing UI stays web-only, §4.9) — and best-effort: a failed
//! billing probe (older server, self-hosted quirks) just drops the bar, the
//! table never blocks on it, and the self-hosted `unlimited` plan renders no
//! bar at all (web parity: `plan !== 'unlimited'`).

use gpui::{
    div, AnyElement, App, Entity,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::ButtonVariant,
    h_flex,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use crate::toast::Toast;
use sync::Store;

use api::attachments::{AttachmentsListForTeamOutput, TeamAttachmentRow};
use api::billing::TeamPlanOut;

use crate::icons::ExpIcon;
use crate::attachment_markdown_preview::{open_markdown_preview, MarkdownPreviewTarget};
use crate::issue_files::{format_bytes, icon_for_content_type, is_markdown_attachment};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{error_notice, section};

struct Loaded {
    list: Result<AttachmentsListForTeamOutput, String>,
    /// `None` = the billing probe failed or hasn't landed (best-effort —
    /// the usage bar just stays away; web parity: the table renders either
    /// way).
    plan: Option<TeamPlanOut>,
}

enum Load {
    Idle,
    Loading,
    Ready(Loaded),
}

pub struct StoragePane {
    nav: Entity<Navigation>,
    load: Load,
    /// The team the current `load` belongs to; a switch re-fetches.
    loaded_team: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch.
    account_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// A delete/sweep in flight — disables the mutation affordances.
    busy: bool,
    _subscriptions: Vec<Subscription>,
}

impl StoragePane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        // The list itself is a server read; the issue-identifier and
        // uploaded-by joins read the synced issues/users collections, so a
        // sync delta re-renders the rows.
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            load: Load::Idle,
            loaded_team: None,
            account_id: None,
            generation: 0,
            busy: false,
            _subscriptions: subscriptions,
        }
    }

    /// Kick the server fetch when the pane is first shown or the team /
    /// account changed. Runs at render time so a hidden pane never fetches.
    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
        }
        let same_team = self.loaded_team.as_deref() == Some(team_id);
        if same_team && !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        self.load = Load::Loading;
        self.loaded_team = Some(team_id.to_string());
        self.generation += 1;
        let generation = self.generation;
        let team_id = team_id.to_string();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    // Billing first and best-effort (`.ok()`): the usage bar
                    // is decoration, the list carries its own error state.
                    let plan = api::billing::billing_team_plan(&trpc, &team_id)
                        .map_err(|err| {
                            log::warn!("[ui] billing.teamPlan failed (usage bar hidden): {err}");
                            err
                        })
                        .ok();
                    let list = api::attachments::attachments_list_for_team(&trpc, &team_id)
                        .map_err(|err| err.to_string());
                    Loaded { list, plan }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                this.load = Load::Ready(result);
                cx.notify();
            });
        })
        .detach();
    }

    /// Refetch after a mutation (or the Refresh button): drop the cached
    /// list; the next render re-fetches.
    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    /// The per-row Delete confirm + `attachments.delete` → refetch. Mirrors
    /// the issue-detail files rail's confirm copy: the server rewrites any
    /// markdown still embedding the attachment.
    fn confirm_delete(
        &mut self,
        row: &TeamAttachmentRow,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        // The OPENER's window — the alert closes on confirm, so a failure
        // notification has to land back on the settings window.
        let handle = window.window_handle();
        let attachment_id = row.id.clone();
        let filename = row.filename.clone();
        let spec = AlertSpec::from_prompt(
            "Delete attachment",
            &domain::prompts::delete_file(&filename),
        )
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let view = view.clone();
            let attachment_id = attachment_id.clone();
            let filename = filename.clone();
            cx.spawn(async move |cx| {
                let deleted_id = attachment_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { api::attachments::attachments_delete(&trpc, &deleted_id) })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    if result.is_ok() {
                        // The list is a server aggregate — refetch, never
                        // edit it optimistically.
                        this.refetch(cx);
                    }
                    cx.notify();
                });
                if let Err(error) = result {
                    log::warn!("[ui] attachments.delete({attachment_id}) failed: {error}");
                    let note = Toast::error(format!(
                        "Could not delete {filename}: {error}"
                    ));
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

    /// The bulk-sweep confirm + `attachments.sweepUnreferencedImages` →
    /// result notification + refetch. `candidates` is the client-computed
    /// image+unreferenced count the button/title show; the server re-derives
    /// the real set (and keeps <24h uploads) inside its transaction.
    fn confirm_sweep(
        &mut self,
        candidates: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        let view = cx.entity().downgrade();
        let handle = window.window_handle();
        let plural = if candidates == 1 { "" } else { "s" };
        let spec = AlertSpec::new(
            format!("Sweep {candidates} unreferenced image{plural}?"),
            "These images are no longer embedded in any description or \
             comment in this team, so deleting them changes no text. Images \
             uploaded in the last 24 hours are kept. They may still be \
             sitting in an unsaved draft. Files are never swept.",
            "Sweep images",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let view = view.clone();
            let team_id = team_id.clone();
            cx.spawn(async move |cx| {
                let sweep_team = team_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::attachments::attachments_sweep_unreferenced_images(&trpc, &sweep_team)
                    })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    this.busy = false;
                    if result.is_ok() {
                        this.refetch(cx);
                    }
                    cx.notify();
                });
                let note = match result {
                    Ok(out) => Toast::info(sweep_result_message(
                        out.deleted_count,
                        out.freed_bytes,
                        out.skipped_recent_count,
                    )),
                    Err(error) => {
                        log::warn!("[ui] attachments.sweepUnreferencedImages failed: {error}");
                        Toast::error(format!(
                            "Could not sweep unreferenced images: {error}"
                        ))
                    }
                };
                let _ = handle.update(cx, |_, window, cx| {
                    crate::toast::show(note, window, cx);
                });
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// One attachment row: type icon, filename, size, owning issue
    /// identifier, uploader, created date, status chip, delete. Identifier
    /// and uploader join from the synced collections — a trashed board's
    /// issues are out of sync and widget screenshots have no uploader, so
    /// those cells read a dash (web parity).
    fn render_row(
        &self,
        row: &TeamAttachmentRow,
        identifier: Option<String>,
        uploader: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let status = attachment_status(row.is_image, &row.content_type, row.referenced);
        let row_for_delete = row.clone();

        // Image filenames open the in-app lightbox (EXP-316) — same
        // `open_image_preview` path as the editor's attachment chips;
        // markdown filenames open the in-app preview (EXP-1003, web parity).
        let attachment_url = format!("/api/attachments/{}", row.id);
        let markdown = is_markdown_attachment(Some(&row.content_type), Some(&row.filename));
        let previewable = (row.is_image || markdown)
            && queries::absolute_api_url(cx, &attachment_url).is_some();
        let filename_cell: AnyElement = if previewable {
            let label = row.filename.clone();
            let markdown_target = markdown.then(|| MarkdownPreviewTarget {
                attachment_id: row.id.clone(),
                filename: row.filename.clone(),
                size_bytes: row.size_bytes,
                team_id: active_team_id(&self.nav, cx),
            });
            div()
                .id(SharedString::from(format!("storage-preview-{}", row.id)))
                .min_w_0()
                .text_sm()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .cursor_pointer()
                .hover(|this| this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
                .on_click(move |_, window, cx| match &markdown_target {
                    Some(target) => open_markdown_preview(target.clone(), window, cx),
                    None => crate::image_preview::open_image_preview(
                        attachment_url.clone(),
                        label.clone(),
                        None,
                        None,
                        window,
                        cx,
                    ),
                })
                .child(SharedString::from(row.filename.clone()))
                .into_any_element()
        } else {
            div()
                .min_w_0()
                .text_sm()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(row.filename.clone()))
                .into_any_element()
        };

        // The owning issue as the ONE issue chip (EXP-316, EXP-885: it used
        // to be a hand-rolled `#IDENT` capsule of its own) — clicking leaves
        // Settings for the issue's detail screen. The column is narrow, so
        // the chip carries the identifier and status only, no title.
        // Unresolved identifiers (trashed board, not yet synced) stay a dash.
        let issue_cell: AnyElement = match identifier {
            Some(identifier) => {
                let issue_id = row.issue_id.clone();
                let chip = crate::issue_chip::issue_chip(
                    SharedString::from(format!("storage-issue-{}", row.id)),
                    identifier,
                    "",
                )
                .on_click(move |_, window, cx| {
                    crate::navigation::navigate(
                        window,
                        cx,
                        crate::navigation::Screen::IssueDetail {
                            issue_id: issue_id.clone(),
                        },
                    );
                });
                match crate::issue_chip::synced_issue_status(&row.issue_id, cx) {
                    Some(status) => chip.status(status),
                    None => chip,
                }
                .into_any_element()
            }
            None => div()
                .text_xs()
                .text_color(muted)
                .child("—")
                .into_any_element(),
        };

        // EXP-1076: an attachment is an ENTITY, so the list is the hairline
        // LADDER (`list_row` + `flat_row`, the web `SETTINGS_LIST_CLASS`
        // twin) rather than the inset-grouped block a form's fields wear.
        // The cells stay a fixed GRID inside the row — every column but the
        // filename carries a definite width, so size/issue/author/date/status
        // stack in straight columns instead of each row laying its own cells
        // out around its own content (which is what made the table zig-zag).
        crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::from(icon_for_content_type(Some(&row.content_type)))
                            .xsmall()
                            .text_color(muted)
                            .flex_shrink_0(),
                    )
                    .child(filename_cell),
            )
            .child(
                div()
                    .w(gpui::px(STORAGE_COL_SIZE))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format_bytes(row.size_bytes))),
            )
            .child(
                h_flex()
                    .w(gpui::px(STORAGE_COL_ISSUE))
                    .flex_shrink_0()
                    .child(issue_cell),
            )
            .child(
                div()
                    .w(gpui::px(STORAGE_COL_AUTHOR))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(uploader.unwrap_or_else(|| "—".to_string()))),
            )
            .child(
                div()
                    .w(gpui::px(STORAGE_COL_DATE))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .whitespace_nowrap()
                    .child(SharedString::from(format_created_date(&row.created_at))),
            )
            .child(
                h_flex()
                    .w(gpui::px(STORAGE_COL_STATUS))
                    .flex_shrink_0()
                    .child(status_chip(
                        SharedString::from(format!("storage-status-{}", row.id)),
                        status,
                        cx,
                    )),
            )
            .child(
                // EXP-862: a row's trash is a GHOST glyph, never a circle.
                crate::controls::ghost_icon_button(
                    SharedString::from(format!("storage-delete-{}", row.id)),
                    Icon::from(ExpIcon::Trash2),
                    cx,
                )
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_delete(&row_for_delete, window, cx);
                    })),
            )
    }
}

impl Render for StoragePane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        self.ensure_loaded(&team_id, cx);

        // Web parity: the bulk Sweep rides the band's trailing slot; there
        // is no Refresh — the list fetches on open and refetches after every
        // delete and sweep.
        let candidates = match &self.load {
            Load::Ready(Loaded { list: Ok(list), .. }) => list
                .attachments
                .iter()
                .filter(|row| row.is_image && !row.referenced)
                .count(),
            _ => 0,
        };
        let sweep_label = if candidates > 0 {
            format!("Sweep unreferenced images ({candidates})")
        } else {
            "Sweep unreferenced images".to_string()
        };
        let sweep = crate::surface::glass_pill_button(
            "storage-sweep",
            crate::surface::PillSize::Sm,
            cx,
        )
        .icon(Icon::from(ExpIcon::Trash2).xsmall())
        .label(SharedString::from(sweep_label))
        .disabled(candidates == 0 || self.busy)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.confirm_sweep(candidates, window, cx);
        }))
        .into_any_element();
        let mut body = section(cx).child(crate::surface::glass_section_header(
            "Storage",
            Some(sweep),
            cx,
        ));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(crate::controls::skeleton().h_4().w_64())
                        .child(crate::controls::skeleton().h_8().w_full())
                        .child(crate::controls::skeleton().h_8().w_full()),
                );
            }
            Load::Ready(Loaded {
                list: Err(message), ..
            }) => {
                body = body.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Loaded {
                list: Ok(list),
                plan,
            }) => {
                // Web `UsageBar`: shown for every billed plan (`plan !==
                // 'unlimited'`); a null storage limit reads "… / unlimited"
                // with no track. Self-hosted (`unlimited`) and a failed
                // billing probe render no meter at all — the totalBytes
                // summary below always carries the usage.
                let meter = plan
                    .as_ref()
                    .filter(|plan| plan.plan != "unlimited")
                    .map(|plan| {
                        super::usage_bar(
                            "Attachment storage",
                            super::format_storage(plan.usage.storage_mb),
                            plan.limits.storage_mb.map(super::format_storage),
                            plan.limits
                                .storage_mb
                                .filter(|limit| *limit > 0.)
                                .map(|limit| plan.usage.storage_mb / limit),
                            cx,
                        )
                    });
                let rows = list.attachments.clone();
                let total_bytes = list.total_bytes;
                let plural = if rows.len() == 1 { "" } else { "s" };

                body = body.children(meter);

                // The usage summary line (the sweep sits in the band).
                body = body.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!(
                            "{} attachment{plural} · {}",
                            rows.len(),
                            format_bytes(total_bytes)
                        ))),
                );

                if rows.is_empty() {
                    body = body.child(
                        div()
                            .py_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No attachments yet."),
                    );
                } else {
                    let collections = Store::global(cx).collections().clone();
                    let joins: Vec<(Option<String>, Option<String>)> = {
                        let issues = collections.issues.read(cx);
                        let users = collections.users.read(cx);
                        rows.iter()
                            .map(|row| {
                                let identifier = issues
                                    .get(&row.issue_id)
                                    .map(|issue| issue.identifier.clone());
                                // Web parity: `uploader?.name || uploader?.
                                // email || —` — a null uploader_id (widget
                                // screenshots) or an unsynced user row reads
                                // a dash.
                                let uploader = row
                                    .uploader_id
                                    .as_deref()
                                    .and_then(|id| users.get(id))
                                    .and_then(|user| {
                                        user.name
                                            .clone()
                                            .filter(|name| !name.is_empty())
                                            .or_else(|| {
                                                user.email
                                                    .clone()
                                                    .filter(|email| !email.is_empty())
                                            })
                                    });
                                (identifier, uploader)
                            })
                            .collect()
                    };
                    // EXP-1076: the gapless ladder under the band.
                    let list: Vec<gpui::Div> = rows
                        .iter()
                        .zip(joins.into_iter())
                        .enumerate()
                        .map(|(index, (row, (identifier, uploader)))| {
                            crate::surface::list_row(
                                self.render_row(row, identifier, uploader, cx),
                                index,
                            )
                        })
                        .collect();
                    body = body.child(v_flex().w_full().min_w_0().children(list));
                }
            }
        }

        v_flex().child(body)
    }
}

// EXP-698 — the attachment table's fixed COLUMN ladder. Only the filename
// cell flexes; every other cell is a definite width so the columns line up
// down the list. The widths are the widest realistic content plus a little
// air: "403.2 KB", "#APP-123", "Aug 30, 2026", "Unreferenced".
/// Size cell.
const STORAGE_COL_SIZE: f32 = 80.;
/// The `#IDENT` chip cell.
const STORAGE_COL_ISSUE: f32 = 120.;
/// Uploader cell (ellipsised).
const STORAGE_COL_AUTHOR: f32 = 120.;
/// Created-date cell.
const STORAGE_COL_DATE: f32 = 90.;
/// Status-chip cell — fixed so the trailing delete button never zig-zags
/// with the chip's label length.
const STORAGE_COL_STATUS: f32 = 104.;

/// The row's status label (web parity): "File" for anything that is not an
/// image, a video or audio; otherwise "In use" / "Unreferenced".
fn attachment_status(is_image: bool, content_type: &str, referenced: bool) -> &'static str {
    let content_type = content_type.trim().to_ascii_lowercase();
    let is_media =
        is_image || content_type.starts_with("video/") || content_type.starts_with("audio/");
    if !is_media {
        "File"
    } else if referenced {
        "In use"
    } else {
        "Unreferenced"
    }
}

/// The status chip = the shared small READONLY glass pill (web `Pill`).
fn status_chip(id: SharedString, label: &'static str, cx: &App) -> impl IntoElement {
    use crate::surface::{PillMode, PillSize};
    crate::surface::glass_pill(id, PillSize::Sm, PillMode::Readonly, cx).child(label)
}

/// Web `formatDate` ("Jul 1, 2026"): month + day + year of the timestamp in
/// the machine's LOCAL zone (`toLocaleDateString` does the same), so a row
/// created at 23:30 UTC reads as the day the person saw it. A bare
/// `YYYY-MM-DD` is a calendar day already and prints as-is; unparseable
/// input echoes back verbatim. `pub(super)`: the API-keys, sign-in-methods
/// and archived-boards panes render their dates with the same shape
/// (EXP-238).
pub(super) fn format_created_date(timestamp: &str) -> String {
    use chrono::{DateTime, Local, NaiveDate};
    const SHAPE: &str = "%b %-d, %Y";
    let date = timestamp.trim();
    // ISO (`2026-07-01T10:00:00.000Z`, offsets too) or the Postgres space
    // form (`2026-01-05 08:00:00+00`, optional fraction).
    let instant = DateTime::parse_from_rfc3339(date)
        .or_else(|_| DateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S%.f%#z"));
    if let Ok(instant) = instant {
        return instant.with_timezone(&Local).format(SHAPE).to_string();
    }
    if let Ok(day) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        return day.format(SHAPE).to_string();
    }
    date.to_string()
}

/// The sweep-result notification copy (web `handleSweep` toast parity).
fn sweep_result_message(deleted: i64, freed_bytes: i64, skipped_recent: i64) -> SharedString {
    if deleted == 0 {
        if skipped_recent > 0 {
            let plural = if skipped_recent == 1 { "" } else { "s" };
            return SharedString::from(format!(
                "Nothing swept. {skipped_recent} recent upload{plural} are still inside \
                 the 24h grace window."
            ));
        }
        return "Nothing to sweep. Every image is still referenced.".into();
    }
    let plural = if deleted == 1 { "" } else { "s" };
    SharedString::from(format!(
        "Deleted {deleted} image{plural}, freeing {}.",
        format_bytes(freed_bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_is_never_labelled_file() {
        assert_eq!(attachment_status(false, "video/mp4", true), "In use");
        assert_eq!(attachment_status(false, "audio/mpeg", false), "Unreferenced");
        assert_eq!(attachment_status(true, "image/png", false), "Unreferenced");
        assert_eq!(attachment_status(false, "application/pdf", true), "File");
    }

    /// 09:00Z stays the same calendar day in every zone (UTC-12 … UTC+14),
    /// so these read the same on a laptop in Vienna and on a UTC runner.
    #[test]
    fn created_dates_read_like_the_web_table() {
        assert_eq!(
            format_created_date("2026-07-01T09:00:00.000Z"),
            "Jul 1, 2026"
        );
        assert_eq!(format_created_date("2025-12-31T09:00:00Z"), "Dec 31, 2025");
        // Postgres space form (with and without a fraction), an explicit
        // offset, and bare dates degrade gracefully too.
        assert_eq!(format_created_date("2026-01-05 09:00:00+00"), "Jan 5, 2026");
        assert_eq!(format_created_date("2026-01-05 09:00:00.123456+00"), "Jan 5, 2026");
        assert_eq!(format_created_date("2026-01-05T09:00:00+02:00"), "Jan 5, 2026");
        assert_eq!(format_created_date("2026-03-09"), "Mar 9, 2026");
        // Garbage echoes back instead of panicking or lying.
        assert_eq!(format_created_date("not-a-date"), "not-a-date");
        assert_eq!(format_created_date(""), "");
    }

    /// F48: the date is the LOCAL day (web `toLocaleDateString`), not the
    /// UTC day the ISO string leads with. 23:30Z on Jun 30 is already Jul 1
    /// east of UTC and still Jun 30 at or west of it.
    #[test]
    fn created_dates_use_the_local_day() {
        use chrono::{Offset, TimeZone};
        let instant = chrono::DateTime::parse_from_rfc3339("2026-06-30T23:30:00Z")
            .expect("a valid timestamp");
        let offset_seconds = chrono::Local
            .offset_from_utc_datetime(&instant.naive_utc())
            .fix()
            .local_minus_utc();
        let expected = if offset_seconds >= 30 * 60 {
            "Jul 1, 2026"
        } else {
            "Jun 30, 2026"
        };
        assert_eq!(format_created_date("2026-06-30T23:30:00Z"), expected);
        // The Postgres form of the same instant lands on the same day.
        assert_eq!(format_created_date("2026-06-30 23:30:00+00"), expected);
    }

    #[test]
    fn sweep_messages_cover_all_outcomes() {
        assert_eq!(
            sweep_result_message(0, 0, 0).as_ref(),
            "Nothing to sweep. Every image is still referenced."
        );
        assert_eq!(
            sweep_result_message(0, 0, 2).as_ref(),
            "Nothing swept. 2 recent uploads are still inside the 24h grace window."
        );
        assert_eq!(
            sweep_result_message(1, 2048, 0).as_ref(),
            "Deleted 1 image, freeing 2.0 KB."
        );
        assert_eq!(
            sweep_result_message(3, 1_572_864, 1).as_ref(),
            "Deleted 3 images, freeing 1.5 MB."
        );
    }
}
