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
//! UNDER the card (`steer_viewer::render_composer_footer`); its attach "+"
//! is back in the card's tool row (EXP-1294).
//!
//! Every slot is optional except the field; the caller owns all state and
//! handlers, this only lays the card out.
//!
//! EXP-1294: [`session_composer_box`] is THE session composer box — Home's
//! launcher card (a 2–8 line field, the "+" bottom-left, the round send
//! bottom-right) on the work column, so the session view, the thread reply
//! and the agent channel paint the composer box from Home. [`SessionReply`]
//! gives the thread reply and the agent channel the session view's attach,
//! paste and upload-then-send.

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

/// EXP-1294 — the parts of a reply host (the thread reply, the agent
/// channel) the shared attach / paste / send helpers drive.
pub(crate) struct ReplyParts<'a> {
    pub(crate) input: &'a Entity<gpui_component::input::TextareaState>,
    pub(crate) images: &'a mut crate::composer_images::PendingImages,
    /// The strip's notice (too many / too big), shown under the card.
    pub(crate) notice: &'a mut Option<SharedString>,
    pub(crate) sending: &'a mut bool,
}

/// EXP-1294 — a view hosting THE session composer box that sends to one
/// session: images stage like the session view's and upload to the
/// session route before `sessions.send({sessionId, text, attachmentIds})`.
pub(crate) trait SessionReply: Sized + 'static {
    fn reply(&mut self) -> ReplyParts<'_>;
}

/// The "+" of a reply box: Add file or image (the session view's attach).
pub(crate) fn reply_attach_tool<V: SessionReply>(
    id: impl Into<ElementId>,
    sending: bool,
    cx: &mut gpui::Context<V>,
) -> AnyElement {
    composer_tool(id, crate::icons::registry::UI_ADD, cx)
        .tooltip("Add file or image")
        .disabled(sending)
        .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
            crate::composer_images::pick_attachment_files(window, cx, stage_reply_files::<V>);
        }))
        .into_any_element()
}

/// The pending strip of a reply box (`None` while nothing is staged).
pub(crate) fn reply_strip<V: SessionReply + gpui::Render>(
    view: &mut V,
    prefix: &'static str,
    cx: &mut gpui::Context<V>,
) -> Option<AnyElement> {
    let parts = view.reply();
    let sending = *parts.sending;
    (!parts.images.is_empty())
        .then(|| parts.images.render_strip(prefix, sending, remove_reply_file::<V>, cx))
}

/// The strip's notice as the box's footer line.
pub(crate) fn reply_notice<V: SessionReply>(view: &mut V, cx: &App) -> Option<AnyElement> {
    let notice = view.reply().notice.clone()?;
    Some(
        div()
            .pt_1()
            .px_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(notice)
            .into_any_element(),
    )
}

/// Whether a reply box has something to send.
pub(crate) fn reply_has_content<V: SessionReply>(view: &mut V, cx: &App) -> bool {
    let parts = view.reply();
    !parts.input.read(cx).value().trim().is_empty() || !parts.images.is_empty()
}

/// The paste capture of a reply box's card: clipboard images stage, text
/// falls through to the field.
pub(crate) fn on_reply_paste<V: SessionReply>(
    view: &mut V,
    _: &gpui_component::input::Paste,
    window: &mut Window,
    cx: &mut gpui::Context<V>,
) {
    let files = crate::composer_images::clipboard_images(cx);
    if files.is_empty() {
        return;
    }
    cx.stop_propagation();
    stage_reply_files(view, files, window, cx);
}

fn stage_reply_files<V: SessionReply>(
    view: &mut V,
    files: Vec<crate::composer_images::StagedFile>,
    window: &mut Window,
    cx: &mut gpui::Context<V>,
) {
    let parts = view.reply();
    *parts.notice = parts.images.stage(files, parts.input, window, cx);
    cx.notify();
}

fn remove_reply_file<V: SessionReply>(
    view: &mut V,
    key: u64,
    window: &mut Window,
    cx: &mut gpui::Context<V>,
) {
    let parts = view.reply();
    parts.images.remove(key, parts.input, window, cx);
    *parts.notice = None;
    cx.notify();
}

/// Put the draft back when a send failed, unless something new was typed.
fn restore_draft(
    input: &Entity<gpui_component::input::TextareaState>,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) {
    if input.read(cx).value().trim().is_empty() {
        let text = text.to_string();
        input.update(cx, |state, cx| state.set_value(text, window, cx));
    }
}

/// EXP-1294 — send a reply box to `session_id`: the draft clears at once,
/// the staged images upload SEQUENTIALLY and idempotently to the session
/// route (a retry uploads only the rest), then `sessions.send`. A failure
/// restores the draft and toasts "Couldn't …"; the images stay staged.
pub(crate) fn send_session_reply<V: SessionReply>(
    view: &mut V,
    session_id: &str,
    window: &mut Window,
    cx: &mut gpui::Context<V>,
) {
    let parts = view.reply();
    let text = parts.input.read(cx).value().to_string();
    if *parts.sending || (text.trim().is_empty() && parts.images.is_empty()) {
        return;
    }
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("send", "the message", None, cx);
        return;
    };
    let jobs = parts.images.jobs();
    let transport = if jobs.is_empty() {
        None
    } else {
        let Some(transport) = crate::queries::attachment_transport(cx) else {
            crate::action_error::failed_action("upload", "the attachment", None, cx);
            return;
        };
        Some(transport)
    };
    *parts.sending = true;
    *parts.notice = None;
    let input = parts.input.clone();
    input.update(cx, |state, cx| state.set_value("", window, cx));
    cx.notify();
    let session_id = session_id.to_string();
    cx.spawn_in(window, async move |this, cx| {
        let uploaded = match transport {
            None => Ok(Vec::new()),
            Some(transport) => {
                let session_id = session_id.clone();
                cx.background_executor()
                    .spawn(async move {
                        crate::composer_images::upload_all(jobs, |filename, content_type, bytes| {
                            transport.upload_session(&session_id, filename, content_type, bytes)
                        })
                    })
                    .await
            }
        };
        let resolved = match uploaded {
            Ok(resolved) => resolved,
            Err((resolved, error)) => {
                log::warn!("[ui] reply upload failed: {error}");
                let _ = this.update_in(cx, |this, window, cx| {
                    let parts = this.reply();
                    *parts.sending = false;
                    parts.images.note_uploaded(&resolved);
                    let input = parts.input.clone();
                    restore_draft(&input, &text, window, cx);
                    crate::action_error::failed_action("upload", "the attachment", None, cx);
                    cx.notify();
                });
                return;
            }
        };
        let Ok((message, ids)) = this.update(cx, |this, _| {
            let parts = this.reply();
            parts.images.note_uploaded(&resolved);
            let message = if resolved.is_empty() {
                text.trim().to_string()
            } else {
                parts.images.message(&text)
            };
            (message, resolved.iter().map(|(_, id, _)| id.clone()).collect::<Vec<_>>())
        }) else {
            return;
        };
        let input = api::sessions::SendInput::message(&session_id, &message, ids);
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::send(&trpc, &input) })
            .await;
        let _ = this.update_in(cx, |this, window, cx| {
            let parts = this.reply();
            *parts.sending = false;
            match result {
                Ok(_) => parts.images.clear(),
                Err(err) => {
                    let input = parts.input.clone();
                    restore_draft(&input, &text, window, cx);
                    crate::action_error::failed_action_err("send", "the message", &err, cx);
                }
            }
            cx.notify();
        });
    })
    .detach();
}
