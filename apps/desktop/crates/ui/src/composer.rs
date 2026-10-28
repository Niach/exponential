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
//! N-12 (the iOS row ×4): the card's bottom row holds the "+" options menu
//! (the only tool), the run's controls ([`SessionComposerControls`]: the
//! Device glyph, the model pin; S-3: no Plan switch), then the usage ring
//! beside the round button; nothing sits under the card.
//!
//! Every slot is optional except the field; the caller owns all state and
//! handlers, this only lays the card out.
//!
//! EXP-1294: [`session_composer_card`] is THE session composer box — Home's
//! launcher card (a 2–8 line field, the "+" bottom-left, the round send
//! bottom-right) on the work column. Dispatcher roots (2026-10-15): the
//! thread screen and the agent channel no longer build a reply of their
//! own — they host the ROOT session's composer (`session_screen::
//! root_composer_view`), so all three paint ONE composer: the box, the `/`
//! menu ([`session_slash_menu`]), the run controls ([`SessionComposerControls`]:
//! [`session_model_pin`], [`session_context_ring`]),
//! Send⇄Stop and the queue strip.

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
    /// N-14: the bottom row is a ROW host ([`GlassComposer::row_host`]).
    row_host: Option<ElementId>,
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
            row_host: None,
        }
    }

    /// N-14 (`picker-presentation.json`): the bottom row (the tools, the
    /// pins, the submit) is a ROW host — an inline pin opened in it
    /// discloses its rows UNDER THE WHOLE ROW, as wide as it, and the pins
    /// never move ([`crate::picker::InlinePickerRow`]).
    pub(crate) fn row_host(mut self, id: impl Into<ElementId>) -> Self {
        self.row_host = Some(id.into());
        self
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
        row_host,
    } = composer;
    let card = v_flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .p_2()
        .rounded(px(t::radius::XL))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(crate::dock::card_fill())
        .children(leading)
        .children(strip);
    // EXP-525: a flex-COLUMN slot, not a nested row — the view child's
    // percent width resolved against unclamped avail in a row hop
    // (EXP-436 class) and the composer collapsed to placeholder width at
    // some window sizes; a column stretches its child to the definite
    // slot width instead.
    let field = v_flex().flex_1().w_full().min_w_0().child(field);
    let row = h_flex()
        .w_full()
        .gap_1()
        .px_1()
        .pb_1()
        .items_center()
        .children(tools)
        .child(div().flex_1())
        .children(submit);
    // NEW-50: the field ABOVE the tool row, always (N-12's row host had
    // dropped it: the session composer drew its tools with no field).
    let card = card.child(field);
    match row_host {
        Some(id) => card.child(crate::picker::InlinePickerRow::new(id, row)),
        None => card.child(row),
    }
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

/// EXP-1294: the round button of [`session_composer_card`] — Send or Stop
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
    /// The run's controls in the card's bottom row (model pin, plan read,
    /// context ring).
    pub(crate) controls: SessionComposerControls,
    /// Applied to the card (the paste capture).
    pub(crate) card_hook: Option<DivHook>,
    /// EXP-1211: the quick replies row ([`composer_replies_row`]), directly
    /// above the card in the same column.
    pub(crate) replies: Option<AnyElement>,
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
            controls: SessionComposerControls::default(),
            card_hook: None,
            replies: None,
        }
    }

    pub(crate) fn replies(mut self, element: Option<AnyElement>) -> Self {
        self.replies = element;
        self
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

    pub(crate) fn controls(mut self, controls: SessionComposerControls) -> Self {
        self.controls = controls;
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

/// N-12 — THE session composer card ×4 (the iOS session view's row): the
/// field over ONE tool row: the "+" (the only tool) · the Device pin as its
/// glyph alone · the model pin · the Plan switch … the context ring · Send
/// ⇄ Stop, all INSIDE the card; nothing under it. The quick replies sit
/// directly above the card.
pub(crate) fn session_composer_card(props: SessionComposerBox, cx: &App) -> AnyElement {
    let SessionComposerBox {
        mention,
        above_field,
        field_hook,
        strip,
        tools,
        submit,
        controls,
        card_hook,
        replies,
    } = props;
    let field_wrap = div().w_full().min_w_0();
    let field_wrap = match field_hook {
        Some(hook) => hook(field_wrap),
        None => field_wrap,
    };
    // N-14: THE one row is a row host: an opened pin (the device pin, the
    // model pin) discloses its rows under the whole row.
    let mut composer = GlassComposer::new(
        gpui_component::v_flex()
            .w_full()
            .min_w_0()
            .children(above_field)
            .child(field_wrap.child(mention))
            .into_any_element(),
    )
    .strip(strip)
    .row_host(ElementId::Name(format!("{:?}-row", submit.id).into()));
    for tool in tools {
        composer = composer.tool(tool);
    }
    let SessionComposerControls {
        device_pin,
        model_pin,
        context_ring,
    } = controls;
    // The run's options after the "+" (web `pl-1 gap-x-2.5`, xs muted).
    if device_pin.is_some() || model_pin.is_some() {
        composer = composer.tool(
            h_flex()
                .min_w_0()
                .pl_1()
                .gap(px(10.))
                .items_center()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .children(device_pin)
                .children(model_pin)
                .into_any_element(),
        );
    }
    let SessionComposerSubmit {
        id,
        kind,
        disabled,
        loading,
        on_click,
    } = submit;
    let button = composer_submit_kind(id, kind, disabled, cx)
        .loading(loading)
        .on_click(move |event, window, cx| on_click(event, window, cx));
    let composer = composer.submit(
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .children(context_ring)
            .child(button)
            .into_any_element(),
    );
    let card = glass_composer(composer);
    let card = match card_hook {
        Some(hook) => hook(card),
        None => card,
    };
    gpui_component::v_flex()
        .w_full()
        .min_w_0()
        .children(replies.map(|replies| div().w_full().min_w_0().pb_2().child(replies)))
        .child(card)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// THE session composer's footer, model pin, context ring and `/` menu — ONE
// implementation for every surface that types at a session: the session
// view (`steer_viewer::render_composer`), the thread screen and the agent
// channel (both host the ROOT session's composer, `steer_viewer::
// render_session_composer`), and the styleguide's thread parts.
// ---------------------------------------------------------------------------

/// EXP-877 — the run's controls in THE session composer's bottom row
/// (web `session-composer.tsx`): the Device glyph, [`session_model_pin`] and
/// the plan read (`plan`) after the "+", [`session_context_ring`] beside
/// Send ⇄ Stop (N-12: all inside the card). Everything here is ABOUT the run
/// rather than part of the message.
#[derive(Default)]
pub(crate) struct SessionComposerControls {
    /// EXP-1252: the Device pin BEFORE the model
    /// ([`crate::device_move::session_device_pin`]): the session's device,
    /// a pick moves it there.
    pub(crate) device_pin: Option<AnyElement>,
    pub(crate) model_pin: Option<AnyElement>,
    pub(crate) context_ring: Option<AnyElement>,
}


/// EXP-877 — THE model pin: the model the run is on (`value`, off
/// `config_state.options[model]`). `picker` (claude) makes it a menu over THE
/// model list ([`crate::coding_selects::MODEL_CHOICES`], all four, no
/// default row); picking calls `on_pick` with the alias (the session view
/// sends `/model <alias>`). Codex has no such prompt, so its pin is the same
/// word + chevron, disabled (web's disabled `ModelPicker`). `None` for a
/// blank value: the pin reports what the run IS on, and a blank names no
/// model.
pub(crate) fn session_model_pin<V: gpui::Render>(
    id: impl Into<SharedString>,
    value: &str,
    picker: bool,
    enabled: bool,
    on_pick: impl Fn(&mut V, &str, &mut Window, &mut gpui::Context<V>) + 'static,
    cx: &mut gpui::Context<V>,
) -> Option<AnyElement> {
    if value.trim().is_empty() {
        return None;
    }
    let label = crate::steer_viewer::model_label(value).to_string();
    // Web's `inline` picker trigger: the muted word + a 12px chevron, no
    // chrome (the IDE twin = the composer options line's pin).
    let trigger = crate::launch_options::inline_pin_trigger(id.into(), label, cx).px_0();
    if !picker {
        return Some(trigger.disabled(true).into_any_element());
    }
    let trigger = trigger.disabled(!enabled);
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

/// The model a session runs on BEFORE its relay says (no `config_state`
/// yet, or no live room at all): the row's own pick, else the device's
/// model for the row's agent, else the contract defaults. `device_agents` = the devices row's
/// `launch_defaults.agents`. Web `fallbackModel` (session-composer.tsx).
pub(crate) fn fallback_session_model(
    row: Option<&domain::rows::SessionRow>,
    device_agents: Option<&serde_json::Value>,
) -> String {
    use domain::contract as c;
    let agent = row.and_then(|row| row.cli.as_deref()).unwrap_or("claude");
    if let Some(model) = row.and_then(|row| row.model.as_deref()).filter(|m| !m.is_empty()) {
        return model.to_string();
    }
    let pick = |key: &str| {
        device_agents
            .and_then(|agents| agents.get(agent))
            .and_then(|defaults| defaults.get(key))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let codex = agent == "codex";
    pick("model").unwrap_or_else(|| {
        if codex { c::DEVICE_AGENT_DEFAULTS_CODEX_MODEL } else { c::DEVICE_AGENT_DEFAULTS_MODEL }.to_string()
    })
}

/// [`fallback_session_model`] off the synced stores: the row's device is
/// looked up in the `devices` shape (`sessions.device_id` = its row id).
pub(crate) fn session_fallback_model(row: Option<&domain::rows::SessionRow>, cx: &App) -> String {
    let device = row
        .and_then(|row| row.device_id.as_deref())
        .and_then(|id| sync::Store::global(cx).collections().devices.read(cx).get(id).cloned());
    let agents = device
        .as_ref()
        .and_then(|device| device.launch_defaults.as_ref())
        .and_then(|defaults| defaults.get("agents"));
    fallback_session_model(row, agents)
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
/// mono name, muted argument hint, muted description; NEW-2: then the team's
/// prompts, icon + name. Hovering MOVES the
/// selection (`on_hover(view, index)`), a press accepts it
/// (`on_pick(view, index)`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn session_slash_menu<V: 'static>(
    id: &'static str,
    row_id: &'static str,
    items: &[crate::slash_commands::SlashItem],
    selected: usize,
    scroll: &gpui::ScrollHandle,
    on_hover: fn(&mut V, usize, &mut gpui::Context<V>),
    on_pick: fn(&mut V, usize, &mut Window, &mut gpui::Context<V>),
    window: &Window,
    cx: &mut gpui::Context<V>,
) -> AnyElement {
    use crate::slash_commands::SlashItem;
    use gpui::prelude::FluentBuilder as _;
    use gpui::InteractiveElement as _;
    use gpui::StatefulInteractiveElement as _;
    let muted = cx.theme().muted_foreground;
    let mut rows: Vec<AnyElement> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let row = crate::controls::typeahead_row((row_id, index), index == selected, cx)
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
            );
        let row = match item {
            SlashItem::Command(command) => row
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
                ),
            // NEW-2 (web `PromptCandidateRow`): the prompt's icon, muted,
            // and its name.
            SlashItem::Prompt(prompt) => row
                .child(
                    crate::icons::action_icon(prompt.icon.as_deref())
                        .small()
                        .flex_shrink_0()
                        .text_color(muted),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .child(SharedString::from(prompt.name.clone())),
                ),
        };
        rows.push(row.into_any_element());
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

// ── EXP-1211: quick replies above the session composer ─────────────────────

/// The quick reply writes in flight (`<message>:<item>`, the ×'s
/// `<message>:*`): a pill (or the ×) locks until its write lands.
#[derive(Default)]
struct PendingReplies(std::collections::HashSet<String>);

impl gpui::Global for PendingReplies {}

fn reply_pending(key: &str, cx: &App) -> bool {
    cx.try_global::<PendingReplies>().is_some_and(|pending| pending.0.contains(key))
}

/// One quick reply write, once per key while in flight; a failure is the
/// `Couldn't <verb> <thing>` toast.
fn reply_write(
    key: String,
    verb: &'static str,
    thing: &'static str,
    cx: &mut App,
    call: impl FnOnce(&api::TrpcClient) -> Result<serde::de::IgnoredAny, api::ApiError> + Send + 'static,
) {
    if !cx.default_global::<PendingReplies>().0.insert(key.clone()) {
        return;
    }
    cx.refresh_windows();
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        cx.default_global::<PendingReplies>().0.remove(&key);
        crate::action_error::failed_action(verb, thing, None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { call(&trpc) }).await;
        cx.update(|cx| {
            cx.default_global::<PendingReplies>().0.remove(&key);
            cx.refresh_windows();
            if let Err(err) = result {
                log::warn!("[composer] {verb} {thing} failed: {err}");
                crate::action_error::failed_action_err(verb, thing, &err, cx);
            }
        });
    })
    .detach();
}

/// The row above a session's composer box (`session-thread.json`
/// `copy.replies`, web `ComposerReplies`): one sm pill per open reply
/// (`sessions.sendReply`), wrapping, then a ghost × (`sessions.
/// dismissReplies`). It rides THE composer box's `replies` slot, so it goes
/// whenever the ask dock takes the composer's slot. `live` =
/// false paints the specimen (no writes).
pub(crate) fn composer_replies_row(
    replies: &domain::session_tree::ComposerReplies,
    live: bool,
    cx: &App,
) -> AnyElement {
    use gpui::InteractiveElement as _;
    let spec = &domain::session_tree::THREAD;
    let test_id = spec.test_ids.replies.as_str();
    let message_id = replies.message_id.as_str();
    let dismiss_key = format!("{message_id}:*");
    let dismissing = reply_pending(&dismiss_key, cx);
    let pills = replies.items.iter().map(|item| {
        let key = format!("{message_id}:{}", item.id);
        let busy = dismissing || reply_pending(&key, cx);
        let (message, item_id) = (message_id.to_string(), item.id.clone());
        crate::surface::glass_pill_button(
            SharedString::from(format!("{test_id}-{message_id}-{}", item.id)),
            crate::surface::PillSize::Sm,
            cx,
        )
        .label(SharedString::from(item.text.clone()))
        .disabled(busy)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if !live {
                return;
            }
            let (message, item_id) = (message.clone(), item_id.clone());
            reply_write(format!("{message}:{item_id}"), "send", "the reply", cx, move |trpc| {
                api::sessions::send_reply(trpc, &message, &item_id)
            });
        })
        .into_any_element()
    });
    let message = message_id.to_string();
    let dismiss = Button::new(SharedString::from(format!("{test_id}-dismiss-{message_id}")))
        .ghost()
        .web_icon_xs()
        .icon(Icon::new(crate::icons::registry::UI_CLOSE))
        .tooltip(spec.copy.replies.dismiss.clone())
        .disabled(dismissing)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if !live {
                return;
            }
            let message = message.clone();
            reply_write(format!("{message}:*"), "dismiss", "the replies", cx, move |trpc| {
                api::sessions::dismiss_replies(trpc, &message)
            });
        });
    h_flex()
        .id(SharedString::from(format!("{test_id}-{message_id}")))
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap_1p5()
        .children(pills)
        .child(dismiss)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{fallback_session_model, glass_composer, GlassComposer};
    use domain::contract as c;
    use domain::rows::SessionRow;
    use gpui::{div, point, px, size, AppContext as _, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _};

    /// NEW-50: the card draws its FIELD above the tool row, with and without
    /// the N-14 row host (the row host had dropped the field: the IDE's
    /// session composer showed its tools with no text field).
    #[gpui::test]
    fn the_card_draws_its_field_above_the_tool_row(cx: &mut gpui::TestAppContext) {
        struct Probe {
            hosted: bool,
        }
        impl gpui::Render for Probe {
            fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
                let mut composer = GlassComposer::new(
                    div().debug_selector(|| "probe-field".into()).h(px(40.)).into_any_element(),
                )
                .tool(div().debug_selector(|| "probe-tool".into()).size(px(24.)).into_any_element());
                if self.hosted {
                    composer = composer.row_host(ElementId::Name("probe-row".into()));
                }
                div().size_full().child(glass_composer(composer))
            }
        }
        let cx = cx.add_empty_window();
        cx.update(|_, cx| gpui_component::init(cx));
        for hosted in [false, true] {
            let probe = cx.update(|_, cx| cx.new(|_| Probe { hosted }));
            for _ in 0..2 {
                cx.draw(point(px(0.), px(0.)), size(px(600.), px(300.)), |_, _| div().size_full().child(probe.clone()));
            }
            let field = cx
                .debug_bounds("probe-field")
                .unwrap_or_else(|| panic!("the field is not drawn (row host: {hosted})"));
            let tool = cx.debug_bounds("probe-tool").expect("the tool row is drawn");
            assert!(f32::from(field.size.height) >= 40., "the field keeps its height (row host: {hosted})");
            assert!(field.origin.y < tool.origin.y, "the field sits above the tool row (row host: {hosted})");
        }
    }

    /// Web `fallbackModel`: the row's pick, else (a dispatcher) the device's
    /// thread model, else the device's model, else the contract defaults.
    #[test]
    fn the_pin_falls_back_like_web() {
        let agents = serde_json::json!({
            "claude": { "model": "opus", "threadModel": "sonnet" },
            "codex": { "model": "", "threadModel": "" },
        });
        let row = |cli: Option<&str>, model: Option<&str>| SessionRow {
            id: "s".into(),
            cli: cli.map(Into::into),
            model: model.map(Into::into),
            ..Default::default()
        };
        // The row's own pick wins.
        assert_eq!(fallback_session_model(Some(&row(None, Some("fable"))), Some(&agents)), "fable");
        // NEW-8: never the legacy dispatcher's thread model; the device's
        // model, else the contract's.
        assert_eq!(fallback_session_model(Some(&row(None, None)), Some(&agents)), "opus");
        assert_eq!(
            fallback_session_model(Some(&row(Some("codex"), None)), Some(&agents)),
            c::DEVICE_AGENT_DEFAULTS_CODEX_MODEL
        );
        assert_eq!(fallback_session_model(None, None), c::DEVICE_AGENT_DEFAULTS_MODEL);
    }
}
