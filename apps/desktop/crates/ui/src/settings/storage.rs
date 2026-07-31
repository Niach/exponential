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
    div, prelude::FluentBuilder as _, AnyElement, App, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    notification::Notification,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::attachments::{AttachmentsListForTeamOutput, TeamAttachmentRow};
use api::billing::TeamPlanOut;

use crate::icons::ExpIcon;
use crate::issue_files::{format_bytes, icon_for_content_type};
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{card_title, error_notice, section};

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
        let spec = AlertSpec::new(
            format!("Delete \"{filename}\"?"),
            "The attachment is deleted for everyone and cannot be restored. \
             Every description or comment that embeds it is rewritten in the \
             same step, replacing the image with a plain-text note.",
            "Delete attachment",
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
                    let note = Notification::error(SharedString::from(format!(
                        "Could not delete {filename}: {error}"
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
                    Ok(out) => Notification::info(sweep_result_message(
                        out.deleted_count,
                        out.freed_bytes,
                        out.skipped_recent_count,
                    )),
                    Err(error) => {
                        log::warn!("[ui] attachments.sweepUnreferencedImages failed: {error}");
                        Notification::error(SharedString::from(format!(
                            "Could not sweep unreferenced images: {error}"
                        )))
                    }
                };
                let _ = handle.update(cx, |_, window, cx| {
                    window.push_notification(note, cx);
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
        index: usize,
        row: &TeamAttachmentRow,
        identifier: Option<String>,
        uploader: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let status: &str = if !row.is_image {
            "File"
        } else if row.referenced {
            "In use"
        } else {
            "Unreferenced"
        };
        let row_for_delete = row.clone();

        // Image filenames open the in-app lightbox (EXP-316) — same
        // `open_image_preview` path as the editor's attachment chips.
        let attachment_url = format!("/api/attachments/{}", row.id);
        let previewable =
            row.is_image && queries::absolute_api_url(cx, &attachment_url).is_some();
        let filename_cell: AnyElement = if previewable {
            let label = row.filename.clone();
            div()
                .id(SharedString::from(format!("storage-preview-{}", row.id)))
                .min_w_0()
                .text_sm()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .cursor_pointer()
                .hover(|this| this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
                .on_click(move |_, window, cx| {
                    crate::image_preview::open_image_preview(
                        attachment_url.clone(),
                        label.clone(),
                        None,
                        window,
                        cx,
                    );
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

        // The owning issue as an editor-parity `#IDENT` chip (EXP-316) —
        // clicking leaves Settings for the issue's detail screen. Unresolved
        // identifiers (trashed board, not yet synced) stay a dash.
        let issue_cell: AnyElement = match identifier {
            Some(identifier) => {
                let issue_id = row.issue_id.clone();
                let hover_bg = cx.theme().secondary_hover;
                div()
                    .id(SharedString::from(format!("storage-issue-{}", row.id)))
                    .px_1p5()
                    .rounded_full()
                    .bg(cx.theme().secondary)
                    .text_xs()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().secondary_foreground)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .cursor_pointer()
                    .hover(move |this| this.bg(hover_bg))
                    .on_click(move |_, window, cx| {
                        crate::navigation::navigate(
                            window,
                            cx,
                            crate::navigation::Screen::IssueDetail {
                                issue_id: issue_id.clone(),
                            },
                        );
                    })
                    .child(SharedString::from(format!("#{identifier}")))
                    .into_any_element()
            }
            None => div()
                .text_xs()
                .text_color(muted)
                .child("—")
                .into_any_element(),
        };

        h_flex()
            .w_full()
            .items_center()
            .gap_3()
            .py_1p5()
            .when(index > 0, |this| {
                this.border_t_1().border_color(super::row_stroke(cx))
            })
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
                    .w_16()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format_bytes(row.size_bytes))),
            )
            .child(h_flex().w_20().flex_shrink_0().child(issue_cell))
            .child(
                div()
                    .w(gpui::px(112.))
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
                    .w_24()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format_created_date(&row.created_at))),
            )
            .child(status_chip(status, cx))
            .child(
                Button::new(SharedString::from(format!("storage-delete-{}", row.id)))
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::Trash2).xsmall().text_color(muted))
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

        let mut body = section(cx).child(card_title("Storage"));

        // Refresh lives at the TOP of the pane (EXP-316) — inside the
        // summary/sweep header row once the list is up, on its own row while
        // loading or after a failure.
        let refresh = Button::new("storage-refresh")
            .ghost()
            .xsmall()
            .label("Refresh")
            .loading(matches!(self.load, Load::Loading))
            .on_click(cx.listener(|this, _, _, cx| this.refetch(cx)));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(h_flex().w_full().justify_end().child(refresh));
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_4().w_64())
                        .child(Skeleton::new().h_8().w_full())
                        .child(Skeleton::new().h_8().w_full()),
                );
            }
            Load::Ready(Loaded {
                list: Err(message), ..
            }) => {
                body = body.child(h_flex().w_full().justify_end().child(refresh));
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
                let candidates = rows
                    .iter()
                    .filter(|row| row.is_image && !row.referenced)
                    .count();
                let plural = if rows.len() == 1 { "" } else { "s" };
                let sweep_label = if candidates > 0 {
                    format!("Sweep unreferenced images ({candidates})")
                } else {
                    "Sweep unreferenced images".to_string()
                };

                body = body.children(meter);

                // Header line: usage summary left, the bulk sweep right.
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
                                    "{} attachment{plural} · {}",
                                    rows.len(),
                                    format_bytes(total_bytes)
                                ))),
                        )
                        .child(refresh)
                        .child(
                            Button::new("storage-sweep")
                                .outline()
                                .xsmall()
                                .icon(Icon::from(ExpIcon::Trash2).xsmall())
                                .label(SharedString::from(sweep_label))
                                .disabled(candidates == 0 || self.busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_sweep(candidates, window, cx);
                                })),
                        ),
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
                    let mut list = v_flex().w_full();
                    for (index, (row, (identifier, uploader))) in
                        rows.iter().zip(joins.into_iter()).enumerate()
                    {
                        list = list.child(self.render_row(index, row, identifier, uploader, cx));
                    }
                    body = body.child(list);
                }
            }
        }

        v_flex().child(body)
    }
}

/// Outline status chip (web `Badge` at compact density): images are
/// "In use" / "Unreferenced", non-images are "File".
fn status_chip(label: &'static str, cx: &App) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(super::row_stroke(cx))
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

/// Web `formatDate` ("Jul 1, 2026"): month + day + year off the ISO
/// timestamp's leading date. Unparseable input echoes back verbatim.
fn format_created_date(timestamp: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = timestamp.trim();
    let mut parts = date.splitn(3, '-');
    let (Some(year), Some(month), Some(rest)) = (parts.next(), parts.next(), parts.next()) else {
        return date.to_string();
    };
    let Ok(month_num) = month.parse::<usize>() else {
        return date.to_string();
    };
    // The day part carries the time suffix (`01T10:00:00.000Z`) — digits only.
    let day_digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let Ok(day_num) = day_digits.parse::<u32>() else {
        return date.to_string();
    };
    if !(1..=12).contains(&month_num) || !(1..=31).contains(&day_num) {
        return date.to_string();
    }
    format!("{} {day_num}, {year}", MONTHS[month_num - 1])
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
    fn created_dates_read_like_the_web_table() {
        assert_eq!(
            format_created_date("2026-07-01T10:00:00.000Z"),
            "Jul 1, 2026"
        );
        assert_eq!(format_created_date("2025-12-31T23:59:59Z"), "Dec 31, 2025");
        // Postgres space form and bare dates degrade gracefully too.
        assert_eq!(format_created_date("2026-01-05 08:00:00+00"), "Jan 5, 2026");
        assert_eq!(format_created_date("2026-03-09"), "Mar 9, 2026");
        // Garbage echoes back instead of panicking or lying.
        assert_eq!(format_created_date("not-a-date"), "not-a-date");
        assert_eq!(format_created_date(""), "");
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
