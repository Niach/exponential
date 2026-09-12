//! "My devices" (EXP-403): the signed-in user's registered devices —
//! desktops and headless `exponential` daemon servers — rendered as a
//! section on the Devices page, so all four clients surface the same device
//! registry.
//!
//! The rows come from the SYNCED `devices` shape and nothing else (EXP-485):
//! online-ness is `last_seen_at` freshness against the contract window — no
//! relay presence involved — and every mutation lands back through sync, so
//! there is no poll and no fallback list to keep in step.
//!
//! EXP-832: the shape deltas land every few seconds (one per heartbeat of
//! every device the caller can see), so the rows are DERIVED on the observer
//! and cached — jsonb account maps parsed once per change, not once per
//! frame — and a derivation equal to the one on screen never notifies.
//!
//! The one tRPC call left is `devices.latestVersions`, the informational
//! `CLIENT_LATEST_VERSION_*` pair behind the amber update nudge. That is
//! instance config, not device state: it is fetched ONCE per section
//! lifetime (the first render) rather than polled.
//!
//! EXP-849/EXP-862: this is the SETUP surface for agent logins. A row carries
//! the worst health among the accounts its device reported and one chip per
//! account, whose menu is the ×4 rule (`usage_bar::chip_actions`): sign in,
//! set as default (`agent_profile_use`: a device-local pointer, never a logout
//! and never a credential copy), remove this device's copy of the login
//! (`agent_profile_remove`). The chip's BADGE is the only signed-out notice on
//! the row — EXP-862 retired the status-line annotations and the "Sign in to
//! <agent>" pill that said the same thing twice. Deciding WHICH account a run
//! should spend is the Accounts section's job (`accounts_section`).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, ClipboardItem, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};

use crate::accounts_section::DEVICE_SETTINGS;
use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::queries;
use crate::usage_bar::{chip_actions, ChipAction, DeviceAccountChip};

/// EXP-832: how often the section re-derives its rows without a shape delta.
/// Online-ness is a CLOCK question (`last_seen_at` against the contract
/// window), so a device that simply stops beating has to age out on a tick;
/// the derivation is compared before it notifies, so an unchanged list costs
/// one pass and no frame.
const LIVENESS_TICK: std::time::Duration = std::time::Duration::from_secs(10);

/// EXP-832 — one device row, fully derived: every string the row renders and
/// the account chips already parsed out of the row's `agent_accounts` jsonb.
/// `PartialEq` is what keeps a heartbeat that changed nothing off the frame.
#[derive(Clone, Debug, PartialEq)]
struct DeviceCard {
    /// The `devices` ROW id — what the Device settings dialog is keyed by.
    row_id: String,
    device_id: String,
    /// The name, falling back to the id for a device that never sent one.
    label: SharedString,
    /// The headless daemon (the only kind that self-updates).
    server: bool,
    online: bool,
    /// This very install.
    own: bool,
    /// Mine, as opposed to a teammate's shared server.
    mine: bool,
    /// EXP-525: a teammate's row keeps the attribution in its tooltip.
    owner_tooltip: Option<SharedString>,
    /// An OWN device shared with at least one team.
    shared: bool,
    is_default: bool,
    version: Option<String>,
    update_requested: bool,
    update_blocked: bool,
    active_sessions: u32,
    last_seen_at: Option<String>,
    /// Runnable agents (signed in) and installed-but-signed-out ones — the ▶
    /// gate reads both.
    agents: Vec<String>,
    unauthed_agents: Vec<String>,
    caps: Vec<String>,
    /// EXP-849: the accounts this device holds, one chip each.
    chips: Vec<DeviceAccountChip>,
    /// The WORST health among them — the row's badge.
    health: coding::agent_accounts::Health,
}

impl DeviceCard {
    fn has_cap(&self, cap: &str) -> bool {
        self.caps.iter().any(|have| have == cap)
    }

    /// The same gate the chips' menus apply: mine, and either this very
    /// install (the login runs in a tab right here) or an online device on a
    /// build that takes the command.
    fn actionable(&self) -> bool {
        self.mine && (self.own || (self.online && self.has_cap("agent-login")))
    }
}

/// The two groups the section renders, in the EXP-623 stable order.
#[derive(Clone, Debug, Default, PartialEq)]
struct DerivedDevices {
    mine: Vec<DeviceCard>,
    team: Vec<DeviceCard>,
}

pub(crate) struct MachinesSection {
    latest: api::devices::LatestVersions,
    /// The one-shot guard for [`Self::ensure_latest_loaded`].
    latest_requested: bool,
    /// Device with an in-flight `requestUpdate` — holds "Updating…" until the
    /// synced row carries the server's own `update_requested_at`.
    updating: Option<String>,
    /// EXP-832: the rows on screen. `None` while the shape is still Waiting.
    derived: Option<DerivedDevices>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl MachinesSection {
    pub(crate) fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The rows stream over the devices shape — re-derive on every delta,
        // and notify only when the derivation actually moved.
        let devices_collection = sync::Store::global(cx).collections().devices.clone();
        let subscriptions = vec![cx.observe(&devices_collection, |this: &mut Self, _, cx| {
            this.refresh_cards(cx);
        })];
        // Online-ness ages out on a clock, not on a delta.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(LIVENESS_TICK).await;
            if this.update(cx, |this, cx| this.refresh_cards(cx)).is_err() {
                return;
            }
        })
        .detach();
        Self {
            latest: api::devices::LatestVersions::default(),
            latest_requested: false,
            updating: None,
            derived: None,
            _subscriptions: subscriptions,
        }
    }

    // -- data ----------------------------------------------------------------

    /// EXP-832: re-derive the rows, and notify only if they CHANGED. Every
    /// heartbeat of every visible device is a delta, so the equality check is
    /// what keeps the page from repainting a dozen times a minute.
    fn refresh_cards(&mut self, cx: &mut gpui::Context<Self>) {
        let derived = Self::derive(cx);
        if derived == self.derived {
            return;
        }
        self.derived = derived;
        cx.notify();
    }

    /// Fetch `devices.latestVersions` once per section lifetime. The values
    /// are instance config (they change on a deploy, not on a heartbeat), so
    /// a failure simply means no update nudge until the section is rebuilt —
    /// far better than a render-driven retry storm.
    fn ensure_latest_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.latest_requested {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            // No client yet — try again on a later render.
            return;
        };
        self.latest_requested = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::latest_versions(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(latest) => this.latest = latest,
                    Err(err) => log::warn!("[ui] devices.latestVersions failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Run a `devices.*` mutation on the background executor. There is no
    /// refetch: every one of these writes a row the `devices` shape streams
    /// back as a delta.
    fn mutate(
        &mut self,
        what: &'static str,
        op: impl FnOnce(&api::TrpcClient) -> Result<(), api::ApiError> + Send + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { op(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    log::warn!("[ui] {what} failed: {err}");
                }
                this.updating = None;
                cx.notify();
            });
        })
        .detach();
    }

    fn remove(&mut self, device_id: String, cx: &mut gpui::Context<Self>) {
        self.mutate(
            "devices.remove",
            move |trpc| api::devices::remove(trpc, &device_id),
            cx,
        );
    }

    fn request_update(&mut self, device_id: String, cx: &mut gpui::Context<Self>) {
        if self.updating.is_some() {
            return;
        }
        self.updating = Some(device_id.clone());
        cx.notify();
        self.mutate(
            "devices.requestUpdate",
            move |trpc| api::devices::request_update(trpc, &device_id, false),
            cx,
        );
    }

    /// FEED-36: "Update now…" — a queued update is parked behind live
    /// sessions; this ends them (the daemon's `update_now` command) so the
    /// update applies right away. Destructive for the sessions, so it
    /// confirms first (web `MyMachines` twin).
    fn prompt_update_now(
        &mut self,
        device_id: String,
        label: String,
        live_sessions: u32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let section = cx.entity().downgrade();
        let spec = AlertSpec::new(
            format!("Update \"{label}\" now?"),
            format!(
                "Ends the {live_sessions} live session(s) on this machine (repo-backed runs can be \
                 resumed from their session page) and restarts it on the new version."
            ),
            "Update now",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(section) = section.upgrade() {
                let device_id = device_id.clone();
                section.update(cx, |this, cx| {
                    this.mutate(
                        "devices.requestUpdate",
                        move |trpc| api::devices::request_update(trpc, &device_id, true),
                        cx,
                    );
                });
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- dialogs -------------------------------------------------------------

    /// Remove behind a confirm — destructive native actions confirm first.
    fn prompt_remove(
        &mut self,
        device_id: String,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let section = cx.entity().downgrade();
        let spec = AlertSpec::new(
            format!("Remove \"{label}\"?"),
            "The machine drops off this list. One still running the daemon \
             re-registers itself on its next heartbeat.",
            "Remove",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(section) = section.upgrade() {
                section.update(cx, |this, cx| this.remove(device_id.clone(), cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// EXP-481/EXP-832: the rows from the SYNCED devices shape as
    /// [`DeviceCard`]s — `(mine, team)`, each group in the EXP-623 stable
    /// order (online-by-label first, so heartbeats can't reorder the list;
    /// offline rows don't beat, so last-seen desc is stable there). EXP-642
    /// keeps the two groups apart: they render as their own headed sections,
    /// web parity. `None` while the shape is Waiting (cold start / old
    /// server) — the section renders its loading note.
    fn derive(cx: &mut App) -> Option<DerivedDevices> {
        // Before the collections are borrowed: resolving it caches a global.
        let own_device_id = crate::queries::own_device_id(cx);
        let collections = sync::Store::global(cx).collections();
        let devices = collections.devices.read(cx);
        if !devices.is_ready() {
            return None;
        }
        // The USER id, not the account key: `devices.user_id` is the Better Auth
        // user id, `Account.id` is the per-instance account key (EXP-642 — the
        // old comparison never matched, which the My/Team split made visible).
        let me = crate::queries::active_account(cx)?.user_id;
        let users = collections.users.read(cx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut derived = DerivedDevices::default();
        for row in devices.iter() {
            let owned = row.user_id.as_deref() == Some(me.as_str());
            let owner_tooltip = (!owned)
                .then(|| {
                    row.user_id
                        .as_deref()
                        .and_then(|user_id| users.get(user_id))
                        .and_then(|user| user.name.clone())
                })
                .flatten()
                .filter(|name| !name.is_empty())
                .map(|name| SharedString::from(format!("Shared by {name}")));
            let device_id = row.device_id.clone().unwrap_or_default();
            let label = row.label.clone().unwrap_or_default();
            let update_requested = row.update_requested_at.is_some();
            let accounts = crate::device_settings::parse_agent_map::<coding::AgentAccount>(
                row.agent_accounts.as_ref(),
            );
            let card = DeviceCard {
                row_id: row.id.clone(),
                label: SharedString::from(if label.trim().is_empty() {
                    device_id.clone()
                } else {
                    label
                }),
                own: device_id == own_device_id,
                mine: owned,
                owner_tooltip,
                server: row.is_server(),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                last_seen_at: row.last_seen_at.clone(),
                version: row.version.clone(),
                update_requested,
                update_blocked: update_requested && row.active_sessions.unwrap_or(0) > 0,
                active_sessions: row.active_sessions.unwrap_or(0).max(0) as u32,
                // EXP-642: only an OWN row wears the chip — the Team devices
                // section is shared by definition.
                shared: owned && !row.shared_team_ids.is_empty(),
                // EXP-622: a teammate's flag is THEIR preference, never ours.
                is_default: owned && row.is_default.unwrap_or(false),
                agents: row.agent_ids(),
                unauthed_agents: row.unauthed_agent_ids(),
                caps: row.cap_ids(),
                health: coding::agent_accounts::worst_health(&accounts),
                chips: crate::usage_bar::device_account_chips(&accounts),
                device_id,
            };
            if owned {
                derived.mine.push(card);
            } else {
                derived.team.push(card);
            }
        }
        // Online first, then by label so heartbeats can't reorder the online
        // group; offline rows don't beat, so last-seen desc (ISO stamps order
        // lexicographically) is stable there.
        let stable_order = |a: &DeviceCard, b: &DeviceCard| {
            b.online
                .cmp(&a.online)
                .then_with(|| {
                    if a.online {
                        std::cmp::Ordering::Equal
                    } else {
                        b.last_seen_at.cmp(&a.last_seen_at)
                    }
                })
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
                .then_with(|| a.device_id.cmp(&b.device_id))
        };
        derived.mine.sort_by(stable_order);
        derived.team.sort_by(stable_order);
        Some(derived)
    }

    // -- render --------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        device: &DeviceCard,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let label = device.label.clone();
        let kind_icon = if device.server {
            registry::UI_SERVER
        } else {
            registry::UI_DEVICE
        };
        // Informational only: the row nudges, the device decides.
        let latest = if device.server {
            self.latest.cli.as_deref()
        } else {
            self.latest.desktop.as_deref()
        };
        let outdated = update_available(device.version.as_deref(), latest);
        let updating =
            device.update_requested || self.updating.as_deref() == Some(&device.device_id);
        // EXP-411: the request is parked behind live sessions on the device —
        // "Update queued" instead of an indefinite "Updating…".
        let queued = device.update_requested && device.update_blocked;
        // A teammate's shared row has nothing here to rename, remove or update.
        let menu = device.mine.then(|| {
            let section = cx.entity().downgrade();
            let device_id = device.device_id.clone();
            let row_id = device.row_id.clone();
            let menu_label = label.clone();
            // EXP-420: offer the update only when a newer CLI version really
            // exists (or one is already in flight — keep its state visible).
            let can_update = device.server && device.online && (outdated || updating);
            // FEED-36: the parked update can be forced on a build that runs
            // `update_now` — it ends the sessions holding it.
            let can_update_now = queued && device.has_cap("update-now");
            let live_sessions = device.active_sessions;
            // EXP-862: a row action is a GHOST glyph — the circle is for the
            // primary ▶ beside it.
            crate::controls::ghost_icon_button(
                ("machine-menu", index),
                Icon::new(registry::UI_MORE),
                cx,
            )
            .dropdown_menu(move |menu, _window, cx| {
                let remove_section = section.clone();
                let remove_id = device_id.clone();
                let remove_label = menu_label.clone();
                let update_section = section.clone();
                let update_id = device_id.clone();
                let update_now_section = section.clone();
                let update_now_id = device_id.clone();
                let update_now_label = menu_label.clone();
                let settings_row_id = row_id.clone();
                // EXP-481: rename and sharing live INSIDE the Device settings
                // dialog (with the defaults editor and the worktree list);
                // EXP-862 renamed the entry from "Edit…" and swapped the
                // pencil for the settings gear ×4.
                menu.item(
                    PopupMenuItem::new(DEVICE_SETTINGS)
                        .icon(Icon::new(registry::NAV_SETTINGS))
                        .on_click(move |_, window, cx| {
                            crate::device_settings::open(window, cx, settings_row_id.clone());
                        }),
                )
                .when(can_update, |menu| {
                    menu.item(
                        PopupMenuItem::new(if queued {
                            "Update queued"
                        } else if updating {
                            "Updating…"
                        } else {
                            "Update"
                        })
                        .icon(Icon::new(registry::UI_UPDATE))
                        .disabled(updating)
                        .on_click(move |_, _window, cx| {
                            let Some(section) = update_section.upgrade() else {
                                return;
                            };
                            let id = update_id.clone();
                            section.update(cx, |this, cx| this.request_update(id, cx));
                        }),
                    )
                })
                .when(can_update_now, |menu| {
                    menu.item(
                        crate::controls::danger_menu_item(
                            "Update now…",
                            Icon::new(registry::UI_UPDATE),
                            cx,
                        )
                        .on_click(move |_, window, cx| {
                            let Some(section) = update_now_section.upgrade() else {
                                return;
                            };
                            let id = update_now_id.clone();
                            let label = update_now_label.to_string();
                            section.update(cx, |this, cx| {
                                this.prompt_update_now(id, label, live_sessions, window, cx);
                            });
                        }),
                    )
                })
                .item(
                    crate::controls::danger_menu_item(
                        "Remove…",
                        Icon::new(registry::UI_DELETE),
                        cx,
                    )
                    .on_click(move |_, window, cx| {
                        let Some(section) = remove_section.upgrade() else {
                            return;
                        };
                        let id = remove_id.clone();
                        let label = remove_label.to_string();
                        section.update(cx, |this, cx| {
                            this.prompt_remove(id, label, window, cx);
                        });
                    }),
                )
            })
        });

        // EXP-615: the web row's ▶ Start-coding button, icon-only. EXP-696:
        // the composer opens with THIS row's device preselected, so the local
        // doctor gates the row only for this install — another device is gated
        // on its OWN advertisement, which is what keeps the button honest.
        let no_agent = match device.own {
            true => crate::coding_flow::no_agent_reason(cx),
            false => remote_start_reason(device).map(SharedString::from),
        };
        let start_device_id = device.device_id.clone();
        // EXP-686: the shared round glass affordance (web/mobile parity) —
        // the same shape the action rows' ▶ Run carries.
        let start_coding = crate::controls::glass_icon_button(
            ("machine-start-coding", index),
            Icon::new(registry::ACTION_RUN),
            cx,
        )
        .tooltip(no_agent.clone().unwrap_or_else(|| "Start coding".into()))
        .disabled(no_agent.is_some())
        .on_click(
            move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut gpui::App| {
                // EXP-825: the composer opens with THIS device preselected.
                crate::navigation::navigate_to_chat(
                    window,
                    cx,
                    crate::navigation::ChatSeed::device(start_device_id.clone()),
                );
            },
        );

        // EXP-642: one row per device, the web `GlassRow` two-line shape —
        // icon · (name · version · default star · "Shared") over the status
        // line · ▶ · ⋯ — `min_w_0` down the name side so only the NAME gives
        // way.
        let row_hover = theme.list_hover;
        crate::surface::flat_row()
            .id(SharedString::from(format!("machine-{}", device.device_id)))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            .cursor_pointer()
            .hover(move |this| this.bg(row_hover))
            .child(
                div()
                    .flex_shrink_0()
                    .child(Icon::new(kind_icon).xsmall().text_color(muted)),
            )
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        gpui_component::h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_1p5()
                            // Web keeps the version ADJACENT to the name, so the name
                            // is fit-content — a `truncate` here would render at the
                            // EXP-175 collapsed width (it needs `flex_1` on the
                            // ellipsis div itself); the cluster clips instead.
                            .overflow_hidden()
                            .child(
                                div()
                                    .id(("machine-name", index))
                                    .flex_shrink_0()
                                    .text_sm()
                                    .whitespace_nowrap()
                                    .text_color(theme.foreground)
                                    .when_some(device.owner_tooltip.clone(), |this, owner| {
                                        this.tooltip(move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(owner.clone())
                                                .build(window, cx)
                                        })
                                    })
                                    .child(label.clone()),
                            )
                            // EXP-696: the device this app IS — the same muted
                            // caption the "Shared" chip uses, so the row line
                            // keeps one rhythm.
                            .when(device.own, |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded(px(theme::tokens::radius::SM))
                                        .border_1()
                                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                        .text_xs()
                                        .text_color(muted)
                                        .child("This device"),
                                )
                            })
                            // EXP-622: the device every picker prefills.
                            .when(device.is_default, |this| {
                                this.child(
                                    Icon::new(registry::UI_DEVICE_DEFAULT)
                                        .xsmall()
                                        .flex_shrink_0()
                                        .text_color(muted),
                                )
                            })
                            .when_some(device.version.clone(), |this, version| {
                                // Stateful: the outdated hint rides a tooltip, and
                                // `tooltip` lives on gpui's STATEFUL interactive trait.
                                let hint: SharedString =
                                    format!("Update available: v{}", latest.unwrap_or_default())
                                        .into();
                                this.child(
                                    div()
                                        .id(("machine-version", index))
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(if outdated {
                                            theme::tokens::YELLOW.to_hsla()
                                        } else {
                                            muted
                                        })
                                        .when(outdated, |this| {
                                            this.tooltip(move |window, cx| {
                                                gpui_component::tooltip::Tooltip::new(hint.clone())
                                                    .build(window, cx)
                                            })
                                        })
                                        .child(SharedString::from(format!("v{version}"))),
                                )
                            })
                            // EXP-642 (web parity): an own device shared with a team
                            // says so on the name line.
                            .when(device.shared, |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded(px(theme::tokens::radius::SM))
                                        .border_1()
                                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                        .text_xs()
                                        .text_color(muted)
                                        .child("Shared"),
                                )
                            }),
                    )
                    .child(
                        gpui_component::h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_color(muted)
                            .when(device.online, |this| {
                                this.child(
                                    div()
                                        .size_1p5()
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(theme::tokens::GREEN.to_hsla()),
                                )
                            })
                            .child(SharedString::from(status_line(device)))
                            // EXP-849: the WORST health among the device's
                            // logins — a revoked credential leaves the device
                            // looking fine while every run on it fails at the
                            // first request. One badge, the ×4 strings; the
                            // repair is in the chip's own menu (EXP-862).
                            .children(crate::usage_bar::health_badge(device.health, cx))
                            .when(updating, |this| {
                                this.child(div().child(if queued {
                                    "Update queued"
                                } else {
                                    "Updating…"
                                }))
                            }),
                    )
                    // EXP-849: the accounts this DEVICE holds, one chip each.
                    .children(Self::render_account_chips(index, device, cx)),
            )
            // EXP-698: a FIXED trailing COLUMN — ▶ and ⋯ each own a 32px
            // slot, so the two actions line up down the list. A row without a
            // menu (a teammate's shared device) keeps an empty placeholder
            // instead of sliding its ▶ under the ⋯ column.
            .child(
                gpui_component::h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(start_coding)
                    .child(match menu {
                        Some(menu) => div().flex_shrink_0().child(menu),
                        None => div()
                            .flex_shrink_0()
                            .w(px(theme::tokens::size::CONTROL_MD)),
                    }),
            )
            .into_any_element()
    }

    /// EXP-849/EXP-862 — the account chips on one Devices row and the menu
    /// each carries: the ×4 rule ([`chip_actions`]) and nothing else. A login
    /// that is missing or broken is signed in again (the agent CLI's own
    /// device-code flow — in a tab here for THIS device, as an `agent_login`
    /// command for another of mine), a healthy one becomes the device's
    /// default (`agent_profile_use`, a device-local pointer) or is removed
    /// from it (`agent_profile_remove`, behind the pinned confirm).
    ///
    /// Never a logout: signing codex out would revoke the account server-wide,
    /// and never a credential copy either — the files stay where the CLI wrote
    /// them. A teammate's shared device is read-only: that login belongs to
    /// its owner.
    fn render_account_chips(
        index: usize,
        device: &DeviceCard,
        cx: &gpui::App,
    ) -> Option<gpui::AnyElement> {
        if device.chips.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let actionable = device.actionable();
        let can_switch = device.own || device.has_cap(coding::doctor::ACCOUNT_SWITCH_CAP);
        let can_remove = device.has_cap(coding::doctor::ACCOUNT_REMOVE_CAP);
        let mut row = gpui_component::h_flex().w_full().min_w_0().flex_wrap().gap_1();
        for (slot, chip) in device.chips.iter().enumerate() {
            let Some(agent) = coding::CodingAgent::parse(&chip.agent) else {
                continue;
            };
            let label = SharedString::from(chip_label(chip, agent));
            let check = (chip.signed_in && chip.active).then(|| {
                Icon::new(registry::UI_CHECK)
                    .with_size(px(crate::surface::PillSize::Sm.glyph()))
                    .text_color(theme::tokens::GREEN.to_hsla())
            });
            let badge = crate::usage_bar::health_badge(chip.health, cx);
            let id = ("machine-account-chip", index * 64 + slot);
            let actions = if actionable {
                chip_actions(
                    chip.signed_in,
                    chip.health,
                    chip.active,
                    &chip.profile_id,
                    can_switch,
                    can_remove,
                )
            } else {
                Vec::new()
            };
            if actions.is_empty() {
                row = row.child(
                    crate::surface::glass_pill(
                        id,
                        crate::surface::PillSize::Sm,
                        crate::surface::PillMode::Readonly,
                        cx,
                    )
                    .when(!device.online, |this| this.text_color(muted))
                    .child(label)
                    .children(check)
                    .children(badge),
                );
                continue;
            }
            let device_id = device.device_id.clone();
            let device_label = device.label.clone();
            let own = device.own;
            let profile_id = chip.profile_id.clone();
            let account_label = SharedString::from(
                chip.email
                    .clone()
                    .or_else(|| chip.plan.clone())
                    .unwrap_or_else(|| chip.profile_label.clone()),
            );
            row = row.child(
                crate::surface::glass_pill_button(id, crate::surface::PillSize::Sm, cx)
                    .when(!device.online, |this| this.text_color(muted))
                    .child(label)
                    .children(check)
                    .children(badge)
                    .dropdown_menu(move |menu, _window, cx| {
                        let mut menu = menu;
                        for action in &actions {
                            let action = *action;
                            let device_id = device_id.clone();
                            let device_label = device_label.clone();
                            let profile_id = profile_id.clone();
                            let account_label = account_label.clone();
                            let click = move |_: &gpui::ClickEvent,
                                              window: &mut Window,
                                              cx: &mut App| match action {
                                ChipAction::SignIn => crate::agent_login::sign_in_on_device(
                                    device_id.clone(),
                                    device_label.clone(),
                                    own,
                                    agent,
                                    coding::agent_login::LoginTarget::Profile(profile_id.clone()),
                                    window,
                                    cx,
                                ),
                                ChipAction::SetDefault => {
                                    crate::accounts_section::use_account_here(
                                        device_id.clone(),
                                        device_label.to_string(),
                                        own,
                                        agent,
                                        profile_id.clone(),
                                        window,
                                        cx,
                                    )
                                }
                                ChipAction::Remove => crate::accounts_section::remove_account(
                                    device_id.clone(),
                                    device_label.clone(),
                                    own,
                                    agent,
                                    profile_id.clone(),
                                    account_label.clone(),
                                    window,
                                    cx,
                                ),
                            };
                            menu = menu.item(match action {
                                ChipAction::Remove => crate::controls::danger_menu_item(
                                    action.label(),
                                    Icon::new(action.icon()),
                                    cx,
                                )
                                .on_click(click),
                                _ => PopupMenuItem::new(action.label())
                                    .icon(Icon::new(action.icon()))
                                    .on_click(click),
                            });
                        }
                        menu
                    }),
            );
        }
        Some(row.into_any_element())
    }
}

/// `Claude Code · dev@acme.test` — a chip's text on a DEVICE row: the agent,
/// then the login (its email, else the bare plan an agent reports instead of
/// an address, else the profile's label). Byte-identical ×4
/// (`machineChipLabel`).
fn chip_label(chip: &DeviceAccountChip, agent: coding::CodingAgent) -> String {
    let who = chip.email.clone().unwrap_or_else(|| {
        if chip.signed_in {
            chip.plan.clone().unwrap_or_else(|| "signed in".to_string())
        } else {
            chip.profile_label.clone()
        }
    });
    format!("{} · {who}", agent.label())
}

/// Where the desktop app's builds live — the "Download desktop app" target.
const DESKTOP_RELEASES_URL: &str = "https://github.com/Niach/exponential/releases/latest";

/// The instance the install one-liner points the daemon at.
fn server_install_origin(cx: &gpui::App) -> String {
    queries::active_account(cx)
        .map(|account| account.instance_url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://app.exponential.at".to_string())
}

/// The headless-daemon install one-liner (web `buildServerInstallSnippet`) —
/// the Add-device dialog shows it, the Getting-started server card copies it.
pub(crate) fn server_install_snippet(cx: &gpui::App) -> String {
    let origin = server_install_origin(cx);
    format!("curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE={origin} sh")
}

/// The install one-liner in its copyable box (EXP-725 — extracted so the
/// Add-device dialog and the onboarding wizard's devices step render the SAME
/// box). The clipboard gets the ONE-LINE command; the box shows it wrapped
/// over two lines so the snippet never needs a horizontal scroll.
pub(crate) fn server_install_snippet_box(
    id: impl Into<gpui::ElementId>,
    cx: &gpui::App,
) -> impl IntoElement {
    let origin = server_install_origin(cx);
    let snippet = server_install_snippet(cx);
    let line_two = SharedString::from(format!("  EXP_INSTANCE={origin} sh"));
    div()
        .relative()
        .p_2()
        .pr_8()
        .rounded(px(theme::tokens::radius::SM))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .text_xs()
        .font_family(theme::terminal::FONT_FAMILY)
        .text_color(cx.theme().foreground)
        .child(
            gpui_component::v_flex()
                .child("curl -fsSL https://exponential.at/install.sh |")
                .child(line_two),
        )
        .child(
            div().absolute().top_1().right_1().child(
                // EXP-862: a copy glyph on a row is ghost chrome, not a circle.
                crate::controls::ghost_icon_button(id, Icon::new(registry::UI_COPY), cx)
                    .tooltip("Copy install command")
                    .on_click(move |_, window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(snippet.clone()));
                        window
                            .push_notification(Notification::success("Copied install command"), cx);
                    }),
            ),
        )
}

/// The "Add device" dialog (EXP-697, one spec with the web twin): the desktop
/// app first — it is what actually runs coding sessions — then the install
/// one-liner for the headless `exponential` CLI as the always-on-server path.
/// The script is served by the CLOUD marketing site for every instance
/// (self-hosted ships no marketing pages), so the snippet always names the
/// target instance explicitly via `EXP_INSTANCE` — the web
/// `buildServerInstallSnippet` shape exactly. Shared (EXP-470): opened from
/// this section's band and from the Getting-started page's server card.
pub(crate) fn open_add_server_dialog(window: &mut Window, cx: &mut gpui::App) {
    let spec = AlertSpec::new(
        "Add device",
        "To run coding sessions, install the desktop app.",
        "Done",
    )
    .without_cancel()
    .height(px(320.))
    .content(move |_, cx| {
        gpui_component::v_flex()
            .gap_3()
            .child(
                gpui_component::h_flex().child(
                    Button::new("add-device-download")
                        .outline()
                        .web_sm()
                        .icon(Icon::new(registry::UI_DOWNLOAD))
                        .label("Download desktop app")
                        .on_click(|_, _, cx| {
                            crate::settings::open_url(cx, DESKTOP_RELEASES_URL.to_string());
                        }),
                ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Or install the Exponential CLI on a server:"),
            )
            .child(server_install_snippet_box("add-device-copy", cx))
            .into_any_element()
    });
    native_dialog::open_alert(window, cx, spec);
}

/// EXP-409: online with NOTHING runnable — every installed agent is signed
/// out, so a start on this device would fail at the first request. EXP-862
/// dropped it from the row's status LINE (the chip badge says it once); the ▶
/// gate still asks.
fn sign_in_needed(device: &DeviceCard) -> bool {
    device.online && device.agents.is_empty() && !device.unauthed_agents.is_empty()
}

/// EXP-696: why ANOTHER device's ▶ is dead. A start is a `steer.startSession`
/// the device has to pick up off its heartbeat and run with a CLI it
/// advertises — offline or agentless, it can do neither, and the preselect
/// would be dropped by the composer. Pure (unit-tested); this device's own row
/// gates on the local doctor (`no_agent_reason`) instead.
fn remote_start_reason(device: &DeviceCard) -> Option<String> {
    if !device.online {
        return Some("Offline — this machine can't take a run".to_string());
    }
    if sign_in_needed(device) {
        return Some(format!("{} not signed in", device.unauthed_agents.join(", ")));
    }
    if device.agents.is_empty() {
        return Some("No agent CLI available on this machine".to_string());
    }
    None
}

/// `Online` / `Last seen 5m` / `Offline` — the web row's caption, in the
/// desktop's relative-time wording.
///
/// EXP-862: nothing about sign-ins any more. "claude not signed in" said what
/// the chip's own badge below already says, and it was the loudest thing on a
/// row that was otherwise fine.
fn status_line(device: &DeviceCard) -> String {
    if device.online {
        return "Online".to_string();
    }
    match device.last_seen_at.as_deref().map(crate::inbox::relative_time) {
        Some(seen) if !seen.is_empty() => format!("Last seen {seen}"),
        _ => "Offline".to_string(),
    }
}

/// The web row's amber nudge: this device's version compares below its
/// platform's `CLIENT_LATEST_VERSION_*`. Unknown on either side = no nudge.
fn update_available(version: Option<&str>, latest: Option<&str>) -> bool {
    match (version, latest) {
        (Some(version), Some(latest)) => crate::update::is_newer(latest, version),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(online: bool, last_seen: Option<&str>) -> DeviceCard {
        DeviceCard {
            row_id: "row-1".to_string(),
            device_id: "dev-1".to_string(),
            label: "Studio".into(),
            server: false,
            online,
            own: false,
            mine: true,
            owner_tooltip: None,
            shared: false,
            is_default: false,
            version: None,
            update_requested: false,
            update_blocked: false,
            active_sessions: 0,
            last_seen_at: last_seen.map(str::to_string),
            agents: Vec::new(),
            unauthed_agents: Vec::new(),
            caps: Vec::new(),
            chips: Vec::new(),
            health: coding::agent_accounts::Health::Unknown,
        }
    }

    #[test]
    fn status_line_reads_online_last_seen_or_offline() {
        assert_eq!(status_line(&device(true, None)), "Online");
        assert!(status_line(&device(false, Some("2020-01-01T00:00:00Z")))
            .starts_with("Last seen "));
        assert_eq!(status_line(&device(false, None)), "Offline");
        // A timestamp the parser rejects must never render a dangling
        // "Last seen " with no time after it.
        assert_eq!(status_line(&device(false, Some("garbage"))), "Offline");
    }

    /// EXP-862: the status line says nothing about sign-ins — a device whose
    /// only agent is signed out still reads "Online" (its chip carries the
    /// "Signed out" badge, once).
    #[test]
    fn status_line_never_names_signed_out_agents() {
        let mut nothing_runnable = device(true, None);
        nothing_runnable.unauthed_agents = vec!["claude".to_string()];
        assert!(sign_in_needed(&nothing_runnable));
        assert_eq!(status_line(&nothing_runnable), "Online");

        let mut partly = device(true, None);
        partly.agents = vec!["codex".to_string()];
        partly.unauthed_agents = vec!["claude".to_string()];
        assert!(!sign_in_needed(&partly));
        assert_eq!(status_line(&partly), "Online");

        let mut offline = device(false, None);
        offline.unauthed_agents = vec!["claude".to_string()];
        assert!(!sign_in_needed(&offline));
        assert_eq!(status_line(&offline), "Offline");
    }

    /// EXP-696: a FOREIGN row's ▶ is only live when that device could
    /// actually take the run — offline or with nothing runnable it is
    /// disabled with the reason, instead of dropping the preselect and
    /// starting here.
    #[test]
    fn remote_start_needs_an_online_device_with_an_agent() {
        let mut ready = device(true, None);
        ready.agents = vec!["claude".to_string()];
        assert_eq!(remote_start_reason(&ready), None);

        let mut offline = ready.clone();
        offline.online = false;
        assert!(remote_start_reason(&offline).is_some_and(|reason| reason.starts_with("Offline")));

        let mut signed_out = device(true, None);
        signed_out.unauthed_agents = vec!["claude".to_string()];
        assert_eq!(
            remote_start_reason(&signed_out).as_deref(),
            Some("claude not signed in")
        );

        // Online, nothing installed at all.
        assert_eq!(
            remote_start_reason(&device(true, None)).as_deref(),
            Some("No agent CLI available on this machine")
        );
    }

    #[test]
    fn update_nudge_needs_both_versions() {
        assert!(update_available(Some("0.4.1"), Some("0.5.0")));
        assert!(!update_available(Some("0.5.0"), Some("0.5.0")));
        // A device ahead of the informational value (a dev build) is fine.
        assert!(!update_available(Some("0.6.0"), Some("0.5.0")));
        assert!(!update_available(None, Some("0.5.0")));
        assert!(!update_available(Some("0.4.1"), None));
    }

    /// EXP-862: a device row's chip menu is the shared rule — a healthy,
    /// non-default login on a device that takes both commands offers the two
    /// writes; an offline device's chips are statements.
    #[test]
    fn chip_label_names_the_agent_and_the_login() {
        let chip = DeviceAccountChip {
            key: "claude:0a1b".to_string(),
            agent: "claude".to_string(),
            profile_id: "0a1b".to_string(),
            profile_label: "Work".to_string(),
            email: Some("dev@acme.test".to_string()),
            plan: Some("max".to_string()),
            signed_in: true,
            active: false,
            health: coding::agent_accounts::Health::Ok,
        };
        assert_eq!(
            chip_label(&chip, coding::CodingAgent::Claude),
            "Claude Code · dev@acme.test"
        );
        // No address: the plan, then the profile's own label.
        let mut plan_only = chip.clone();
        plan_only.email = None;
        assert_eq!(
            chip_label(&plan_only, coding::CodingAgent::Claude),
            "Claude Code · max"
        );
        let mut signed_out = plan_only.clone();
        signed_out.signed_in = false;
        assert_eq!(
            chip_label(&signed_out, coding::CodingAgent::Claude),
            "Claude Code · Work"
        );
    }

    /// EXP-832: a heartbeat that changes nothing must not repaint the list —
    /// the derivation compares equal.
    #[test]
    fn derived_rows_compare_equal_when_nothing_moved() {
        let one = DerivedDevices {
            mine: vec![device(true, None)],
            team: Vec::new(),
        };
        let two = DerivedDevices {
            mine: vec![device(true, None)],
            team: Vec::new(),
        };
        assert_eq!(one, two);
        let moved = DerivedDevices {
            mine: vec![device(false, None)],
            team: Vec::new(),
        };
        assert_ne!(one, moved);
    }
}

impl Render for MachinesSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_latest_loaded(cx);
        // First frame: the observer only fires on a DELTA, so the derivation
        // has to happen here once.
        if self.derived.is_none() {
            self.derived = Self::derive(cx);
        }

        let muted = cx.theme().muted_foreground;
        let derived = self.derived.clone();
        let (mine, team) = match derived.as_ref() {
            Some(derived) => (derived.mine.as_slice(), derived.team.as_slice()),
            None => (&[][..], &[][..]),
        };
        let mine_rows: Vec<gpui::AnyElement> = mine
            .iter()
            .enumerate()
            .map(|(index, device)| self.render_row(index, device, cx))
            .collect();
        let team_rows: Vec<gpui::AnyElement> = team
            .iter()
            .enumerate()
            // Offset the element ids so the two groups can never collide.
            .map(|(index, device)| self.render_row(index + mine.len(), device, cx))
            .collect();

        // EXP-642: the web `GlassSectionHeader` — a plain-text heading with
        // no count, the "Add device" control trailing (EXP-697: the dialog
        // leads with the desktop app, so the button no longer says server).
        let add_server = crate::surface::glass_pill_button(
            "machines-add-server",
            crate::surface::PillSize::Sm,
            cx,
        )
        .icon(registry::UI_ADD)
        .label("Add device")
        .on_click(|_: &gpui::ClickEvent, window, cx| {
            open_add_server_dialog(window, cx);
        })
        .into_any_element();

        // NO `w_full` (EXP-508): as a child of the page's centered column, a
        // percent width resolves against the UNCLAMPED ancestor available
        // width and shrink-wraps the section at wide windows (the EXP-436
        // leak). Auto width + the column's flex-col stretch size it to the
        // capped column width; the heading/rows below a stretch-sized parent
        // resolve their `w_full` correctly.
        gpui_component::v_flex()
            .min_w_0()
            .child(crate::surface::glass_section_header(
                "My devices",
                Some(add_server),
                cx,
            ))
            .when(derived.is_none(), |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading devices…"),
                )
            })
            .when(derived.is_some() && mine.is_empty(), |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("No devices yet. Open the Exponential desktop app, or add a device."),
                )
            })
            .child(gpui_component::v_flex().min_w_0().children(mine_rows))
            // EXP-432/642: teammates' shared devices get their OWN headed
            // section, exactly like the web page.
            .when(!team.is_empty(), |this| {
                this.child(
                    gpui_component::v_flex()
                        .min_w_0()
                        .pt_4()
                        .child(crate::surface::glass_section_header(
                            "Team devices",
                            None,
                            cx,
                        ))
                        .child(gpui_component::v_flex().min_w_0().children(team_rows)),
                )
            })
    }
}
