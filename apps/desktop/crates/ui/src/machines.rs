//! "My machines" (EXP-403): the signed-in user's registered devices —
//! desktops and headless `exponential` daemon servers — rendered as a
//! section under the Actions tool window's list, so all four clients surface
//! the same device registry.
//!
//! The data is tRPC (`devices.list`), NOT an Electric shape: machine state is
//! per-USER, not team product data, and it merges live relay presence that no
//! table holds. There is therefore no echo to observe — the section polls
//! every [`POLL_INTERVAL`] while it is on screen and refetches right after
//! every mutation.
//!
//! Visibility is inferred from rendering: this view is only in the element
//! tree while the Actions tool window is open, so a poll tick that finds the
//! last render older than [`VISIBLE_GRACE`] retires the loop (the sidebar's
//! Support poll self-terminates on its fetch key the same way). Every fetch
//! notifies — including failures — so a visible section always re-renders
//! inside the grace window.

use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AppContext as _, ClipboardItem, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _, Icon, Sizable as _,
};

use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// Matches the sidebar's Support poll cadence class: often enough that an
/// online/offline flip lands while the user is looking, cheap enough for a
/// per-user query.
const POLL_INTERVAL: Duration = Duration::from_secs(15);
/// A poll tick with no render this recent means the tool window was closed.
/// Two intervals plus slack — a single slow fetch must not retire the loop.
const VISIBLE_GRACE: Duration = Duration::from_secs(40);

pub(crate) struct MachinesSection {
    /// `None` until the first answer lands (renders the loading note).
    devices: Option<Vec<api::devices::DeviceEntry>>,
    latest: api::devices::LatestVersions,
    fetch_seq: u64,
    polling: bool,
    last_render: Instant,
    /// Device with an in-flight `requestUpdate` — holds "Updating…" until the
    /// refetch carries the server's own `updateRequested` flag.
    updating: Option<String>,
    rename_input: Entity<InputState>,
}

impl MachinesSection {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("Machine name"));
        Self {
            devices: None,
            latest: api::devices::LatestVersions::default(),
            fetch_seq: 0,
            polling: false,
            last_render: Instant::now(),
            updating: None,
            rename_input,
        }
    }

    // -- data ----------------------------------------------------------------

    /// Start the fetch + poll pair once; a later render only refreshes
    /// `last_render`, which is what keeps the loop alive.
    fn ensure_polling(&mut self, cx: &mut gpui::Context<Self>) {
        if self.polling {
            return;
        }
        self.polling = true;
        self.fetch(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.last_render.elapsed() > VISIBLE_GRACE {
                        // Off screen — the next render restarts everything.
                        this.polling = false;
                        return false;
                    }
                    this.fetch(cx);
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// One seq-guarded `devices.list`. A failure keeps the rendered rows (the
    /// next tick retries) but still notifies, so the poll's liveness check
    /// sees a fresh render.
    fn fetch(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.fetch_seq += 1;
        let seq = self.fetch_seq;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::list(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.fetch_seq != seq {
                    return;
                }
                match result {
                    Ok(list) => {
                        this.devices = Some(list.devices);
                        this.latest = list.latest_versions;
                    }
                    Err(err) => log::warn!("[ui] devices.list failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Run a `devices.*` mutation on the background executor and refetch — no
    /// mutation has an echo, so the list is only as fresh as the next read.
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
                this.fetch(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn rename(&mut self, device_id: String, label: String, cx: &mut gpui::Context<Self>) {
        self.mutate(
            "devices.rename",
            move |trpc| api::devices::rename(trpc, &device_id, &label),
            cx,
        );
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

    /// The web "Add a server" dialog (EXP-480): the install one-liner for the
    /// headless `exponential` CLI, with OK = copy to clipboard. The script is
    /// served by the CLOUD marketing site for every instance (self-hosted
    /// ships no marketing pages), so the snippet always names the target
    /// instance explicitly via `EXP_INSTANCE` — the web
    /// `buildServerInstallSnippet` shape exactly.
    fn prompt_add_server(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let origin = queries::active_account(cx)
            .map(|account| account.instance_url.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://app.exponential.at".to_string());
        let snippet =
            format!("curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE={origin} sh");
        let content_snippet = SharedString::from(snippet.clone());
        let spec = AlertSpec::new(
            "Add a server",
            "Run this on any Linux or macOS machine. It installs the \
             exponential CLI, signs you in with a device code, and registers \
             the machine here.",
            "Copy",
        )
        // Description + the wrapped two-line snippet box.
        .height(px(264.))
        .content(move |_, cx| {
            div()
                .mt_2()
                .p_2()
                .rounded(px(theme::tokens::radius::SM))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .text_color(cx.theme().foreground)
                .child(content_snippet.clone())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(snippet.clone()));
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// Rename behind the shared alert window, the typed-input pattern from
    /// the team Danger Zone (the input is a long-lived child so its state
    /// survives the dialog window).
    fn prompt_rename(
        &mut self,
        device_id: String,
        label: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.rename_input.update(cx, |state, cx| {
            state.set_value(label.clone(), window, cx);
        });
        let section = cx.entity().downgrade();
        let content_input = self.rename_input.clone();
        let ok_input = self.rename_input.clone();
        let spec = AlertSpec::new(
            "Rename machine",
            "The name is yours only — it never overwrites what the machine \
             advertises to the relay.",
            "Rename",
        )
        // Two description lines + the input — the 220 default clips them.
        .height(gpui::px(240.))
        .content(move |_, _| {
            div()
                .mt_2()
                .child(Input::new(&content_input).small())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let typed = ok_input.read(cx).value().trim().to_string();
            if typed.is_empty() {
                // Empty keeps the dialog open (there is no unnamed machine).
                return false;
            }
            if let Some(section) = section.upgrade() {
                section.update(cx, |this, cx| this.rename(device_id.clone(), typed, cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
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

    // -- render --------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        device: &api::devices::DeviceEntry,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let label: SharedString = if device.device_label.trim().is_empty() {
            device.device_id.clone().into()
        } else {
            device.device_label.clone().into()
        };
        let server = device.is_server();
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
        let menu = device.registered.then(|| {
            let section = cx.entity().downgrade();
            let device_id = device.device_id.clone();
            let menu_label = label.clone();
            // EXP-420: offer the update only when a newer CLI version really
            // exists (or one is already in flight — keep its state visible).
            let can_update = server && device.online && (outdated || updating);
            Button::new(("machine-menu", index))
                .ghost()
                .xsmall()
                .icon(registry::UI_MORE)
                .dropdown_menu(move |menu, _window, _cx| {
                    let rename_section = section.clone();
                    let rename_id = device_id.clone();
                    let rename_label = menu_label.clone();
                    let remove_section = section.clone();
                    let remove_id = device_id.clone();
                    let remove_label = menu_label.clone();
                    let update_section = section.clone();
                    let update_id = device_id.clone();
                    menu.item(
                        PopupMenuItem::new("Rename…")
                            .icon(Icon::new(registry::UI_EDIT))
                            .on_click(move |_, window, cx| {
                                let Some(section) = rename_section.upgrade() else {
                                    return;
                                };
                                let id = rename_id.clone();
                                let label = rename_label.to_string();
                                section.update(cx, |this, cx| {
                                    this.prompt_rename(id, label, window, cx);
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
                    .separator()
                    .item(
                        PopupMenuItem::new("Remove…")
                            .icon(Icon::new(registry::UI_DELETE))
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

        // ONE line per machine, the web row's shape (EXP-480): icon · name ·
        // version | spacer | status dot + text · ⋯ — `min_w_0` down the name
        // side so only the NAME gives way.
        gpui_component::h_flex()
            .id(SharedString::from(format!("machine-{}", device.device_id)))
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .hover(|this| this.bg(theme.list_hover))
            .child(
                div()
                    .flex_shrink_0()
                    .child(Icon::new(kind_icon).xsmall().text_color(muted)),
            )
            .child(
                gpui_component::h_flex()
                    .flex_1()
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
                            .flex_shrink_0()
                            .text_sm()
                            .whitespace_nowrap()
                            .text_color(theme.foreground)
                            .child(label.clone()),
                    )
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
                    }),
            )
            .child(
                gpui_component::h_flex()
                    .flex_shrink_0()
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
            )
            .children(menu.map(|menu| div().flex_shrink_0().child(menu)))
            .into_any_element()
    }
}

/// EXP-409: online with NOTHING runnable — every installed agent is signed
/// out, so the row reads amber with the sign-in reason instead of "Online".
fn sign_in_needed(device: &api::devices::DeviceEntry) -> bool {
    device.online && device.agents.is_empty() && !device.unauthed_agents.is_empty()
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
        // Rendering IS the visibility signal (module doc).
        self.last_render = Instant::now();
        self.ensure_polling(cx);

        let muted = cx.theme().muted_foreground;
        let devices = self.devices.clone();
        let rows: Vec<gpui::AnyElement> = devices
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, device)| self.render_row(index, device, cx))
            .collect();

        // The web `SectionLabel` band (EXP-480): label · count · spacer ·
        // "Add server" — the same band design as the Actions section below.
        let add_server = Button::new("machines-add-server")
            .outline()
            .xsmall()
            .icon(registry::UI_ADD)
            .label("Add server")
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                this.prompt_add_server(window, cx);
            }))
            .into_any_element();
        let count = devices.as_ref().map(Vec::len).unwrap_or(0);

        gpui_component::v_flex()
            .w_full()
            .min_w_0()
            .child(crate::actions_view::section_band(
                "My machines",
                count,
                Some(add_server),
                cx,
            ))
            .when(devices.is_none(), |this| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading machines…"),
                )
            })
            .when(devices.as_ref().is_some_and(|rows| rows.is_empty()), |this| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            "No machines yet. Sign in on a desktop app, or install the \
                             exponential CLI on a server.",
                        ),
                )
            })
            .children(rows)
    }
}
