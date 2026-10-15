//! EXP-698 — the ONE composer card.
//!
//! Two surfaces type into a message box: the issue timeline's comment
//! composer (`comments::composer_row`) and the steer viewer's reply composer
//! (`steer_viewer::render_composer`); the helpdesk thread's reply/note
//! composer was the third until SLOP-4 (gone with the helpdesk). They used
//! to be three hand-built cards with three different radii, paddings and
//! submit affordances. They are one recipe now:
//!
//! ```text
//! ┌───────────────────────────────────────────┐  radius XL, FILL_CARD
//! │ [leading row]                             │  (Reply to reporter pill)
//! │ [strip]                                   │  attachment chips
//! │ the field                                 │  borderless, auto-grow
//! │ ⧉ ⌗ ☺                              ( ↑ )  │  tool row
//! └───────────────────────────────────────────┘
//! ```
//!
//! EXP-790: the submit slot is one round button that is either Send or
//! Stop ([`SubmitKind`]): same ring, the glyph swaps.
//!
//! EXP-877: the steer composer's model and context controls sit in a footer
//! UNDER the card ([`session_composer_footer`]); its attach "+" is back in
//! the card's tool row (EXP-1294).
//!
//! Every slot is optional except the field; the caller owns all state and
//! handlers, this only lays the card out.
//!
//! EXP-1294: [`session_composer_box`] is THE session composer box — Home's
//! launcher card (a 2–8 line field, the "+" bottom-left, the round send
//! bottom-right) on the work column. Dispatcher roots (2026-10-15): the
//! thread screen and the agent channel no longer build a reply of their
//! own — they host the ROOT session's composer (`session_screen::
//! root_composer_view`), so all three paint ONE composer: the box, the `/`
//! menu ([`session_slash_menu`]), the footer ([`session_composer_footer`]:
//! [`session_model_pin`] + [`session_context_ring`]), Send⇄Stop and the
//! queue strip.

use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Div, ElementId, Entity,
    IntoElement as _, ParentElement as _, SharedString, Styled, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};
use theme::tokens as t;

use crate::controls::WebControl as _;

/// The composer card's slots. Build it with [`GlassComposer::new`] and fill
/// what the surface has.
pub(crate) struct GlassComposer {
    field: AnyElement,
    leading: Option<AnyElement>,
    strip: Option<AnyElement>,
    tools: Vec<AnyElement>,
    submit: Option<AnyElement>,
}

/// EXP-790: what the composer's one round button does right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubmitKind {
    /// Send the draft (`ui-submit`, the circled arrow).
    Send,
    /// Stop the agent's running turn (`ui-stop`, the circled square).
    Stop,
}

impl SubmitKind {
    pub(crate) fn icon(self) -> crate::icons::ExpIcon {
        match self {
            SubmitKind::Send => crate::icons::registry::UI_SUBMIT,
            SubmitKind::Stop => crate::icons::registry::UI_STOP,
        }
    }
}

impl GlassComposer {
    pub(crate) fn new(field: impl Into<AnyElement>) -> Self {
        Self {
            field: field.into(),
            leading: None,
            strip: None,
            tools: Vec::new(),
            submit: None,
        }
    }

    /// A row INSIDE the card, above everything else — the issue composer's
    /// "Reply to reporter" pill (SLOP-4; the helpdesk's Reply / Internal
    /// note pills sat here before).
    pub(crate) fn leading(mut self, leading: impl Into<AnyElement>) -> Self {
        self.leading = Some(leading.into());
        self
    }

    /// The attachment/pending strip between the leading row and the field.
    pub(crate) fn strip(mut self, strip: Option<AnyElement>) -> Self {
        self.strip = strip;
        self
    }

    /// One leading tool of the bottom row — a 24px ghost glyph button
    /// ([`composer_tool`]).
    pub(crate) fn tool(mut self, tool: impl Into<AnyElement>) -> Self {
        self.tools.push(tool.into());
        self
    }

    /// The trailing round ghost submit ([`composer_submit`]).
    pub(crate) fn submit(mut self, submit: impl Into<AnyElement>) -> Self {
        self.submit = Some(submit.into());
        self
    }
}

/// Lay a [`GlassComposer`] out as the card. Returns a bare `Div` so the
/// caller can still hang a `capture_action` (the steer composer's paste
/// handler) or a mode tint on it.
pub(crate) fn glass_composer(composer: GlassComposer) -> Div {
    let GlassComposer {
        field,
        leading,
        strip,
        tools,
        submit,
    } = composer;
    let card = v_flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .p_2()
        .rounded(px(t::radius::XL))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_CARD.to_hsla())
        .children(leading)
        .children(strip);
    // EXP-525: a flex-COLUMN slot, not a nested row — the view child's
    // percent width resolved against unclamped avail in a row hop
    // (EXP-436 class) and the composer collapsed to placeholder width at
    // some window sizes; a column stretches its child to the definite
    // slot width instead.
    let field = v_flex().flex_1().w_full().min_w_0().child(field);
    card.child(field).child(
        h_flex()
            .w_full()
            .gap_1()
            .px_1()
            .pb_1()
            .items_center()
            .children(tools)
            .child(div().flex_1())
            .children(submit),
    )
}

/// One leading tool of the composer's bottom row: a muted 24px ghost glyph
/// button. The caller adds the tooltip, the disabled state and the handler.
pub(crate) fn composer_tool(
    id: impl Into<gpui::ElementId>,
    icon: crate::icons::ExpIcon,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost()
        .web_icon_xs()
        .icon(Icon::new(icon).text_color(cx.theme().muted_foreground))
}

/// EXP-599/EXP-698: the composer's send — a ROUND GHOST capsule around the
/// glyph, primary-tinted, never a filled button (the `ui-submit` glyph is
/// itself a circled arrow, and a filled box would draw a second ring around
/// it). `icon` differs by surface: `UI_SUBMIT` for comments, `UI_SEND` for
/// steer replies — same shape, same tint, same 32px hit box.
pub(crate) fn composer_submit(
    id: impl Into<gpui::ElementId>,
    icon: crate::icons::ExpIcon,
    disabled: bool,
    cx: &App,
) -> Button {
    composer_round(id, icon, disabled, cx)
}

/// EXP-790: the one round button as Send OR Stop — [`composer_submit`] with
/// the glyph picked by [`SubmitKind`]. Stop is never dimmed by an empty
/// draft (the caller passes `disabled` for its own reasons only).
pub(crate) fn composer_submit_kind(
    id: impl Into<gpui::ElementId>,
    kind: SubmitKind,
    disabled: bool,
    cx: &App,
) -> Button {
    composer_round(id, kind.icon(), disabled, cx)
}

fn composer_round(
    id: impl Into<gpui::ElementId>,
    icon: crate::icons::ExpIcon,
    disabled: bool,
    cx: &App,
) -> Button {
    // Web `disabled:opacity-40` — the explicit icon tint would otherwise
    // override the ghost variant's own disabled treatment.
    let tint = if disabled {
        cx.theme().primary.opacity(0.4)
    } else {
        cx.theme().primary
    };
    Button::new(id)
        .ghost()
        .with_size(px(t::size::CONTROL_MD))
        .rounded_full()
        .cursor_pointer()
        .icon(Icon::new(icon).text_color(tint))
        .disabled(disabled)
}

/// EXP-1294: the round button of [`session_composer_box`] — Send or Stop
/// ([`SubmitKind`]), its own disabled and loading state, the click handler.
pub(crate) struct SessionComposerSubmit {
    pub(crate) id: ElementId,
    pub(crate) kind: SubmitKind,
    /// The caller's reason to dim it (an empty draft); Stop passes `false`.
    pub(crate) disabled: bool,
    /// A send in flight (the steer view's `sending`).
    pub(crate) loading: bool,
    pub(crate) on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl SessionComposerSubmit {
    /// A Send button.
    pub(crate) fn send(
        id: impl Into<ElementId>,
        disabled: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            kind: SubmitKind::Send,
            disabled,
            loading: false,
            on_click: Box::new(on_click),
        }
    }

    pub(crate) fn kind(mut self, kind: SubmitKind) -> Self {
        self.kind = kind;
        self
    }

    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
}

/// A decoration hook on one of the box's divs — a key context, captured
/// actions (the steer view's slash keys and paste), a tint.
pub(crate) type DivHook = Box<dyn FnOnce(Div) -> Div>;

/// EXP-1294 — THE session composer box's slots. Only the field and the
/// submit are required; the session view fills every slot (the `/` menu, its
/// key captures, the pending images, the paste capture and the model /
/// context footer), the thread reply and the agent channel the field, the
/// pending images and the button.
pub(crate) struct SessionComposerBox {
    /// The mention-capable field ([`session_composer_field`]); its
    /// placeholder lives on the textarea state.
    pub(crate) mention: Entity<crate::mention_input::MentionInput>,
    /// Inside the card, above the field (the steer view's `/` menu).
    pub(crate) above_field: Option<AnyElement>,
    /// Applied to the field's wrapper div (key context + captured actions).
    pub(crate) field_hook: Option<DivHook>,
    /// The pending attachment strip.
    pub(crate) strip: Option<AnyElement>,
    /// The card's bottom-left tools (the "+" attach, [`composer_tool`]).
    pub(crate) tools: Vec<AnyElement>,
    pub(crate) submit: SessionComposerSubmit,
    /// The row UNDER the card (attach, model pin, context ring).
    pub(crate) footer: Option<AnyElement>,
    /// Applied to the card (the paste capture).
    pub(crate) card_hook: Option<DivHook>,
}

impl SessionComposerBox {
    pub(crate) fn new(
        mention: Entity<crate::mention_input::MentionInput>,
        submit: SessionComposerSubmit,
    ) -> Self {
        Self {
            mention,
            above_field: None,
            field_hook: None,
            strip: None,
            tools: Vec::new(),
            submit,
            footer: None,
            card_hook: None,
        }
    }

    pub(crate) fn above_field(mut self, element: Option<AnyElement>) -> Self {
        self.above_field = element;
        self
    }

    pub(crate) fn field_hook(mut self, hook: impl FnOnce(Div) -> Div + 'static) -> Self {
        self.field_hook = Some(Box::new(hook));
        self
    }

    pub(crate) fn strip(mut self, strip: Option<AnyElement>) -> Self {
        self.strip = strip;
        self
    }

    /// One bottom-left tool inside the card (Home's "+" slot).
    pub(crate) fn tool(mut self, tool: impl Into<AnyElement>) -> Self {
        self.tools.push(tool.into());
        self
    }

    pub(crate) fn footer(mut self, footer: Option<AnyElement>) -> Self {
        self.footer = footer;
        self
    }

    pub(crate) fn card_hook(mut self, hook: impl FnOnce(Div) -> Div + 'static) -> Self {
        self.card_hook = Some(Box::new(hook));
        self
    }
}

/// EXP-1294 — the field THE session composer box types into: Home's 2–8
/// line textarea (Enter sends, Shift+Enter breaks a line) with the §4.6
/// completions, chrome off (the card draws it). Returns the textarea state
/// (read and clear the draft through it) and the mention field the box
/// mounts; point the completions at a team with `MentionInput::set_source`.
pub(crate) fn session_composer_field(
    placeholder: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> (
    Entity<gpui_component::input::TextareaState>,
    Entity<crate::mention_input::MentionInput>,
) {
    let placeholder = placeholder.into();
    let input = cx.new(|cx| {
        crate::controls::web_textarea(2, 8, window, cx)
            .submit_on_enter(true)
            .placeholder(placeholder)
    });
    let mention = cx.new(|cx| {
        let mut mention = crate::mention_input::MentionInput::new(input.clone(), cx);
        mention.set_appearance(false);
        mention
    });
    (input, mention)
}

/// EXP-1294 — THE session composer box = Home's launcher card
/// (`chat_screen::render_launcher`): the field over a tool row with the "+"
/// bottom-left and the ONE round button bottom-right, the optional footer
/// under the card, both on the centred work column (EXP-1191:
/// `WORK_COLUMN_W` with the `WORK_GUTTER` padding) like the transcript.
pub(crate) fn session_composer_box(props: SessionComposerBox, cx: &App) -> AnyElement {
    let SessionComposerBox {
        mention,
        above_field,
        field_hook,
        strip,
        tools,
        submit,
        footer,
        card_hook,
    } = props;
    let field_wrap = div().w_full().min_w_0();
    let field_wrap = match field_hook {
        Some(hook) => hook(field_wrap),
        None => field_wrap,
    };
    let mut composer = GlassComposer::new(
        gpui_component::v_flex()
            .w_full()
            .min_w_0()
            .children(above_field)
            .child(field_wrap.child(mention))
            .into_any_element(),
    )
    .strip(strip);
    for tool in tools {
        composer = composer.tool(tool);
    }
    let SessionComposerSubmit {
        id,
        kind,
        disabled,
        loading,
        on_click,
    } = submit;
    let composer = composer.submit(
        composer_submit_kind(id, kind, disabled, cx)
            .loading(loading)
            .on_click(move |event, window, cx| on_click(event, window, cx)),
    );
    let card = glass_composer(composer);
    let card = match card_hook {
        Some(hook) => hook(card),
        None => card,
    };
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_center()
        .py_2()
        .child(
            gpui_component::v_flex()
                .w_full()
                .min_w_0()
                .max_w(px(crate::work_header::WORK_COLUMN_W))
                .px(px(crate::work_header::WORK_GUTTER))
                .child(card)
                .children(footer),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// THE session composer's footer, model pin, context ring and `/` menu — ONE
// implementation for every surface that types at a session: the session
// view (`steer_viewer::render_composer`), the thread screen and the agent
// channel (both host the ROOT session's composer, `steer_viewer::
// render_session_composer`), and the styleguide's thread parts.
// ---------------------------------------------------------------------------

/// EXP-847: the footer's read-only plan read (`config_state.currentMode`).
pub(crate) const PLAN_MODE_FOOTER: &str = "Plan mode";

/// EXP-877 — the row UNDER the composer card. Left: the plan-mode read
/// (`plan`, never on a dispatcher root). Right: which model the run is on
/// ([`session_model_pin`]) and how full its context window is
/// ([`session_context_ring`]). Everything here is ABOUT the run rather than
/// part of the message (EXP-1294: the attach "+" sits inside the card).
pub(crate) fn session_composer_footer(
    plan: bool,
    model_pin: Option<AnyElement>,
    context_ring: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    use gpui::prelude::FluentBuilder as _;
    let muted = cx.theme().muted_foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1()
        .pt_1()
        .px_1()
        .when(plan, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(t::BLUE.to_hsla())
                    .child(SharedString::from(PLAN_MODE_FOOTER)),
            )
        })
        .child(div().flex_1().min_w_0())
        .children(model_pin)
        .children(context_ring)
        .text_color(muted)
        .into_any_element()
}

/// EXP-877 — THE model pin: the model the run is on (`value`, off
/// `config_state.options[model]`). `picker` (claude) makes it a menu over THE
/// model list ([`crate::coding_selects::MODEL_CHOICES`], all four, no
/// default row); picking calls `on_pick` with the alias (the session view
/// sends `/model <alias>`). Codex has no such prompt, so its model is a
/// plain label. `None` for a blank value: a footer reports what the run IS
/// on, and a blank names no model.
pub(crate) fn session_model_pin<V: gpui::Render>(
    id: impl Into<ElementId>,
    value: &str,
    picker: bool,
    enabled: bool,
    on_pick: impl Fn(&mut V, &str, &mut Window, &mut gpui::Context<V>) + 'static,
    cx: &mut gpui::Context<V>,
) -> Option<AnyElement> {
    if value.trim().is_empty() {
        return None;
    }
    let label = SharedString::from(crate::steer_viewer::model_label(value));
    if !picker {
        return Some(
            div()
                .flex_shrink_0()
                .px_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label)
                .into_any_element(),
        );
    }
    let trigger = Button::new(id)
        .ghost()
        .cursor_pointer()
        .xsmall()
        .label(label)
        .disabled(!enabled);
    Some(
        crate::launch_options::choice_pin(
            trigger,
            &crate::coding_selects::MODEL_CHOICES,
            value.to_string(),
            on_pick,
            cx,
        )
        .into_any_element(),
    )
}

/// What [`session_context_ring`] reads: the run's usage + seeded layout, and
/// where the run lives (the sheet's account windows and its switch).
pub(crate) struct ContextRingProps {
    pub(crate) id: SharedString,
    pub(crate) session_id: String,
    pub(crate) device_id: Option<String>,
    pub(crate) agent: Option<coding::CodingAgent>,
    pub(crate) local: bool,
    pub(crate) working: bool,
    pub(crate) usage: Option<steer::SessionUsage>,
    pub(crate) layout: Vec<steer::ContextSegment>,
    pub(crate) legend_open: bool,
    /// Flips the host's legend fold (EXP-1051: the sheet is a pure render).
    pub(crate) on_toggle_legend: std::rc::Rc<dyn Fn(&mut Window, &mut App)>,
}

/// EXP-877 — THE context ring and the usage sheet it opens. `None` when the
/// run published no window.
pub(crate) fn session_context_ring(props: ContextRingProps, cx: &App) -> Option<AnyElement> {
    let ContextRingProps {
        id,
        session_id,
        device_id,
        agent,
        local,
        working,
        usage,
        layout,
        legend_open,
        on_toggle_legend,
    } = props;
    let percent = crate::usage_bar::context_percent(usage.as_ref())?;
    let ring = crate::usage_sheet::context_ring(id.clone(), percent, usage.as_ref(), cx);
    let open_session_id = session_id.clone();
    let open_device_id = device_id.clone();
    Some(
        gpui_component::popover::Popover::new(SharedString::from(format!("{id}-usage")))
            .p_2()
            .trigger(ring)
            // EXP-909: opening the sheet asks for ONE fresh read of the run's
            // own login (a glance, not a Refresh; no polling while open).
            .on_open_change(move |open, _window, cx| {
                if !*open {
                    return;
                }
                let Some(agent) = agent else {
                    return;
                };
                crate::steer_viewer::refresh_run_usage(
                    &open_session_id,
                    open_device_id.as_deref(),
                    agent,
                    local,
                    cx,
                );
            })
            .content(move |_, _window, cx| {
                // The run's OWN login's rate-limit windows: the local agent
                // status for a run hosted here, the synced `devices` row for
                // one hosted anywhere else (EXP-875 §1).
                let windows = agent.and_then(|agent| {
                    if local {
                        let account = crate::steer_viewer::run_account(&session_id, cx);
                        let (accounts, usage) = crate::device_settings::own_agent_status(cx);
                        crate::usage_sheet::account_windows(&accounts, &usage, agent, account.as_deref())
                    } else {
                        crate::usage_sheet::device_usage(device_id.as_deref(), agent, cx)
                    }
                });
                let switch = agent.map(|agent| crate::account_switch::SwitchContext {
                    session_id: session_id.clone(),
                    device_id: device_id.clone(),
                    local,
                    agent,
                    working,
                });
                crate::usage_sheet::render_usage_sheet(
                    agent,
                    usage.as_ref(),
                    &layout,
                    legend_open,
                    on_toggle_legend.clone(),
                    windows.as_ref(),
                    switch,
                    cx,
                )
            })
            .into_any_element(),
    )
}

/// EXP-724 — THE `/` command menu INSIDE the composer card, above the field:
/// mono name, muted argument hint, muted description. Hovering MOVES the
/// selection (`on_hover(view, index)`), a press accepts it
/// (`on_pick(view, index)`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn session_slash_menu<V: 'static>(
    id: &'static str,
    row_id: &'static str,
    items: &[crate::slash_commands::MenuCommand],
    selected: usize,
    scroll: &gpui::ScrollHandle,
    on_hover: fn(&mut V, usize, &mut gpui::Context<V>),
    on_pick: fn(&mut V, usize, &mut Window, &mut gpui::Context<V>),
    window: &Window,
    cx: &mut gpui::Context<V>,
) -> AnyElement {
    use gpui::prelude::FluentBuilder as _;
    use gpui::InteractiveElement as _;
    use gpui::StatefulInteractiveElement as _;
    let muted = cx.theme().muted_foreground;
    let mut rows: Vec<AnyElement> = Vec::with_capacity(items.len());
    for (index, command) in items.iter().enumerate() {
        rows.push(
            crate::controls::typeahead_row((row_id, index), index == selected, cx)
                // EXP-892: hovering MOVES the selection onto the row under
                // the pointer, it never paints a second highlight.
                .on_hover(cx.listener(move |view, hovered: &bool, _window, cx| {
                    if *hovered {
                        on_hover(view, index, cx);
                    }
                }))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, window, cx| on_pick(view, index, window, cx)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(SharedString::from(format!("/{}", command.name))),
                )
                .when(!command.arg_hint.is_empty(), |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(command.arg_hint.to_string())),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(command.description.to_string())),
                )
                .into_any_element(),
        );
    }
    // EXP-970: the ONE typeahead surface, in the composer card's flow.
    crate::controls::typeahead_menu(
        id,
        crate::controls::TypeaheadArm::Inline,
        rows,
        scroll,
        window,
        cx,
    )
}
