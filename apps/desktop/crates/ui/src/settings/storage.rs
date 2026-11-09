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
//! billing probe (self-hosted quirks) just drops the bar, the
//! table never blocks on it, and the self-hosted `unlimited` plan renders no
//! bar at all (web parity: `plan !== 'unlimited'`).

use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window};
use gpui_component::{button::ButtonVariant, ActiveTheme as _};
use serde_json::{json, Value};

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

use super::doc::{and, empty_state, failed_state, hex, mount, not, path, skeleton, IdeToasts, Ink, SM, XS};

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
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on open or after a write fails quietly.
    retry_pressed: bool,
    /// #EXP-1629: the pane's document and the binding its projection rides.
    view: Entity<SurfaceView>,
    binding: Entity<Binding<Value>>,
    /// The pictures' team (`settings_bench`).
    #[cfg(test)]
    shot_team: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl StoragePane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The list itself is a server read; the issue-identifier and
        // uploaded-by joins read the synced issues/users collections, so a
        // sync delta re-renders the rows.
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];
        // The actions leave the binding for the pane: a delete or a sweep
        // confirms in a native alert, a preview opens over the window.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["retry", "sweep", "delete", "issue", "preview"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| match name {
                                "retry" => pane.retry_load(cx),
                                "sweep" => {
                                    let candidates = match &pane.load {
                                        Load::Ready(Loaded { list: Ok(list), .. }) => list.attachments.iter().filter(|row| row.is_image && !row.referenced).count(),
                                        _ => 0,
                                    };
                                    if candidates > 0 && !pane.busy {
                                        pane.confirm_sweep(candidates, window, cx);
                                    }
                                }
                                _ => pane.row_action(name, &key, window, cx),
                            });
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let (track, list_hover) = (hex(theme.muted.opacity(0.35)), hex(theme.list_hover));
        let view = mount("settings-storage", document(&Ink::of(cx), &track, &list_hover), &binding, window, cx);
        Self {
            nav,
            load: Load::Idle,
            loaded_team: None,
            account_id: None,
            generation: 0,
            busy: false,
            retry_pressed: false,
            view,
            binding,
            #[cfg(test)]
            shot_team: None,
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
                        .map_err(|err| err.user_message());
                    Loaded { list, plan }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                // FEED-79: a failed load = the empty state + Retry; only a
                // Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.retry_pressed);
                if let Err(message) = &result.list {
                    crate::action_error::failed_load(
                        "the attachments",
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

    /// Refetch after a mutation: drop the cached list; the next render
    /// re-fetches.
    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    /// The failed list's Retry: the same load, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.refetch(cx);
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
                    let note = crate::action_error::failed_toast(
                        "delete",
                        &filename,
                        crate::action_error::action_error_description(&error),
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
                        crate::action_error::failed_toast(
                            "sweep",
                            "unreferenced images",
                            crate::action_error::action_error_description(&error),
                        )
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

    /// The team the pane shows (the pictures pin one: the headless store
    /// syncs none).
    fn team_id(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(team) = &self.shot_team {
            return Some(team.clone());
        }
        active_team_id(&self.nav, cx)
    }

    /// The pane as the document binds it; the issue identifier and the
    /// uploader join from the synced issues/users collections — a trashed
    /// board's issues are out of sync and widget screenshots have no
    /// uploader, so those cells read a dash (web parity).
    fn project(&self, team: bool, cx: &App) -> Value {
        let mut out = json!({"noTeam": !team, "loading": false, "failed": false, "ready": false, "busy": self.busy, "empty": false, "rows": [], "hasMeter": false, "meter": Value::Null, "summary": "", "candidates": 0});
        if !team {
            return out;
        }
        match &self.load {
            Load::Idle | Load::Loading => out["loading"] = json!(true),
            Load::Ready(Loaded { list: Err(_), .. }) => out["failed"] = json!(true),
            Load::Ready(Loaded { list: Ok(list), plan }) => {
                out["ready"] = json!(true);
                // Web `UsageBar`: shown for every billed plan (`plan !==
                // 'unlimited'`); a null storage limit reads "… / unlimited"
                // with no track. Self-hosted (`unlimited`) and a failed
                // billing probe render no meter at all — the totalBytes
                // summary below always carries the usage.
                if let Some(plan) = plan.as_ref().filter(|plan| plan.plan != "unlimited") {
                    let current = super::format_storage(plan.usage.storage_mb);
                    let amount = match plan.limits.storage_mb.map(super::format_storage) {
                        Some(limit) => format!("{current} / {limit}"),
                        None => format!("{current} / unlimited"),
                    };
                    let fraction = plan.limits.storage_mb.filter(|limit| *limit > 0.).map(|limit| (plan.usage.storage_mb / limit).clamp(0., 1.));
                    out["hasMeter"] = json!(true);
                    out["meter"] = json!({"amount": amount, "track": fraction.is_some(), "width": format!("{}%", fraction.unwrap_or(0.) * 100.)});
                }
                let rows = &list.attachments;
                // EXP-878 / EXP-1557: draft files are private and never
                // listed, but they are the team's bytes — the summary names
                // them like web's "(incl. … in unsaved drafts)".
                let drafts = (list.draft_bytes > 0).then(|| format!(" (incl. {} in unsaved drafts)", format_bytes(list.draft_bytes))).unwrap_or_default();
                let plural = if rows.len() == 1 { "" } else { "s" };
                out["summary"] = json!(format!("{} attachment{plural} · {}{drafts}", rows.len(), format_bytes(list.total_bytes)));
                out["empty"] = json!(rows.is_empty());
                out["candidates"] = json!(rows.iter().filter(|row| row.is_image && !row.referenced).count());
                let collections = Store::global(cx).collections();
                let issues = collections.issues.read(cx);
                let users = collections.users.read(cx);
                out["rows"] = rows
                    .iter()
                    .map(|row| {
                        let identifier = issues.get(&row.issue_id).map(|issue| format!("#{}", issue.identifier));
                        // Web parity: `uploader?.name || uploader?.email || —`.
                        let uploader = row
                            .uploader_id
                            .as_deref()
                            .and_then(|id| users.get(id))
                            .and_then(|user| user.name.clone().filter(|name| !name.is_empty()).or_else(|| user.email.clone().filter(|email| !email.is_empty())));
                        // Image filenames open the in-app lightbox (EXP-316),
                        // markdown ones the in-app preview (EXP-1003).
                        let markdown = is_markdown_attachment(Some(&row.content_type), Some(&row.filename));
                        let previewable = (row.is_image || markdown) && queries::absolute_api_url(cx, &format!("/api/attachments/{}", row.id)).is_some();
                        json!({
                            "key": row.id,
                            "icon": icon_name(Some(&row.content_type)),
                            "filename": row.filename,
                            "deleteLabel": format!("Delete {}", row.filename),
                            "previewable": previewable,
                            "size": format_bytes(row.size_bytes),
                            "hasIssue": identifier.is_some(),
                            "identifier": identifier.unwrap_or_default(),
                            "author": uploader.unwrap_or_else(|| "—".to_string()),
                            "date": format_created_date(&row.created_at),
                            "status": attachment_status(row.is_image, &row.content_type, row.referenced),
                        })
                    })
                    .collect();
            }
        }
        let candidates = out["candidates"].as_u64().unwrap_or(0);
        out["sweepLabel"] = json!(if candidates > 0 { format!("Sweep unreferenced images ({candidates})") } else { "Sweep unreferenced images".to_string() });
        out["sweepDisabled"] = json!(candidates == 0 || self.busy);
        out
    }

    /// A row action (`preview`, `issue`, `delete`) on the attachment `key`.
    fn row_action(&mut self, name: &str, key: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Load::Ready(Loaded { list: Ok(list), .. }) = &self.load else { return };
        let Some(row) = list.attachments.iter().find(|row| row.id == key).cloned() else { return };
        match name {
            "delete" if !self.busy => self.confirm_delete(&row, window, cx),
            // The owning issue's pill leaves Settings for its detail screen.
            "issue" => crate::navigation::navigate(window, cx, crate::navigation::Screen::IssueDetail { issue_id: row.issue_id.clone() }),
            "preview" => {
                let url = format!("/api/attachments/{}", row.id);
                if is_markdown_attachment(Some(&row.content_type), Some(&row.filename)) {
                    let target = MarkdownPreviewTarget { attachment_id: row.id.clone(), filename: row.filename.clone(), size_bytes: row.size_bytes, team_id: active_team_id(&self.nav, cx) };
                    open_markdown_preview(target, window, cx);
                } else {
                    crate::image_preview::open_image_preview(url, row.filename.clone(), None, None, window, cx);
                }
            }
            _ => {}
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        Self::new(nav, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, load: ShotLoad, cx: &mut gpui::Context<Self>) {
        self.shot_team = Some("team-shots".into());
        self.loaded_team = self.shot_team.clone();
        self.account_id = Store::global(cx).session(cx).account_id().map(str::to_string);
        self.load = Load::Ready(match load {
            ShotLoad::Ready(list, plan) => Loaded { list: Ok(list), plan },
            ShotLoad::Failed => Loaded { list: Err("offline".into()), plan: None },
        });
        cx.notify();
    }
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) enum ShotLoad {
    Ready(AttachmentsListForTeamOutput, Option<TeamPlanOut>),
    Failed,
}

/// [`icon_for_content_type`]'s glyph by name (the document's `Icon`).
fn icon_name(content_type: Option<&str>) -> &'static str {
    match icon_for_content_type(content_type) {
        ExpIcon::Image => "image",
        ExpIcon::Play => "play",
        ExpIcon::FileText => "file-text",
        ExpIcon::Package => "package",
        _ => "file",
    }
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The pane's tree: the band with the Sweep pill, the usage meter, the
/// summary line, then the attachment ladder (EXP-1076: an attachment is an
/// ENTITY, so flat gapless rows under the band, each a fixed GRID of cells
/// — every column but the filename carries a definite width so the columns
/// line up down the list) or the skeleton / failed / empty states.
fn document(ink: &Ink, track: &str, list_hover: &str) -> Value {
    let sweep = ink.pill_button("sweep", Some("ui-clean"), path("/pane/sweepLabel"), path("/pane/sweepDisabled"), json!({"name": "sweep"}));
    let sweep = json!({"id": "sweep_slot", "component": "Box", "style": {"display": "flex", "flexShrink": 0}, "children": sweep});
    let band = ink.band("band", "Storage", Some(sweep));

    // Web `UsageBar` (`settings::usage_bar`): label left, "current / limit"
    // right (`gap_1` over a 6px capsule track).
    let meter = json!({"id": "meter", "component": "Box", "visible": path("/pane/hasMeter"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25)},
        "children": [
            {"id": "meter_head", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between"},
                "children": [ink.text("meter_label", json!("Attachment storage"), SM, &ink.foreground), ink.text("meter_amount", path("/pane/meter/amount"), XS, &ink.muted)]},
            {"id": "meter_track", "component": "Box", "visible": path("/pane/meter/track"), "style": {"height": 6, "width": "100%", "borderRadius": 9999, "backgroundColor": track},
                "children": [{"id": "meter_fill", "component": "Box", "style": {"height": "100%", "width": path("/pane/meter/width"), "borderRadius": 9999, "backgroundColor": ink.primary}}]},
        ]});
    let mut summary = ink.text("summary", path("/pane/summary"), SM, &ink.muted);
    summary["visible"] = path("/pane/ready");
    summary["style"]["width"] = json!("100%");
    summary["style"]["minWidth"] = json!(0);

    let cell = |id: &str, width: f32, child: Value| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": width, "flexShrink": 0}, "children": [child]});
    let muted = |id: &str, text: Value, width: f32| {
        let mut t = ink.text(id, text, XS, &ink.muted);
        t["props"]["lines"] = json!(1);
        t["style"]["width"] = json!(width);
        t["style"]["flexShrink"] = json!(0);
        t
    };
    let filename = |id: &str, press: bool| {
        let mut t = ink.text(id, path("filename"), SM, &ink.foreground);
        t["props"]["lines"] = json!(1);
        t["style"]["minWidth"] = json!(0);
        t["style"]["flexShrink"] = json!(1);
        if press {
            t["style"]["cursor"] = json!("pointer");
            t["style"][":hover"] = json!({"backgroundColor": ink.fill_active});
            json!({"id": format!("{id}_press"), "component": "Box", "visible": path("previewable"), "props": {"pressable": true}, "accessibility": {"label": path("filename")}, "on": {"press": {"event": {"name": "preview", "context": {"key": path("key")}}}}, "style": {"display": "flex", "minWidth": 0, "flexShrink": 1}, "children": [t]})
        } else {
            t["visible"] = not(path("previewable"));
            t
        }
    };
    let mut dash = ink.text("row_issue_none", json!("—"), XS, &ink.muted);
    dash["visible"] = not(path("hasIssue"));
    let mut issue = ink.pill("row_issue_pill", path("identifier"), true, Some(json!({"name": "issue", "context": {"key": path("key")}})));
    issue["visible"] = path("hasIssue");
    let row = json!({"id": "row", "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [
            {"id": "row_lead", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "gap": ink.r(0.5)},
                "children": [
                    {"id": "row_icon", "component": "Icon", "props": {"name": path("icon")}, "style": {"width": ink.r(0.75), "height": ink.r(0.75), "color": ink.muted, "flexShrink": 0}},
                    filename("row_file", true),
                    filename("row_file_plain", false),
                ]},
            muted("row_size", path("size"), COL_SIZE),
            {"id": "row_issue", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": COL_ISSUE, "flexShrink": 0}, "children": [issue, dash]},
            muted("row_author", path("author"), COL_AUTHOR),
            muted("row_date", path("date"), COL_DATE),
            cell("row_status", COL_STATUS, ink.pill("row_status_pill", path("status"), false, None)),
            // EXP-862: a row's trash is a GHOST glyph, never a circle.
            // a11y: named by the caller ("Delete <filename>", the node's accessibility label).
            ink.ghost_icon_button("row_delete", "trash-2", path("deleteLabel"), json!({"name": "delete", "context": {"key": path("key")}}), list_hover),
        ]});
    let rows = json!({"id": "rows", "component": "List", "visible": and(path("/pane/ready"), not(path("/pane/empty"))), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]});

    let section = json!({"id": "section", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            band,
            skeleton("skeleton", path("/pane/loading")),
            // A failed load (FEED-79): the empty state + Retry, no toast.
            failed_state(ink, "failed", "ui-attach", crate::empty_state_copy::ATTACHMENTS, path("/pane/failed")),
            meter,
            summary,
            empty_state(ink, "empty", "ui-attach", crate::empty_state_copy::ATTACHMENTS, and(path("/pane/ready"), path("/pane/empty"))),
            rows,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [section, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

impl Render for StoragePane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team = self.team_id(cx);
        if let Some(team_id) = &team {
            self.ensure_loaded(team_id, cx);
        }
        let projection = self.project(team.is_some(), cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

// EXP-698 — the attachment table's fixed COLUMN ladder. Only the filename
// cell flexes; every other cell is a definite width so the columns line up
// down the list. The widths are the widest realistic content plus a little
// air: "403.2 KB", "#APP-123", "Aug 30, 2026", "Unreferenced".
/// Size cell.
const COL_SIZE: f32 = 80.;
/// The `#IDENT` chip cell.
const COL_ISSUE: f32 = 120.;
/// Uploader cell (ellipsised).
const COL_AUTHOR: f32 = 120.;
/// Created-date cell.
const COL_DATE: f32 = 90.;
/// Status-chip cell — fixed so the trailing delete button never zig-zags
/// with the chip's label length.
const COL_STATUS: f32 = 104.;

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
