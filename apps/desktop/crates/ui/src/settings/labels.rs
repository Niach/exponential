//! Settings → Labels (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/labels-section.tsx` — inline label rows
//! (color swatch `Popover` + borderless name `Input` persisting on
//! blur/Enter + inline "Delete?" confirm) and the create form (name input +
//! `ColorSwatchGrid` + Create/Cancel).
//!
//! Reads come from the synced `labels` collection; create/update/delete are
//! §4.1 un-gated tRPC mutations reflected by the Electric echo. Per-label
//! `InputState`s are created lazily in window-aware observers/render — like
//! the web's per-row `useState(label.name)`, they do NOT resync on external
//! edits until the row remounts.

use std::collections::HashMap;

use gpui::{
    div, App, AppContext as _, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    popover::Popover,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use domain::rows::Label;

use crate::navigation::{active_team_id, Navigation};

use super::{card, card_header, parse_hex_color, spawn_trpc};

/// Web `LABEL_COLORS` (lib/label-colors.ts) — verbatim.
pub(crate) const LABEL_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

pub struct LabelsPane {
    nav: Entity<Navigation>,
    /// label id → its name input (created lazily with the window in scope).
    name_inputs: HashMap<String, Entity<InputState>>,
    input_subs: HashMap<String, Subscription>,
    confirming_delete: Option<String>,
    creating: bool,
    new_name: Entity<InputState>,
    new_color: String,
    submitting: bool,
    /// Inline error under the create form (duplicate name / server reject).
    create_error: Option<String>,
    /// (label id, message) — inline error under a row whose rename failed.
    row_error: Option<(String, String)>,
    _subscriptions: Vec<Subscription>,
}

/// Web `A label with this name already exists.` — shown for the local
/// duplicate pre-check on create and rename (EXP-254).
const DUPLICATE_NAME_MESSAGE: &str = "A label with this name already exists.";

impl LabelsPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("Label name"));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe_in(&nav, window, |this, _, window, cx| {
                // Team switch: per-row inputs (and any inline errors) belong
                // to the old scope.
                this.confirming_delete = None;
                this.create_error = None;
                this.row_error = None;
                this.sync_inputs(window, cx);
                cx.notify();
            }),
            cx.observe_in(&collections.labels, window, |this, _, window, cx| {
                this.sync_inputs(window, cx);
                cx.notify();
            }),
            // Enter in the create form submits (web onKeyDown Enter).
            cx.subscribe_in(&new_name, window, |this, _, event: &InputEvent, _, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.create(cx),
                    InputEvent::Change => {
                        // Typing clears a stale server error (the live
                        // duplicate check re-derives in render).
                        this.create_error = None;
                        cx.notify();
                    }
                    _ => {}
                }
            }),
        ];

        let mut this = Self {
            nav,
            name_inputs: HashMap::new(),
            input_subs: HashMap::new(),
            confirming_delete: None,
            creating: false,
            new_name,
            new_color: LABEL_COLORS[6].to_string(),
            submitting: false,
            create_error: None,
            row_error: None,
            _subscriptions: subscriptions,
        };
        this.sync_inputs(window, cx);
        this
    }

    /// Web `isDuplicateName`: another label in the team already has this name
    /// (case-insensitive, matching the server's (team_id, lower(name)) unique).
    fn is_duplicate_name(&self, name: &str, exclude_id: Option<&str>, cx: &App) -> bool {
        let needle = name.trim().to_lowercase();
        self.scoped_labels(cx).iter().any(|label| {
            Some(label.id.as_str()) != exclude_id && label.name.trim().to_lowercase() == needle
        })
    }

    /// The team's labels, `sortOrder` then name (web `orderBy`).
    fn scoped_labels(&self, cx: &App) -> Vec<Label> {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return Vec::new();
        };
        let mut labels: Vec<Label> = Store::global(cx)
            .collections()
            .labels
            .read(cx)
            .iter()
            .filter(|label| label.team_id == team_id)
            .cloned()
            .collect();
        labels.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(f64::MAX)
                .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        labels
    }

    /// Ensure one `InputState` per visible label; drop stale ones. Runs in
    /// window-aware observers (InputState construction needs the window).
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let labels = self.scoped_labels(cx);
        let live: std::collections::HashSet<&str> =
            labels.iter().map(|label| label.id.as_str()).collect();
        self.name_inputs.retain(|id, _| live.contains(id.as_str()));
        self.input_subs.retain(|id, _| live.contains(id.as_str()));

        for label in &labels {
            if self.name_inputs.contains_key(&label.id) {
                continue;
            }
            let input = cx.new(|cx| {
                InputState::new(window, cx).default_value(label.name.clone())
            });
            let label_id = label.id.clone();
            let sub = cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            this.persist_name(&label_id, window, cx);
                        }
                        InputEvent::Change => {
                            // Typing clears the row's stale duplicate error.
                            if this
                                .row_error
                                .as_ref()
                                .is_some_and(|(id, _)| id == &label_id)
                            {
                                this.row_error = None;
                                cx.notify();
                            }
                        }
                        _ => {}
                    }
                },
            );
            self.name_inputs.insert(label.id.clone(), input);
            self.input_subs.insert(label.id.clone(), sub);
        }
    }

    /// Web `persistName`: trim; empty or unchanged resets to the synced name,
    /// otherwise `labels.update`.
    fn persist_name(
        &mut self,
        label_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(input) = self.name_inputs.get(label_id).cloned() else {
            return;
        };
        let synced = Store::global(cx)
            .collections()
            .labels
            .read(cx)
            .get(label_id)
            .cloned();
        let Some(label) = synced else { return };
        let typed = input.read(cx).value().trim().to_string();
        if typed.is_empty() || typed == label.name {
            let name = label.name.clone();
            input.update(cx, |state, cx| state.set_value(name, window, cx));
            self.row_error = None;
            cx.notify();
            return;
        }
        // Web `persistName` duplicate pre-check: keep the typed name so the
        // user can fix it, show the inline error, skip the mutation (the
        // server enforces the same unique either way — EXP-254).
        if self.is_duplicate_name(&typed, Some(label_id), cx) {
            self.row_error = Some((label_id.to_string(), DUPLICATE_NAME_MESSAGE.to_string()));
            cx.notify();
            return;
        }
        self.row_error = None;
        let team_id = label.team_id.clone();
        let label_id = label_id.to_string();
        spawn_trpc(cx, "labels.update(name)", move |trpc| {
            api::labels::labels_update(trpc, &team_id, &label_id, Some(&typed), None)
        });
    }

    fn create(&mut self, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        let name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() || self.is_duplicate_name(&name, None, cx) {
            return;
        }
        let color = self.new_color.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        self.submitting = true;
        self.create_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::labels::labels_create(&trpc, &team_id, &name, Some(&color))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.submitting = false;
                if let Err(err) = &result {
                    log::warn!("[ui] labels.create failed: {err}");
                    // Server reject (e.g. the duplicate-name CONFLICT racing
                    // a not-yet-synced label) — show its clean message inline.
                    this.create_error = Some(match err {
                        api::ApiError::Http { message, .. } => message.clone(),
                        other => other.to_string(),
                    });
                } else {
                    this.creating = false;
                    this.reset_form();
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Web `resetForm`: random color for the next label (the input value
    /// itself resets in the click handlers, where the window is available).
    fn reset_form(&mut self) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(0);
        self.new_color = LABEL_COLORS[nanos % LABEL_COLORS.len()].to_string();
        self.create_error = None;
    }

    fn render_label_row(&self, label: &Label, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let color = label.color.clone().unwrap_or_default();
        let swatch_color = parse_hex_color(&color).unwrap_or(cx.theme().muted_foreground);
        let confirming = self.confirming_delete.as_deref() == Some(label.id.as_str());
        let label_id = label.id.clone();
        let team_id = label.team_id.clone();

        let mut row = h_flex()
            .gap_3()
            .items_center()
            .px_3()
            .py_1p5()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            // Color swatch popover (web `Popover` + `ColorSwatchGrid`).
            .child(
                Popover::new(row_id("label-color", &label.id))
                    .trigger(
                        Button::new(row_id("label-color-trigger", &label.id))
                            .ghost()
                            .xsmall()
                            .child(
                                div()
                                    .size_4()
                                    .rounded_full()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .bg(swatch_color),
                            ),
                    )
                    .content({
                        let current = color.clone();
                        let label_id = label_id.clone();
                        let team_id = team_id.clone();
                        move |_, _, cx| {
                            let popover = cx.entity();
                            let label_id = label_id.clone();
                            let team_id = team_id.clone();
                            swatch_grid(
                                &format!("label-swatch-{label_id}"),
                                Some(current.as_str()),
                                move |picked, window, cx| {
                                    let team_id = team_id.clone();
                                    let label_id = label_id.clone();
                                    let picked = picked.to_string();
                                    spawn_trpc(cx, "labels.update(color)", move |trpc| {
                                        api::labels::labels_update(
                                            trpc,
                                            &team_id,
                                            &label_id,
                                            None,
                                            Some(&picked),
                                        )
                                    });
                                    popover.update(cx, |state, cx| state.dismiss(window, cx));
                                },
                                cx,
                            )
                        }
                    }),
            );

        // Borderless name input (web `border-none shadow-none`). `flex_1`
        // goes ON the Input (upstream input_story pattern): its root sizes
        // itself with a percent width, which collapses to content width
        // inside a flex-basis-0 wrapper div — cutting the name to ~1 char.
        if let Some(input) = self.name_inputs.get(&label.id) {
            row = row.child(Input::new(input).small().appearance(false).flex_1().min_w_0());
        } else {
            row = row.child(
                div()
                    .flex_1()
                    .text_sm()
                    .child(SharedString::from(label.name.clone())),
            );
        }

        if confirming {
            let del_team = team_id.clone();
            let del_label = label_id.clone();
            row = row.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Delete?"),
                    )
                    .child(
                        Button::new(row_id("label-delete-confirm", &label.id))
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(IconName::Check).text_color(cx.theme().danger))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let team_id = del_team.clone();
                                let label_id = del_label.clone();
                                this.confirming_delete = None;
                                spawn_trpc(cx, "labels.delete", move |trpc| {
                                    api::labels::labels_delete(trpc, &team_id, &label_id)
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(row_id("label-delete-cancel", &label.id))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirming_delete = None;
                                cx.notify();
                            })),
                    ),
            );
        } else {
            let confirm_id = label_id.clone();
            row = row.child(
                Button::new(row_id("label-delete", &label.id))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Delete)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirming_delete = Some(confirm_id.clone());
                        cx.notify();
                    })),
            );
        }

        // Web LabelRow: the error line renders under the row (inside its
        // border box on web; a stacked line here).
        let error = self
            .row_error
            .as_ref()
            .filter(|(id, _)| id == &label.id)
            .map(|(_, message)| message.clone());
        v_flex().gap_1().child(row).when_some(error, |col, message| {
            col.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .px_1()
                    .child(SharedString::from(message)),
            )
        })
    }
}

impl Render for LabelsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let labels = self.scoped_labels(cx);

        let mut body = card(cx).child(card_header(
            "Labels",
            format!(
                "{} label{} in this team. Deleting a label removes it from all issues.",
                labels.len(),
                if labels.len() == 1 { "" } else { "s" }
            ),
            cx,
        ));

        let mut list = v_flex().gap_2();
        for label in &labels {
            list = list.child(self.render_label_row(label, cx));
        }
        if labels.is_empty() && !self.creating {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .py_2()
                    .child("No labels yet."),
            );
        }
        body = body.child(list);

        if self.creating {
            let name = self.new_name.read(cx).value().trim().to_string();
            // Web `newNameIsDuplicate`: live check against the synced labels;
            // the message shows and Create disables before any round-trip.
            let duplicate = !name.is_empty() && self.is_duplicate_name(&name, None, cx);
            let form_error = if duplicate {
                Some(DUPLICATE_NAME_MESSAGE.to_string())
            } else {
                self.create_error.clone()
            };
            let entity = cx.entity();
            body = body.child(
                v_flex()
                    .gap_3()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.new_name).small())
                    .when_some(form_error, |col, message| {
                        col.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(SharedString::from(message)),
                        )
                    })
                    .child(
                        v_flex()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Color"),
                            )
                            .child(swatch_grid(
                                "new-label-swatch",
                                Some(self.new_color.as_str()),
                                move |picked, _, cx| {
                                    let picked = picked.to_string();
                                    entity.update(cx, |this, cx| {
                                        this.new_color = picked;
                                        cx.notify();
                                    });
                                },
                                cx,
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("label-create")
                                    .primary()
                                    .xsmall()
                                    .label(if self.submitting {
                                        "Creating…"
                                    } else {
                                        "Create label"
                                    })
                                    .disabled(name.is_empty() || self.submitting || duplicate)
                                    .loading(self.submitting)
                                    .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                            )
                            .child(
                                Button::new("label-create-cancel")
                                    .ghost()
                                    .xsmall()
                                    .label("Cancel")
                                    .disabled(self.submitting)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.creating = false;
                                        this.new_name.update(cx, |state, cx| {
                                            state.set_value("", window, cx)
                                        });
                                        this.reset_form();
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        } else {
            body = body.child(
                h_flex().child(
                    Button::new("label-new")
                        .outline()
                        .small()
                        .icon(IconName::Plus)
                        .label("New label")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.creating = true;
                            this.new_name
                                .update(cx, |state, cx| state.set_value("", window, cx));
                            cx.notify();
                        })),
                ),
            );
        }

        v_flex().child(body)
    }
}

/// Web `ColorSwatchGrid`: the fixed 20-color palette as clickable dots, ring
/// on the selected one. Shared by the label rows' popover and the create
/// form.
fn swatch_grid(
    id_prefix: &str,
    current: Option<&str>,
    on_pick: impl Fn(&str, &mut Window, &mut gpui::App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let mut grid = h_flex().flex_wrap().gap_1p5().w(gpui::px(190.));
    for color in LABEL_COLORS {
        let selected = current == Some(color);
        let fill = parse_hex_color(color).unwrap_or(cx.theme().muted_foreground);
        let on_pick = on_pick.clone();
        grid = grid.child(
            div()
                .id(ElementId::Name(SharedString::from(format!(
                    "{id_prefix}-{color}"
                ))))
                .size_5()
                .rounded_full()
                .bg(fill)
                .cursor_pointer()
                .when(selected, |dot| {
                    dot.border_2().border_color(cx.theme().foreground)
                })
                .on_click(move |_, window, cx| on_pick(color, window, cx)),
        );
    }
    grid
}

use gpui::prelude::FluentBuilder as _;

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}
