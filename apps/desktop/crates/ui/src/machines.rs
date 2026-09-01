//! "My machines" (EXP-403): the signed-in user's registered devices —
//! desktops and headless `exponential` daemon servers — rendered as a
//! section under the Actions tool window's list, so all four clients surface
//! the same device registry.
//!
//! The rows come from the SYNCED `devices` shape and nothing else (EXP-485):
//! online-ness is `last_seen_at` freshness against the contract window — no
//! relay presence involved — and every mutation lands back through sync, so
//! there is no poll and no fallback list to keep in step.
//!
//! The one tRPC call left is `devices.latestVersions`, the informational
//! `CLIENT_LATEST_VERSION_*` pair behind the amber update nudge. That is
//! instance config, not device state: it is fetched ONCE per section
//! lifetime (the first render) rather than polled.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, ClipboardItem, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

pub(crate) struct MachinesSection {
    latest: api::devices::LatestVersions,
    /// The one-shot guard for [`Self::ensure_latest_loaded`].
    latest_requested: bool,
    /// Device with an in-flight `requestUpdate` — holds "Updating…" until the
    /// synced row carries the server's own `update_requested_at`.
    updating: Option<String>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl MachinesSection {
    pub(crate) fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The rows stream over the devices shape — re-render on every delta.
        let devices_collection = sync::Store::global(cx).collections().devices.clone();
        let subscriptions = vec![cx.observe(&devices_collection, |_, _, cx| cx.notify())];
        Self {
            latest: api::devices::LatestVersions::default(),
            latest_requested: false,
            updating: None,
            _subscriptions: subscriptions,
        }
    }

    // -- data ----------------------------------------------------------------

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
            move |trpc| api::devices::request_update(trpc, &device_id),
            cx,
        );
    }

    // -- dialogs -------------------------------------------------------------

    /// EXP-481: Edit… — the Device settings dialog, keyed by the SYNCED
    /// devices row (rename and sharing live inside it). Every rendered row
    /// IS a synced row since EXP-485, so a miss means the row was removed
    /// out from under the open menu: do nothing.
    fn open_settings(&mut self, device_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let row_id = sync::Store::global(cx)
            .collections()
            .devices
            .read(cx)
            .iter()
            .find(|row| row.device_id.as_deref() == Some(device_id.as_str()))
            .map(|row| row.id.clone());
        let Some(row_id) = row_id else {
            return;
        };
        crate::device_settings::open(window, cx, row_id);
    }

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

    /// EXP-481: the rows from the SYNCED devices shape, mapped into the
    /// legacy `DeviceEntry` shape `render_row` consumes — `(mine, team)`,
    /// each group in the EXP-623 stable order (online-by-label first, so
    /// heartbeats can't reorder the list; offline rows don't beat, so
    /// last-seen desc is stable there). EXP-642 keeps the two groups apart:
    /// they render as their own headed sections, web parity. `None` while the
    /// shape is Waiting (cold start / old server) — the section renders its
    /// loading note.
    #[allow(clippy::type_complexity)] // one call site, two plain row groups
    fn synced_entries(
        &self,
        cx: &App,
    ) -> Option<(Vec<api::devices::DeviceEntry>, Vec<api::devices::DeviceEntry>)> {
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
        let mut mine = Vec::new();
        let mut shared = Vec::new();
        for row in devices.iter() {
            let owned = row.user_id.as_deref() == Some(me.as_str());
            let owner = (!owned).then(|| api::devices::DeviceOwner {
                id: row.user_id.clone().unwrap_or_default(),
                name: row
                    .user_id
                    .as_deref()
                    .and_then(|user_id| users.get(user_id))
                    .and_then(|user| user.name.clone())
                    .unwrap_or_default(),
            });
            let update_requested = row.update_requested_at.is_some();
            let entry = api::devices::DeviceEntry {
                device_id: row.device_id.clone().unwrap_or_default(),
                device_label: row.label.clone().unwrap_or_default(),
                kind: row.kind.clone().unwrap_or_default(),
                platform: row.platform.clone(),
                agents: row.agent_ids(),
                unauthed_agents: row.unauthed_agent_ids(),
                caps: row.cap_ids(),
                online: crate::device_settings::row_is_online(
                    row.last_seen_at.as_deref(),
                    now_ms,
                ),
                last_seen_at: row.last_seen_at.clone(),
                registered: true,
                version: row.version.clone(),
                update_requested,
                update_blocked: update_requested
                    && row.active_sessions.unwrap_or(0) > 0,
                launch_defaults: row.launch_defaults.clone(),
                shared_team_id: row.shared_team_id.clone(),
                // EXP-622: a teammate's flag is THEIR preference, never ours.
                is_default: owned && row.is_default.unwrap_or(false),
                owner,
            };
            if owned {
                mine.push(entry);
            } else {
                shared.push(entry);
            }
        }
        // Online first, then by label so heartbeats can't reorder the online
        // group; offline rows don't beat, so last-seen desc (ISO stamps order
        // lexicographically) is stable there.
        let stable_order = |a: &api::devices::DeviceEntry, b: &api::devices::DeviceEntry| {
            b.online
                .cmp(&a.online)
                .then_with(|| {
                    if a.online {
                        std::cmp::Ordering::Equal
                    } else {
                        b.last_seen_at.cmp(&a.last_seen_at)
                    }
                })
                .then_with(|| {
                    a.device_label
                        .to_lowercase()
                        .cmp(&b.device_label.to_lowercase())
                })
                .then_with(|| a.device_id.cmp(&b.device_id))
        };
        mine.sort_by(stable_order);
        shared.sort_by(stable_order);
        Some((mine, shared))
    }

    // -- render --------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        device: &api::devices::DeviceEntry,
        own_device_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let base_label = if device.device_label.trim().is_empty() {
            device.device_id.clone()
        } else {
            device.device_label.clone()
        };
        // EXP-525: no owner-name suffix in the visible label — the row shows
        // just the machine name (web parity); a teammate's shared row keeps
        // the attribution in its tooltip below.
        let label: SharedString = base_label.into();
        let owner_tooltip: Option<SharedString> = device
            .owner
            .as_ref()
            .filter(|owner| !owner.name.is_empty())
            .map(|owner| format!("Shared by {}", owner.name).into());
        let server = device.is_server();
        // EXP-642: only an OWN row wears the chip — the Team machines section
        // is shared by definition.
        let shared = device.owner.is_none() && device.shared_team_id.is_some();
        let kind_icon = if server {
            registry::UI_SERVER
        } else {
            registry::UI_DEVICE
        };
        // Informational only: the row nudges, the machine decides.
        let latest = if server {
            self.latest.cli.as_deref()
        } else {
            self.latest.desktop.as_deref()
        };
        let outdated = update_available(device.version.as_deref(), latest);
        let updating = device.update_requested || self.updating.as_deref() == Some(&device.device_id);
        // EXP-411: the request is parked behind live sessions on the machine —
        // "Update queued" instead of an indefinite "Updating…".
        let queued = device.update_requested && device.update_blocked;
        // A relay-only row (a build predating the registry) has nothing to
        // rename, remove or update — it carries no registry row.
        let menu = (device.registered && device.owner.is_none()).then(|| {
            let section = cx.entity().downgrade();
            let device_id = device.device_id.clone();
            let menu_label = label.clone();
            // EXP-420: offer the update only when a newer CLI version really
            // exists (or one is already in flight — keep its state visible).
            let can_update = server && device.online && (outdated || updating);
            Button::new(("machine-menu", index))
                .ghost().cursor_pointer()
                .xsmall()
                .icon(registry::UI_MORE)
                .dropdown_menu(move |menu, _window, cx| {
                    let edit_section = section.clone();
                    let edit_id = device_id.clone();
                    let remove_section = section.clone();
                    let remove_id = device_id.clone();
                    let remove_label = menu_label.clone();
                    let update_section = section.clone();
                    let update_id = device_id.clone();
                    // EXP-481: Rename + Sharing live INSIDE the Device
                    // settings dialog now (with the defaults editor and the
                    // worktree list) — the menu is Edit/Update/Remove.
                    menu.item(
                        PopupMenuItem::new("Edit…")
                            .icon(Icon::new(registry::UI_EDIT))
                            .on_click(move |_, window, cx| {
                                let Some(section) = edit_section.upgrade() else {
                                    return;
                                };
                                let id = edit_id.clone();
                                section.update(cx, |this, cx| {
                                    this.open_settings(id, window, cx);
                                });
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
        // the dialog opens with THIS row's machine preselected in its Device
        // picker, so the local doctor gates the row only for this install —
        // another machine is gated on its OWN advertisement (the dialog's
        // blocker re-reads it), which is what keeps the button honest: a
        // preselect for a machine that cannot take the run must not look
        // startable.
        let own = device.device_id == own_device_id;
        let no_agent = match own {
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
            .on_click(move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut gpui::App| {
                let nav = crate::navigation::nav_for_window(window, cx);
                let Some(team_id) = crate::navigation::active_team_id(&nav, cx) else {
                    return;
                };
                crate::start_coding_dialog::open_for_selection(
                    window,
                    cx,
                    team_id,
                    Vec::new(),
                    None,
                    Some(start_device_id.clone()),
                );
            });

        // EXP-642: one CARD per machine, the web `GlassRow` two-line shape —
        // icon · (name · version · default star · "Shared") over the status
        // line · ▶ · ⋯ — `min_w_0` down the name side so only the NAME gives
        // way.
        let row_hover = theme.list_active.opacity(0.5);
        crate::surface::glass_row_card()
            .id(SharedString::from(format!("machine-{}", device.device_id)))
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
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
                                .when_some(owner_tooltip, |this, owner| {
                                    this.tooltip(move |window, cx| {
                                        gpui_component::tooltip::Tooltip::new(owner.clone())
                                            .build(window, cx)
                                    })
                                })
                                .child(label.clone()),
                        )
                        // EXP-696: the machine this app IS — the same muted
                        // caption the "Shared" chip uses, so the row line
                        // keeps one rhythm.
                        .when(own, |this| {
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
                        // EXP-622: the machine every device picker prefills.
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
                                format!("Update available: v{}", latest.unwrap_or_default()).into();
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
                        // EXP-642 (web parity): an own machine shared with a team
                        // says so on the name line.
                        .when(shared, |this| {
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
                                // EXP-409: online but nothing runnable (every
                                // installed agent signed out) shows amber, not green.
                                let dot = if sign_in_needed(device) {
                                    theme::tokens::YELLOW.to_hsla()
                                } else {
                                    theme::tokens::GREEN.to_hsla()
                                };
                                this.child(
                                    div()
                                        .size_1p5()
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(dot),
                                )
                            })
                            .child(SharedString::from(status_line(device)))
                            .when(updating, |this| {
                                this.child(div().child(if queued {
                                    "Update queued"
                                } else {
                                    "Updating…"
                                }))
                            }),
                    ),
            )
            .child(div().flex_shrink_0().child(start_coding))
            .children(menu.map(|menu| div().flex_shrink_0().child(menu)))
            .into_any_element()
    }
}

/// Where the desktop app's builds live — the "Download desktop app" target.
const DESKTOP_RELEASES_URL: &str = "https://github.com/Niach/exponential/releases/latest";

/// The "Add device" dialog (EXP-697, one spec with the web twin): the desktop
/// app first — it is what actually runs coding sessions — then the install
/// one-liner for the headless `exponential` CLI as the always-on-server path.
/// The script is served by the CLOUD marketing site for every instance
/// (self-hosted ships no marketing pages), so the snippet always names the
/// target instance explicitly via `EXP_INSTANCE` — the web
/// `buildServerInstallSnippet` shape exactly. Shared (EXP-470): opened from
/// this section's band and from the Getting-started page's server card.
pub(crate) fn open_add_server_dialog(window: &mut Window, cx: &mut gpui::App) {
    let origin = queries::active_account(cx)
        .map(|account| account.instance_url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://app.exponential.at".to_string());
    // The clipboard gets the ONE-LINE command; the box shows it wrapped over
    // two lines so the snippet never needs a horizontal scroll.
    let snippet =
        format!("curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE={origin} sh");
    let line_two = SharedString::from(format!("  EXP_INSTANCE={origin} sh"));
    let spec = AlertSpec::new(
        "Add device",
        "To run coding sessions, install the desktop app.",
        "Done",
    )
    .without_cancel()
    .height(px(320.))
    .content(move |_, cx| {
        let snippet = snippet.clone();
        let foreground = cx.theme().foreground;
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
            .child(
                div()
                    .relative()
                    .p_2()
                    .pr_8()
                    .rounded(px(theme::tokens::radius::SM))
                    .border_1()
                    .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                    .text_xs()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_color(foreground)
                    .child(
                        gpui_component::v_flex()
                            .child("curl -fsSL https://exponential.at/install.sh |")
                            .child(line_two.clone()),
                    )
                    .child(
                        div().absolute().top_1().right_1().child(
                            Button::new("add-device-copy")
                                .ghost()
                                .cursor_pointer()
                                .xsmall()
                                .icon(Icon::new(registry::UI_COPY))
                                .tooltip("Copy install command")
                                .on_click(move |_, window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        snippet.clone(),
                                    ));
                                    window.push_notification(
                                        Notification::success("Copied install command"),
                                        cx,
                                    );
                                }),
                        ),
                    ),
            )
            .into_any_element()
    });
    native_dialog::open_alert(window, cx, spec);
}

/// EXP-409: online with NOTHING runnable — every installed agent is signed
/// out, so the row reads amber with the sign-in reason instead of "Online".
fn sign_in_needed(device: &api::devices::DeviceEntry) -> bool {
    device.online && device.agents.is_empty() && !device.unauthed_agents.is_empty()
}

/// EXP-696: why ANOTHER machine's ▶ is dead. A start is a `steer.startSession`
/// the machine has to pick up off its heartbeat and run with a CLI it
/// advertises — offline or agentless, it can do neither, and the preselect
/// would be dropped by the dialog. Pure (unit-tested); this machine's own row
/// gates on the local doctor (`no_agent_reason`) instead.
fn remote_start_reason(device: &api::devices::DeviceEntry) -> Option<String> {
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
/// desktop's relative-time wording. Signed-out agents (EXP-409) annotate the
/// online state ("claude not signed in" replaces it when nothing is
/// runnable).
fn status_line(device: &api::devices::DeviceEntry) -> String {
    if device.online {
        let unauthed = device.unauthed_agents.join(", ");
        if sign_in_needed(device) {
            return format!("{unauthed} not signed in");
        }
        if !unauthed.is_empty() {
            return format!("Online · {unauthed} not signed in");
        }
        return "Online".to_string();
    }
    match device.last_seen_at.as_deref().map(crate::inbox::relative_time) {
        Some(seen) if !seen.is_empty() => format!("Last seen {seen}"),
        _ => "Offline".to_string(),
    }
}

/// The web row's amber nudge: this machine's version compares below its
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

    fn device(online: bool, last_seen: Option<&str>) -> api::devices::DeviceEntry {
        api::devices::DeviceEntry {
            online,
            last_seen_at: last_seen.map(str::to_string),
            ..Default::default()
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

    /// EXP-409: signed-out agents replace "Online" when nothing is runnable
    /// and annotate it when a runnable sibling exists; offline rows are
    /// untouched.
    #[test]
    fn status_line_names_signed_out_agents() {
        let mut nothing_runnable = device(true, None);
        nothing_runnable.unauthed_agents = vec!["claude".to_string()];
        assert!(sign_in_needed(&nothing_runnable));
        assert_eq!(status_line(&nothing_runnable), "claude not signed in");

        let mut partly = device(true, None);
        partly.agents = vec!["codex".to_string()];
        partly.unauthed_agents = vec!["claude".to_string()];
        assert!(!sign_in_needed(&partly));
        assert_eq!(status_line(&partly), "Online · claude not signed in");

        let mut offline = device(false, None);
        offline.unauthed_agents = vec!["claude".to_string()];
        assert!(!sign_in_needed(&offline));
        assert_eq!(status_line(&offline), "Offline");
    }

    /// EXP-696: a FOREIGN row's ▶ is only live when that machine could
    /// actually take the run — offline or with nothing runnable it is
    /// disabled with the reason, instead of dropping the preselect and
    /// starting here.
    #[test]
    fn remote_start_needs_an_online_machine_with_an_agent() {
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
        // A machine ahead of the informational value (a dev build) is fine.
        assert!(!update_available(Some("0.6.0"), Some("0.5.0")));
        assert!(!update_available(None, Some("0.5.0")));
        assert!(!update_available(Some("0.4.1"), None));
    }
}

impl Render for MachinesSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_latest_loaded(cx);

        let muted = cx.theme().muted_foreground;
        // EXP-485: the synced shape is the only source of rows.
        let groups = self.synced_entries(cx);
        let (mine, team) = match groups.as_ref() {
            Some((mine, team)) => (mine.as_slice(), team.as_slice()),
            None => (&[][..], &[][..]),
        };
        // EXP-696: the machine this IDE runs on wears the "This device"
        // marker (and its ▶ is the only one the LOCAL doctor gates). Cached
        // per process — resolving it reads settings.json.
        let own_device_id = queries::own_device_id(cx);
        let mine_rows: Vec<gpui::AnyElement> = mine
            .iter()
            .enumerate()
            .map(|(index, device)| self.render_row(index, device, &own_device_id, cx))
            .collect();
        let team_rows: Vec<gpui::AnyElement> = team
            .iter()
            .enumerate()
            // Offset the element ids so the two groups can never collide.
            .map(|(index, device)| {
                self.render_row(index + mine.len(), device, &own_device_id, cx)
            })
            .collect();

        // EXP-642: the web `GlassSectionHeader` — a plain-text heading with
        // no count, the "Add device" control trailing (EXP-697: the dialog
        // leads with the desktop app, so the button no longer says server).
        let add_server = Button::new("machines-add-server")
            .outline().cursor_pointer()
            .web_xs()
            .icon(registry::UI_ADD)
            .label("Add device")
            .on_click(|_: &gpui::ClickEvent, window, cx| {
                open_add_server_dialog(window, cx);
            })
            .into_any_element();

        // NO `w_full` (EXP-508): as a child of the Actions page's centered
        // column, a percent width resolves against the UNCLAMPED ancestor
        // available width and shrink-wraps the section at wide windows (the
        // EXP-436 leak). Auto width + the column's flex-col stretch size it
        // to the capped column width; the heading/rows below a stretch-sized
        // parent resolve their `w_full` correctly.
        gpui_component::v_flex()
            .min_w_0()
            .child(crate::actions_view::section_heading(
                "My machines",
                Some(add_server),
                cx,
            ))
            .when(groups.is_none(), |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading machines…"),
                )
            })
            .when(groups.is_some() && mine.is_empty(), |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            "No machines yet. Sign in on a desktop app, or install the \
                             exponential CLI on a server.",
                        ),
                )
            })
            .child(gpui_component::v_flex().min_w_0().gap_2().children(mine_rows))
            // EXP-432/642: teammates' shared machines get their OWN headed
            // section, exactly like the web page.
            .when(!team.is_empty(), |this| {
                this.child(
                    gpui_component::v_flex()
                        .min_w_0()
                        .pt_4()
                        .child(crate::actions_view::section_heading(
                            "Team machines",
                            None,
                            cx,
                        ))
                        .child(
                            gpui_component::v_flex().min_w_0().gap_2().children(team_rows),
                        ),
                )
            })
    }
}
