//! #EDE-65 lane O: THE session composer IS the shared chat document's
//! footer (`apps/ede/ui/chat.json` `chat_footer`, `uicore_model::chat`), the
//! one eDE draws: the `/` and `@`/`#`/`:` menus (`/chatMenu`), the strip of
//! the Assistant's team chips and the staged files (`/chatFiles`), the
//! quick replies, and genUI's Composer with the "+" options Menu in its
//! `leading` slot and the context ring (an Embed this host claims) in its
//! `trailing` one. It is mounted on its own (the document narrowed to the
//! footer) as THE Dock band's composer, so the Dock's cards still sit above
//! it.
//!
//! The view's `TextareaState` stays THE draft every existing path reads and
//! writes (send, Stop's restore, a `/` insertion, a prompt's body, the
//! image markers): the document's `change` writes it, and a draft the host
//! wrote goes back into the document's field (`/chat/draft`).

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::host::MediaOptions;
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin, UploadEvent};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{AnyElement, AnyWindowHandle, App, AppContext as _, Entity, Hsla, SharedString, WeakEntity, Window};
use serde_json::{json, Value};
use uicore_model::chat::{ChatPlusItem, ChatPlusRow, ChatRepliesInput, ChatReplyInput};

use super::SteerSessionView;
use crate::slash_commands::{self, SlashItem};

/// The document's Composer.
pub(crate) const COMPOSER: &str = "chat_composer";
/// The "+" Menu in its `leading` slot.
pub(crate) const PLUS: &str = "chat_plus";
/// The context ring's Embed in its `trailing` slot.
pub(crate) const RING: &str = "chat_ring";
/// The ring's box (THE ring button, `web_icon_xs`).
const RING_SIZE: f32 = 24.;
/// A chip's picture is the image itself up to this size (a `data:` url).
const CHIP_PICTURE_MAX: usize = 2 * 1024 * 1024;
/// The "Issue ›" submenu's rows (the search list's open issues, newest first).
const ISSUE_ROWS: usize = 50;

// ---------------------------------------------------------------------------
// The mapping (pure)
// ---------------------------------------------------------------------------

/// THE `/` menu's rows ([`slash_commands::slash_items`]) as `/chatMenu`
/// binds them (`domain::chat_document::slash_menu`'s shape), row
/// `selected` highlighted.
pub(crate) fn slash_rows(items: &[SlashItem], selected: usize) -> Value {
    let rows: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut row = match item {
                SlashItem::Command(command) => json!({
                    "key": format!("command:{}", command.name),
                    "command": format!("/{}", command.name),
                    "description": command.description,
                    "hint": command.arg_hint,
                    "icon": null,
                    "insert": slash_commands::insertion(command),
                    "prompt": "",
                }),
                SlashItem::Prompt(prompt) => json!({
                    "key": format!("prompt:{}", prompt.id),
                    "command": "",
                    "description": prompt.name,
                    "hint": "",
                    "icon": prompt.icon.clone().filter(|icon| !icon.is_empty()).unwrap_or_else(|| domain::chat_document::PROMPT_ICON.into()),
                    "insert": "",
                    "prompt": prompt.id,
                }),
            };
            row["selected"] = json!(index == selected);
            row
        })
        .collect();
    json!({"open": !rows.is_empty(), "rows": rows})
}

/// What the document's composer asked for (`ede_chat` {action, …}: a
/// function call's args or an event's context).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComposerEvent {
    Draft(String),
    Send(String),
    Stop,
    /// A menu row's press: its insertion, or the prompt it loads.
    Pick { insert: String, prompt: String },
    /// A menu key the composer handed over (`menuKeys`).
    Key(String),
    /// `pasted` = a pasted image's (already staged through the upload hook).
    Attach { pasted: bool },
    Unattach(String),
    Plus(PlusAction),
    Reply { message_id: String, reply_id: String },
    DismissReplies(String),
}

pub(crate) fn composer_event(args: &Value) -> Option<ComposerEvent> {
    let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    Some(match args.get("action").and_then(Value::as_str)? {
        "draft" => ComposerEvent::Draft(arg("text")),
        "send" => ComposerEvent::Send(arg("text")),
        "stop" => ComposerEvent::Stop,
        "slash" => ComposerEvent::Pick { insert: arg("insert"), prompt: arg("prompt") },
        "key" => ComposerEvent::Key(arg("key")),
        "attach" => ComposerEvent::Attach { pasted: args.get("files").is_some_and(|files| !files.is_null()) },
        "unattach" => ComposerEvent::Unattach(arg("key")),
        "plus" => ComposerEvent::Plus(plus_action(&arg("value"))?),
        "reply" => ComposerEvent::Reply { message_id: arg("messageId"), reply_id: arg("replyId") },
        "dismissReplies" => ComposerEvent::DismissReplies(arg("messageId")),
        _ => return None,
    })
}

/// A "+" pick (composer-menu.json's rows, `uicore_model::chat::plus_pick`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlusAction {
    AddFile,
    /// `#IDENT` at the caret.
    Issue(String),
    /// The prompt's body at the caret.
    Prompt(String),
    /// A team toggled in the Assistant's pick.
    Team(String),
    /// An app editor's device.
    Device(String),
    /// A login (a session's switch, or an editor's next send).
    Account(String),
    Model(String),
}

pub(crate) fn plus_action(value: &str) -> Option<PlusAction> {
    use domain::chat_document as d;
    Some(match uicore_model::chat::plus_pick(value) {
        (d::PLUS_ADD_FILE, None) => PlusAction::AddFile,
        (d::PLUS_ISSUE, Some(id)) => PlusAction::Issue(id.to_string()),
        (d::PLUS_PROMPTS, Some(id)) => PlusAction::Prompt(id.to_string()),
        (d::PLUS_TEAMS, Some(id)) => PlusAction::Team(id.to_string()),
        (d::PLUS_DEVICE, Some(id)) => PlusAction::Device(id.to_string()),
        (d::PLUS_ACCOUNT, Some(id)) => PlusAction::Account(id.to_string()),
        (d::PLUS_MODEL, Some(id)) => PlusAction::Model(id.to_string()),
        _ => return None,
    })
}

/// One staged file as the strip reads it.
pub(crate) struct StripFile<'a> {
    pub key: u64,
    pub filename: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
    pub is_file: bool,
}

/// The strip's key of a team chip (its removal = the team leaves the pick).
const TEAM_KEY: &str = "team:";

/// `/chatFiles`: the Assistant's picked teams (a subset only) lead the
/// strip, then the staged files: an image's own picture (`data:`), else
/// its glyph; every chip removable.
pub(crate) fn strip(teams: &[&crate::teams_pick::PickTeam], files: &[StripFile]) -> Value {
    use base64::Engine as _;
    let mut items: Vec<Value> = teams
        .iter()
        .map(|team| json!({"key": format!("{TEAM_KEY}{}", team.id), "label": team.name, "image": team.icon_url.clone().unwrap_or_default(), "icon": "ui-team"}))
        .collect();
    items.extend(files.iter().map(|file| {
        let picture = if !file.is_file && file.bytes.len() <= CHIP_PICTURE_MAX {
            format!("data:{};base64,{}", file.content_type, base64::engine::general_purpose::STANDARD.encode(file.bytes))
        } else {
            String::new()
        };
        json!({"key": file.key.to_string(), "label": file.filename, "image": picture, "icon": if file.is_file { "file" } else { "image" }})
    }));
    json!({"open": !items.is_empty(), "items": items})
}

/// A chip's key: a team's, or a staged file's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StripKey {
    Team(String),
    File(u64),
}

pub(crate) fn strip_key(key: &str) -> Option<StripKey> {
    match key.strip_prefix(TEAM_KEY) {
        Some(team) => Some(StripKey::Team(team.to_string())),
        None => key.parse().ok().map(StripKey::File),
    }
}

/// The chat document narrowed to its footer, in THE Dock band's box: no
/// column, gutter or padding of its own (the band has them).
pub(crate) fn document() -> uicore_gpui::inline::Document {
    let mut doc = uicore_gpui::inline::document(uicore_docs::doc("chat").expect("the factory chat"));
    uicore_gpui::inline::only(&mut doc, "chat_footer");
    for (key, value) in [("maxWidth", Value::Null), ("alignSelf", Value::Null), ("paddingLeft", json!(0)), ("paddingRight", json!(0)), ("paddingBottom", json!(0))] {
        uicore_gpui::inline::set_style(&mut doc, "chat_footer", key, value);
    }
    doc
}

// ---------------------------------------------------------------------------
// The mounted footer
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FooterState {
    steer: Option<WeakEntity<SteerSessionView>>,
    window: Option<AnyWindowHandle>,
    /// Staged files: the send stays enabled with an empty field.
    submits_empty: bool,
}

/// What the footer showed last (each path set only when it changed).
#[derive(Default)]
pub(crate) struct ComposerDoc {
    view: Option<Entity<SurfaceView>>,
    state: Rc<RefCell<FooterState>>,
    /// The literal props (placeholder, busy, the "+" items) the document
    /// was built with.
    props: Value,
    chatting: Value,
    menu: Value,
    files: Value,
    /// The draft the document's field holds (as it last reported it, or as
    /// the host last wrote it).
    pub(crate) draft: String,
    /// A send fired: the field's own emptying follows; the host's draft
    /// (refused, or held for a confirm) goes back into it.
    pub(crate) submitted: bool,
    /// The `@`/`#`/`:` menu: the draft it was built for, its rows, its
    /// highlight, and the draft an Escape (or a pick) shut it on.
    pub(crate) mention: Option<(String, Value)>,
    pub(crate) mention_selected: usize,
    pub(crate) mention_dismissed: Option<String>,
}

impl ComposerDoc {
    /// The footer's view, once mounted.
    #[cfg(test)]
    pub(crate) fn view(&self) -> Option<&Entity<SurfaceView>> {
        self.view.as_ref()
    }

    /// Opens the "+" Menu (a press of its trigger) and, given, its item
    /// `item` (a submenu opens beside it): the dev capture's
    /// (`EXP_DEV_COMPOSER_MENU=session-account`) and the tests'.
    pub(crate) fn open_plus(&self, item: Option<usize>, window: &mut Window, cx: &mut App) -> bool {
        let Some(view) = self.view.clone() else { return false };
        view.update(cx, |view, cx| {
            let Some(trigger) = view.index_of(&format!("{PLUS}_button")) else { return false };
            view.press(trigger, window, cx);
            if let Some(item) = item.and_then(|item| view.index_of(&format!("{PLUS}.item.{item}"))) {
                view.press(item, window, cx);
            }
            true
        })
    }

    /// The Composer field's focus handle, once mounted.
    pub(crate) fn field_focus(&self, cx: &App) -> Option<gpui::FocusHandle> {
        self.view.as_ref()?.read(cx).field_focus_handle(COMPOSER, cx)
    }

    fn mount(&mut self, steer: WeakEntity<SteerSessionView>, team_id: Option<String>, window: &mut Window, cx: &mut App) {
        if self.view.is_some() {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.steer = Some(steer);
            state.window = Some(window.window_handle());
        }
        let md = Rc::new(crate::markdown::AppMarkdownHost::for_document(team_id, cx));
        let host: Rc<dyn HostPlugin> = Rc::new(FooterHost { state: self.state.clone(), md });
        let doc = document();
        let theme = uicore_gpui::inline::theme(&eui_core::themes::default_theme());
        let view = cx.new(|cx| {
            let settings = eui_core::surface::SurfaceSettings { locale: "en-US".into(), mode: eui_core::theme::ModeSetting::Dark, ..Default::default() };
            // The "+" Menu opens above the composer, out of the footer's box.
            let options = SurfaceViewOptions { surface_id: doc.name.clone(), theme: Some(theme), mode: eui_core::theme::Mode::Dark, host, settings: Some(settings), lift_layers: true, ..Default::default() };
            let mut view = SurfaceView::new(options, window, cx);
            view.record_bounds(true);
            uicore_gpui::load(&mut view, &doc, &uicore_data::Feed::new(), cx);
            view
        });
        self.view = Some(view);
    }

    fn set(view: &Entity<SurfaceView>, path: &str, value: Value, cx: &mut App) {
        view.update(cx, |view, cx| {
            let _ = view.set_data(path, Some(value), cx);
        });
    }
}

/// Everything one frame of the footer shows.
pub(crate) struct ComposerFrame {
    pub input: uicore_model::chat::ChatInput,
    /// The field's placeholder (the host's: an agent channel's own).
    pub placeholder: String,
    pub menu: Value,
    pub files: Value,
    pub draft: String,
    pub submits_empty: bool,
}

impl ComposerDoc {
    /// Mounts (once) and brings the footer up to `frame`; its element.
    pub(crate) fn element(&mut self, steer: WeakEntity<SteerSessionView>, team_id: Option<String>, frame: ComposerFrame, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.mount(steer, team_id, window, cx);
        let view = self.view.clone()?;
        self.state.borrow_mut().submits_empty = frame.submits_empty;
        let mut composer = uicore_model::chat::chat_view(&frame.input).composer;
        composer.placeholder = frame.placeholder.clone();
        let props = json!({"placeholder": composer.placeholder, "busy": composer.busy, "items": composer.menu});
        if props != self.props {
            self.props = props;
            let mut doc = document();
            uicore_gpui::inline::set_prop(&mut doc, COMPOSER, "placeholder", json!(composer.placeholder));
            uicore_gpui::inline::set_prop(&mut doc, COMPOSER, "busy", json!(composer.busy));
            uicore_gpui::inline::set_prop(&mut doc, PLUS, "items", Value::Array(composer.menu.clone()));
            uicore_gpui::inline::reload(&view, &doc, cx);
        }
        let chatting = json!({"composer": composer});
        if chatting != self.chatting {
            self.chatting = chatting.clone();
            ComposerDoc::set(&view, "/chatting", chatting, cx);
        }
        if frame.menu != self.menu {
            self.menu = frame.menu.clone();
            ComposerDoc::set(&view, "/chatMenu", frame.menu, cx);
        }
        if frame.files != self.files {
            self.files = frame.files.clone();
            ComposerDoc::set(&view, "/chatFiles", frame.files, cx);
        }
        // A draft the host wrote (a `/` insertion, a prompt, Stop's
        // restore, an image marker) goes into the field.
        if frame.draft != self.draft {
            self.draft = frame.draft.clone();
            ComposerDoc::set(&view, "/chat/draft", json!(frame.draft), cx);
        }
        let ring = frame.input.ring;
        if ring {
            view.update(cx, |view, cx| view.host_leaf_resized(RING, cx));
        }
        Some(gpui::div().w_full().min_w_0().child(view).into_any_element())
    }
}

use gpui::{IntoElement as _, ParentElement as _, Styled as _};

// ---------------------------------------------------------------------------
// The host
// ---------------------------------------------------------------------------

/// The footer's host: the IDE's icons and pictures, its ring, and every
/// composer action on the session view's own handlers.
struct FooterHost {
    state: Rc<RefCell<FooterState>>,
    md: Rc<crate::markdown::AppMarkdownHost>,
}

impl FooterHost {
    /// `event` on the session view, after the document's update (it is
    /// mid-dispatch), in the view's window.
    fn dispatch(&self, event: ComposerEvent, cx: &mut App) {
        let (steer, window) = {
            let state = self.state.borrow();
            (state.steer.clone(), state.window)
        };
        let (Some(steer), Some(window)) = (steer, window) else { return };
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = steer.update(cx, |this, cx| this.on_composer_event(event, window, cx));
            });
        });
    }
}

impl HostPlugin for FooterHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.md.icon(name)
    }

    fn media_options(&self) -> MediaOptions {
        let mut options = self.md.media_options();
        let schemes = options.schemes.get_or_insert_with(|| vec!["https".into(), "http".into()]);
        if !schemes.iter().any(|scheme| scheme == "data") {
            schemes.push("data".into());
        }
        options
    }

    fn media_request(&self, src: &str) -> Option<eui_core::host::MediaRequest> {
        self.md.media_request(src)
    }

    fn chip_icon(&self, name: &str, size: f32, color: Hsla) -> Option<AnyElement> {
        self.md.chip_icon(name, size, color)
    }

    fn host_leaf_height(&self, component_id: &str, _width: f32, _window: &mut Window, _cx: &mut App) -> Option<f32> {
        (component_id == RING).then_some(RING_SIZE)
    }

    fn host_leaf(&self, component_id: &str, _window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        if component_id != RING {
            return None;
        }
        let steer = self.state.borrow().steer.clone()?.upgrade()?;
        steer.update(cx, |this, cx| this.render_context_ring(cx))
    }

    fn composer_submits_empty(&self, component_id: &str, _cx: &App) -> bool {
        component_id == COMPOSER && self.state.borrow().submits_empty
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        if event.name != "ede_chat" {
            return;
        }
        if let Some(event) = composer_event(&event.args) {
            self.dispatch(event, cx);
        }
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        if event.name != "ede_chat" {
            return;
        }
        if let Some(event) = composer_event(&event.context) {
            self.dispatch(event, cx);
        }
    }

    /// A pasted image stages like a picked file (the composer's `attach`
    /// {files} follows and does nothing more).
    fn on_upload(&self, event: &UploadEvent, cx: &mut App) {
        if event.component_id != COMPOSER {
            return;
        }
        let (steer, window) = {
            let state = self.state.borrow();
            (state.steer.clone(), state.window)
        };
        let (Some(steer), Some(window)) = (steer, window) else { return };
        let read: Vec<crate::composer_images::StagedFile> = event.paths.iter().filter_map(|path| crate::composer_images::read_pick(path)).collect();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = steer.update(cx, |this, cx| this.stage_images(read, window, cx));
            });
        });
    }
}

// ---------------------------------------------------------------------------
// The session view's side
// ---------------------------------------------------------------------------

impl SteerSessionView {
    /// THE composer: the chat document's footer.
    pub(super) fn render_doc_composer(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let frame = self.composer_frame(cx);
        let team_id = self.row.as_ref().and_then(|row| row.team_id.clone());
        let steer = cx.entity().downgrade();
        // #EXP-1535 (DEV-ONLY, `EXP_DEV_COMPOSER_MENU=session-account`):
        // the menu opens once with Account › open.
        let account_step = frame.input.plus.iter().position(|row| row.id == domain::chat_document::PLUS_ACCOUNT);
        let mut doc = std::mem::take(&mut self.composer_doc);
        let element = doc.element(steer, team_id, frame, window, cx);
        self.composer_doc = doc;
        if account_step.is_some() && crate::navigation::shapes_ready(cx) && crate::screens::take_dev_session_account_menu() {
            let view = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = view.update(cx, |this, cx| this.composer_doc.open_plus(account_step, window, cx));
            });
        }
        element.unwrap_or_else(|| gpui::div().into_any_element())
    }

    fn composer_frame(&mut self, cx: &mut gpui::Context<Self>) -> ComposerFrame {
        let draft = self.input.read(cx).value().to_string();
        let menu = self.composer_menu(&draft, cx);
        let teams = if self.to_assistant(cx) { crate::teams_pick::pick_teams(cx) } else { Vec::new() };
        let chips = crate::teams_pick::chip_teams(self.teams_picked.as_deref(), &teams);
        let files: Vec<StripFile> = self
            .pending_images
            .items()
            .iter()
            .map(|item| StripFile { key: item.key, filename: &item.filename, content_type: &item.content_type, bytes: &item.preview.bytes, is_file: item.is_file })
            .collect();
        let files = strip(&chips, &files);
        let ring = !self.source.read_only() && !self.session_over() && crate::usage_bar::context_percent(self.usage().as_ref()).is_some();
        let input = uicore_model::chat::ChatInput {
            session_id: Some(self.session_id.to_string()),
            slash_commands: true,
            busy: self.shows_stop(cx),
            // #EDE-65 lane T: the "+" is inert while a send is in flight.
            sending: self.sending,
            plus: self.plus_rows(cx),
            replies: self.replies.as_ref().map(|replies| ChatRepliesInput {
                message_id: replies.message_id.clone(),
                items: replies.items.iter().map(|item| ChatReplyInput { id: item.id.clone(), text: item.text.clone() }).collect(),
            }),
            ring,
            ..Default::default()
        };
        ComposerFrame { input, placeholder: self.composer_placeholder.to_string(), menu, files, draft, submits_empty: !self.pending_images.is_empty() && !self.sending && self.may_message }
    }

    /// `/chatMenu`: the `@`/`#`/`:` typeahead over the draft's last token,
    /// else THE `/` menu.
    fn composer_menu(&mut self, draft: &str, cx: &App) -> Value {
        if self.composer_doc.mention_dismissed.as_deref() != Some(draft) {
            self.composer_doc.mention_dismissed = None;
            let fresh = self.composer_doc.mention.as_ref().is_some_and(|(at, _)| at == draft);
            if !fresh {
                let menu = self.mention_menu(draft, cx).unwrap_or(Value::Null);
                self.composer_doc.mention = Some((draft.to_string(), menu));
                self.composer_doc.mention_selected = 0;
            }
            if let Some((_, menu)) = self.composer_doc.mention.as_ref().filter(|(_, menu)| menu["open"] == true) {
                let mut menu = menu.clone();
                self.composer_doc.mention_selected = domain::chat_document::highlight(&mut menu, self.composer_doc.mention_selected, true);
                return menu;
            }
        }
        match self.slash.as_ref() {
            Some(slash) => slash_rows(&slash.items, slash.selected),
            None => json!({"open": false, "rows": []}),
        }
    }

    /// THE typeahead's rows (`domain::chat_document::mention_menu`): the
    /// team's members for `@`, its open issues for `#`, the emoji for `:`.
    fn mention_menu(&self, draft: &str, cx: &App) -> Option<Value> {
        let token = domain::typeahead::detect_trigger(draft, draft.len())?;
        let team_id = self.row.as_ref().and_then(|row| row.team_id.clone()).filter(|_| sync::Store::try_global(cx).is_some());
        let mut members = Vec::new();
        let mut issues = Vec::new();
        if let Some(team_id) = team_id.as_deref() {
            match token.trigger {
                domain::typeahead::CompletionTrigger::Mention => {
                    let collections = sync::Store::global(cx).collections();
                    let users = collections.users.read(cx);
                    members = collections
                        .team_members
                        .read(cx)
                        .iter()
                        .filter(|member| member.team_id == team_id)
                        .filter_map(|member| users.get(&member.user_id))
                        .filter_map(|user| {
                            let email = user.email.clone()?;
                            Some(domain::chat_document::Member { name: user.name.clone().unwrap_or_else(|| email.clone()), email })
                        })
                        .collect();
                }
                domain::typeahead::CompletionTrigger::IssueRef => {
                    let query = token.query.to_lowercase();
                    issues = crate::picker::issue_picker::snapshot_rows(cx, team_id, &Default::default())
                        .into_iter()
                        .filter(|row| query.is_empty() || row.identifier.to_lowercase().contains(&query) || row.title.to_lowercase().contains(&query))
                        .take(domain::typeahead::MAX_ITEMS)
                        .map(|row| (row.identifier, row.title))
                        .collect();
                }
                domain::typeahead::CompletionTrigger::Emoji => {}
            }
        }
        domain::chat_document::mention_menu(draft, &members, &issues)
    }

    /// The "+" rows this composer serves (composer-menu.json on its surface,
    /// `chat_screen::composer_menu_rows`), each with its submenu's entries.
    fn plus_rows(&self, cx: &App) -> Vec<ChatPlusRow> {
        use crate::chat_screen::{composer_menu_rows, ComposerMenuConditions, ComposerMenuRow as Row, ComposerSurface};
        use domain::chat_document as d;
        let surface = if self.row.as_ref().and_then(|row| row.agent_id.as_deref()).is_some_and(|id| !id.is_empty()) {
            ComposerSurface::Agent
        } else {
            ComposerSurface::Session
        };
        let team_id = self.row.as_ref().and_then(|row| row.team_id.clone()).filter(|_| sync::Store::try_global(cx).is_some());
        let prompts: Vec<slash_commands::SlashPrompt> = team_id
            .as_deref()
            .map(|team_id| crate::queries::team_prompts(cx, team_id).0.iter().map(slash_commands::SlashPrompt::from).collect())
            .unwrap_or_default();
        let account_switch = self.composer_account_switch(cx);
        let editor_runner = self.editor_runner.as_ref().filter(|_| !self.configurable());
        let to_assistant = self.to_assistant(cx);
        let rows = composer_menu_rows(
            surface,
            ComposerMenuConditions {
                device: self.editor_device.is_some(),
                account: account_switch.is_some() || editor_runner.is_some(),
                model: self.configurable() || editor_runner.is_some(),
                prompts: !prompts.is_empty(),
                assistant: to_assistant,
                teams: to_assistant && crate::teams_pick::several_teams(cx),
                ..ComposerMenuConditions::default()
            },
        );
        let submenu = |id: &str, items: Vec<ChatPlusItem>, searchable: bool| ChatPlusRow { items: Some(items), searchable, ..d::plus_row(id) };
        let mut out = Vec::new();
        for row in rows {
            out.push(match row {
                Row::Separator => ChatPlusRow { id: "separator".into(), ..Default::default() },
                Row::AddFile => d::plus_row(d::PLUS_ADD_FILE),
                Row::ImplementIssue => {
                    let Some(team_id) = team_id.as_deref() else { continue };
                    let items = crate::picker::issue_picker::snapshot_rows(cx, team_id, &Default::default())
                        .into_iter()
                        .take(ISSUE_ROWS)
                        .map(|row| ChatPlusItem { label: format!("{} {}", row.identifier, row.title), value: row.identifier, ..Default::default() })
                        .collect();
                    submenu(d::PLUS_ISSUE, items, true)
                }
                Row::Prompts => submenu(
                    d::PLUS_PROMPTS,
                    prompts
                        .iter()
                        .map(|prompt| ChatPlusItem {
                            label: prompt.name.clone(),
                            value: prompt.id.clone(),
                            icon: Some(prompt.icon.clone().filter(|icon| !icon.is_empty()).unwrap_or_else(|| d::PROMPT_ICON.into())),
                            ..Default::default()
                        })
                        .collect(),
                    true,
                ),
                Row::Teams => {
                    let teams = crate::teams_pick::pick_teams(cx);
                    let picked = self.teams_picked.as_deref();
                    let items = teams
                        .iter()
                        .map(|team| ChatPlusItem {
                            label: team.name.clone(),
                            value: team.id.clone(),
                            checked: Some(picked.is_none_or(|picked| picked.contains(&team.id))),
                            ..Default::default()
                        })
                        .collect();
                    submenu(d::PLUS_TEAMS, items, false)
                }
                Row::Device => {
                    let Some(device) = self.editor_device.as_ref() else { continue };
                    let items = device
                        .rows
                        .iter()
                        .map(|(id, label)| ChatPlusItem { label: label.clone(), value: id.clone(), checked: Some(device.device_id.as_deref() == Some(id.as_str())), ..Default::default() })
                        .collect();
                    submenu(d::PLUS_DEVICE, items, false)
                }
                Row::Account => match (&account_switch, editor_runner) {
                    (Some((_, targets)), _) => submenu(
                        d::PLUS_ACCOUNT,
                        targets
                            .iter()
                            .map(|target| ChatPlusItem {
                                label: target.caption.clone(),
                                value: target.profile_id.clone(),
                                checked: Some(target.current),
                                disabled: !target.pickable(),
                                ..Default::default()
                            })
                            .collect(),
                        false,
                    ),
                    (None, Some(runner)) if !runner.menu.accounts.is_empty() => submenu(
                        d::PLUS_ACCOUNT,
                        runner
                            .menu
                            .accounts
                            .iter()
                            .map(|option| ChatPlusItem {
                                label: option.email.clone(),
                                value: option.account_option_key(),
                                checked: Some(option.account_option_key() == runner.menu.account_picked),
                                ..Default::default()
                            })
                            .collect(),
                        false,
                    ),
                    _ => continue,
                },
                Row::Model => {
                    let (models, picked) = match editor_runner {
                        Some(runner) if !self.configurable() => (runner.menu.models, runner.menu.model_picked.clone()),
                        _ if self.configurable() => self.model_choices(cx),
                        _ => continue,
                    };
                    let items = models.iter().map(|(label, value)| ChatPlusItem { label: (*label).to_string(), value: (*value).to_string(), checked: Some(picked == *value), ..Default::default() }).collect();
                    submenu(d::PLUS_MODEL, items, false)
                }
                // Launch-only rows never reach a session's surface.
                _ => continue,
            });
        }
        out
    }

    /// THE model list of the session's agent and the run's model.
    fn model_choices(&self, cx: &App) -> (&'static [(&'static str, &'static str)], String) {
        let value = crate::session_extras::option_chip(self.feed.config(), super::CONFIG_MODEL_ID)
            .map(|chip| chip.value)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| crate::composer::session_fallback_model(self.row.as_ref(), cx));
        let choices: &'static [(&'static str, &'static str)] =
            if self.agent() == steer::activity::SessionAgent::Claude { &crate::coding_selects::MODEL_CHOICES } else { &crate::coding_selects::CODEX_MODEL_CHOICES };
        (choices, value)
    }

    /// The document's composer asked for `event`.
    pub(crate) fn on_composer_event(&mut self, event: ComposerEvent, window: &mut Window, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        DISPATCHED.with(|dispatched| dispatched.borrow_mut().push(event.clone()));
        match event {
            ComposerEvent::Draft(text) => {
                if std::mem::take(&mut self.composer_doc.submitted) && text.is_empty() {
                    // The field emptied itself after a send: the host's draft
                    // (a refused send's, a confirm's) goes back into it.
                    self.composer_doc.draft.clear();
                    cx.notify();
                    return;
                }
                self.composer_doc.draft = text.clone();
                // EXP-551: a closed exact `:shortcode:` becomes its emoji at once.
                let text = domain::chat_document::commit_emoji(&text).unwrap_or(text);
                self.set_draft(text, window, cx);
            }
            ComposerEvent::Send(text) => {
                if !text.is_empty() && self.input.read(cx).value().trim() != text.trim() {
                    self.set_draft(text, window, cx);
                }
                self.composer_doc.submitted = true;
                self.send(window, cx);
            }
            ComposerEvent::Stop => self.stop_turn(window, cx),
            ComposerEvent::Pick { insert, prompt } => self.composer_pick(&insert, &prompt, window, cx),
            ComposerEvent::Key(key) => self.composer_key(&key, window, cx),
            ComposerEvent::Attach { pasted: true } => {}
            ComposerEvent::Attach { pasted: false } | ComposerEvent::Plus(PlusAction::AddFile) => self.pick_files(window, cx),
            ComposerEvent::Unattach(key) => match strip_key(&key) {
                Some(StripKey::File(key)) => self.remove_pending(key, window, cx),
                Some(StripKey::Team(id)) => {
                    let teams = crate::teams_pick::pick_teams(cx);
                    let rest: Vec<String> = crate::teams_pick::chip_teams(self.teams_picked.as_deref(), &teams).iter().filter(|team| team.id != id).map(|team| team.id.clone()).collect();
                    let next = if rest.is_empty() { None } else { crate::teams_pick::next_teams_pick(&rest, &teams) };
                    (self.teams_change(cx))(next, window, cx);
                }
                None => {}
            },
            ComposerEvent::Plus(action) => self.composer_plus(action, window, cx),
            ComposerEvent::Reply { message_id, reply_id } => crate::composer::send_quick_reply(&message_id, &reply_id, cx),
            ComposerEvent::DismissReplies(message_id) => crate::composer::dismiss_quick_replies(&message_id, cx),
        }
    }

    /// The draft becomes `text` (the caret at its end: the document's field
    /// reports text, not a caret), and the menus follow it.
    fn set_draft(&mut self, text: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.input.read(cx).value() != text.as_str() {
            let position = crate::markdown::byte_offset_to_position(&text, text.len());
            self.input.update(cx, |state, cx| {
                state.set_value(text, window, cx);
                state.set_cursor_position(position, window, cx);
            });
        }
        self.refresh_slash(cx);
        cx.notify();
    }

    /// A menu row picked (its press, or Enter/Tab on the highlight): a
    /// prompt loads its body, a builtin or a typeahead row puts its
    /// insertion in the composer, which keeps the menus shut for it.
    fn composer_pick(&mut self, insert: &str, prompt: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !prompt.is_empty() {
            let picked = self.slash.as_ref().and_then(|menu| {
                menu.items.iter().find_map(|item| match item {
                    SlashItem::Prompt(p) if p.id == prompt => Some(p.clone()),
                    _ => None,
                })
            });
            if let Some(picked) = picked {
                self.slash = None;
                self.pick_prompt(picked, window, cx);
            }
            return;
        }
        // Set BEFORE the write: the new draft must not re-open a menu.
        self.slash_dismissed_for = Some(insert.to_string());
        self.composer_doc.mention_dismissed = Some(insert.to_string());
        self.slash = None;
        self.set_draft(insert.to_string(), window, cx);
    }

    /// The composer's menu keys (`chat_document::menu_key`, the IDE's rules).
    fn composer_key(&mut self, key: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let draft = self.input.read(cx).value().to_string();
        let menu = self.composer_menu(&draft, cx);
        let rows = menu["rows"].as_array().cloned().unwrap_or_default();
        let mention = self.composer_doc.mention.as_ref().is_some_and(|(_, m)| m["open"] == true) && self.composer_doc.mention_dismissed.is_none();
        let selected = if mention { self.composer_doc.mention_selected } else { self.slash.as_ref().map_or(0, |menu| menu.selected) };
        match domain::chat_document::menu_key(key, selected, rows.len()) {
            domain::chat_document::MenuKey::Highlight(index) => {
                if mention {
                    self.composer_doc.mention_selected = index;
                } else if let Some(menu) = self.slash.as_mut() {
                    menu.selected = index;
                }
                cx.notify();
            }
            domain::chat_document::MenuKey::Pick(index) => {
                let field = |key: &str| rows[index][key].as_str().unwrap_or("").to_string();
                self.composer_pick(&field("insert"), &field("prompt"), window, cx);
            }
            domain::chat_document::MenuKey::Dismiss => {
                if mention {
                    self.composer_doc.mention_dismissed = Some(draft);
                } else {
                    self.slash = None;
                    self.slash_dismissed_for = Some(draft);
                }
                cx.notify();
            }
            domain::chat_document::MenuKey::Ignore => {}
        }
    }

    /// A "+" pick on the handlers the IDE's own menu called.
    fn composer_plus(&mut self, action: PlusAction, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let insert = |this: &mut Self, text: String, window: &mut Window, cx: &mut gpui::Context<Self>| {
            this.input.update(cx, |state, cx| state.insert(text, window, cx));
            this.refresh_slash(cx);
            cx.notify();
        };
        match action {
            PlusAction::AddFile => self.pick_files(window, cx),
            PlusAction::Issue(identifier) => insert(self, crate::picker::search_list::typeahead_insert(&identifier), window, cx),
            PlusAction::Prompt(id) => {
                let team_id = self.row.as_ref().and_then(|row| row.team_id.clone());
                let prompt = team_id
                    .and_then(|team_id| crate::queries::team_prompts(cx, &team_id).0.iter().find(|p| p.id == id).map(slash_commands::SlashPrompt::from));
                let Some(prompt) = prompt else { return };
                let view = cx.entity().downgrade();
                let at_caret: Rc<dyn Fn(String, &mut Window, &mut App)> = Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    let _ = view.update(cx, |this, cx| {
                        this.input.update(cx, |state, cx| state.insert(text, window, cx));
                        this.refresh_slash(cx);
                        cx.notify();
                    });
                });
                crate::picker::search_list::insert_prompt_body(prompt, at_caret, window, cx);
            }
            PlusAction::Team(id) => {
                let teams = crate::teams_pick::pick_teams(cx);
                if let Some(next) = crate::teams_pick::toggle_team(self.teams_picked.as_deref(), &teams, &id) {
                    (self.teams_change(cx))(next, window, cx);
                }
            }
            PlusAction::Device(id) => {
                if let Some(device) = self.editor_device.as_ref() {
                    let on_pick = device.on_pick.clone();
                    on_pick(id, window, cx);
                }
            }
            PlusAction::Account(profile_id) => {
                if let Some((context, targets)) = self.composer_account_switch(cx) {
                    if targets.iter().any(|target| target.profile_id == profile_id && target.pickable()) {
                        crate::account_switch::switch_to(context.session_id, context.local, profile_id, window, cx);
                    }
                } else if let Some(runner) = self.editor_runner.as_ref() {
                    if let Some(option) = runner.menu.accounts.iter().find(|option| option.account_option_key() == profile_id).cloned() {
                        let on_account = runner.on_account.clone();
                        on_account(option, window, cx);
                    }
                }
            }
            PlusAction::Model(value) => {
                if self.configurable() {
                    Self::configure(
                        cx.entity().downgrade(),
                        api::sessions::ConfigureInput { session_id: self.session_id.to_string(), model: Some(value), ..Default::default() },
                        cx,
                    );
                } else if let Some(runner) = self.editor_runner.as_ref() {
                    let on_model = runner.on_model.clone();
                    on_model(value, window, cx);
                }
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Every composer event the view handled, in order (tests).
    pub(crate) static DISPATCHED: RefCell<Vec<ComposerEvent>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
impl SteerSessionView {
    /// Test aid: the composer's document.
    pub(crate) fn composer_document(&self) -> Option<Entity<SurfaceView>> {
        self.composer_doc.view().cloned()
    }

    /// Test aid: THE draft.
    pub(crate) fn composer_draft(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// Test aid: the staged files' count.
    pub(crate) fn composer_files(&self) -> usize {
        self.pending_images.len()
    }

    /// Test aid: opens the "+" Menu (and its item `item`).
    pub(crate) fn open_composer_plus(&self, item: Option<usize>, window: &mut Window, cx: &mut App) -> bool {
        self.composer_doc.open_plus(item, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, hint: &str) -> SlashItem {
        SlashItem::Command(slash_commands::MenuCommand { name: name.into(), description: format!("{name} it"), arg_hint: hint.into(), confirm: false })
    }

    /// THE `/` menu's rows have the document's shape (the one eDE's
    /// `slash_menu` binds), the highlight on its row.
    #[test]
    fn the_slash_rows_are_the_documents_menu() {
        let items = vec![
            command("compact", ""),
            command("effort", "<level>"),
            SlashItem::Prompt(slash_commands::SlashPrompt { id: "p1".into(), name: "Triage".into(), icon: None }),
        ];
        let menu = slash_rows(&items, 1);
        assert_eq!(menu["open"], true);
        let rows = menu["rows"].as_array().unwrap();
        assert_eq!(rows[0], json!({"key": "command:compact", "command": "/compact", "description": "compact it", "hint": "", "icon": null, "insert": "/compact", "prompt": "", "selected": false}));
        assert_eq!(rows[1]["insert"], "/effort ", "an argument leaves room to type it");
        assert_eq!(rows[1]["selected"], true);
        assert_eq!(rows[2], json!({"key": "prompt:p1", "command": "", "description": "Triage", "hint": "", "icon": domain::chat_document::PROMPT_ICON, "insert": "", "prompt": "p1", "selected": false}));
        assert_eq!(slash_rows(&[], 0), json!({"open": false, "rows": []}));
        // The same keys as eDE's menu rows.
        let ede = domain::chat_document::slash_menu("/", Some("claude"), &[]);
        let keys = |row: &Value| row.as_object().unwrap().keys().filter(|k| *k != "selected").cloned().collect::<Vec<_>>();
        assert_eq!(keys(&ede["rows"][0]), keys(&rows[0]));
    }

    /// Every composer action the document fires lands on its handler.
    #[test]
    fn the_documents_actions_map_onto_the_handlers() {
        let event = |args: Value| composer_event(&args);
        assert_eq!(event(json!({"action": "draft", "text": "hi"})), Some(ComposerEvent::Draft("hi".into())));
        assert_eq!(event(json!({"action": "send", "text": "hi"})), Some(ComposerEvent::Send("hi".into())));
        assert_eq!(event(json!({"action": "stop"})), Some(ComposerEvent::Stop));
        assert_eq!(event(json!({"action": "slash", "insert": "/compact", "prompt": ""})), Some(ComposerEvent::Pick { insert: "/compact".into(), prompt: String::new() }));
        assert_eq!(event(json!({"action": "key", "key": "ArrowDown"})), Some(ComposerEvent::Key("ArrowDown".into())));
        assert_eq!(event(json!({"action": "attach"})), Some(ComposerEvent::Attach { pasted: false }));
        assert_eq!(event(json!({"action": "attach", "files": [{"name": "image.png"}]})), Some(ComposerEvent::Attach { pasted: true }));
        assert_eq!(event(json!({"action": "unattach", "key": "3"})), Some(ComposerEvent::Unattach("3".into())));
        assert_eq!(event(json!({"action": "plus", "value": "add-file"})), Some(ComposerEvent::Plus(PlusAction::AddFile)));
        assert_eq!(event(json!({"action": "reply", "messageId": "m1", "replyId": "yes"})), Some(ComposerEvent::Reply { message_id: "m1".into(), reply_id: "yes".into() }));
        assert_eq!(event(json!({"action": "dismissReplies", "messageId": "m1"})), Some(ComposerEvent::DismissReplies("m1".into())));
        assert_eq!(event(json!({"action": "nope"})), None);
        assert_eq!(event(json!({})), None);
    }

    /// A "+" pick names its row and entry (composer-menu.json's ids).
    #[test]
    fn a_plus_pick_names_its_row_and_entry() {
        assert_eq!(plus_action("add-file"), Some(PlusAction::AddFile));
        assert_eq!(plus_action("implement-issue:EXP-12"), Some(PlusAction::Issue("EXP-12".into())));
        assert_eq!(plus_action("prompts:p1"), Some(PlusAction::Prompt("p1".into())));
        assert_eq!(plus_action("teams:t1"), Some(PlusAction::Team("t1".into())));
        assert_eq!(plus_action("device:d1"), Some(PlusAction::Device("d1".into())));
        assert_eq!(plus_action("account:claude:work"), Some(PlusAction::Account("claude:work".into())), "only the first : splits");
        assert_eq!(plus_action("model:opus"), Some(PlusAction::Model("opus".into())));
        assert_eq!(plus_action("model"), None, "a submenu row itself picks nothing");
        assert_eq!(plus_action("ultracode"), None);
    }

    /// The strip: the picked teams lead, then the staged files (a picture
    /// for an image, the glyph for a file); a chip's key names what it is.
    #[test]
    fn the_strip_leads_with_the_teams_then_the_files() {
        let team = crate::teams_pick::PickTeam { id: "t1".into(), name: "Core".into(), icon_url: None };
        let files = [
            StripFile { key: 1, filename: "shot.png", content_type: "image/png", bytes: &[1, 2, 3], is_file: false },
            StripFile { key: 2, filename: "log.txt", content_type: "text/plain", bytes: &[4], is_file: true },
        ];
        let strip = strip(&[&team], &files);
        assert_eq!(strip["open"], true);
        let items = strip["items"].as_array().unwrap();
        assert_eq!(items[0], json!({"key": "team:t1", "label": "Core", "image": "", "icon": "ui-team"}));
        assert_eq!(items[1]["image"], "data:image/png;base64,AQID");
        assert_eq!(items[1]["icon"], "image");
        assert_eq!(items[2], json!({"key": "2", "label": "log.txt", "image": "", "icon": "file"}));
        assert_eq!(super::strip(&[], &[]), json!({"open": false, "items": []}));
        assert_eq!(strip_key("team:t1"), Some(StripKey::Team("t1".into())));
        assert_eq!(strip_key("7"), Some(StripKey::File(7)));
        assert_eq!(strip_key("x"), None);
    }

    /// The footer is the chat document's own `chat_footer` (its Composer,
    /// the "+" Menu and the ring in its slots), in the band's box.
    #[test]
    fn the_footer_is_the_chat_documents() {
        let doc = document();
        let tree = uicore_gpui::inline::root_json(&doc).unwrap();
        assert_eq!(tree["id"], "chat_footer");
        assert!(tree["style"].get("maxWidth").is_none());
        let text = tree.to_string();
        for id in [COMPOSER, PLUS, RING, "chat_slash", "chat_files", "chat_replies"] {
            assert!(text.contains(&format!("\"id\":\"{id}\"")), "{id}");
        }
    }

    /// #EDE-65 lane T: the IDE card's look and rules through existing genUI:
    /// the FLAT Composer, the "+" (24 px, Tooltip `Add`, inert while a send
    /// is in flight), the `/` menu floating over the card (a Popover anchored
    /// on the composer, never in the column), the quick replies wrapping.
    #[test]
    fn the_footer_draws_the_ide_card() {
        fn node<'a>(tree: &'a Value, id: &str) -> Option<&'a Value> {
            match tree {
                Value::Object(map) if map.get("id").and_then(Value::as_str) == Some(id) => Some(tree),
                Value::Object(map) => map.values().find_map(|v| node(v, id)),
                Value::Array(items) => items.iter().find_map(|v| node(v, id)),
                _ => None,
            }
        }
        let doc = document();
        let tree = uicore_gpui::inline::root_json(&doc).unwrap();
        let composer = node(&tree, COMPOSER).expect("the composer");
        assert_eq!(composer["props"]["layout"], "flat");
        let tip = node(&tree, "chat_plus_tip").expect("the + tooltip");
        assert_eq!(tip["component"], "Tooltip");
        assert_eq!(tip["props"]["content"], crate::chat_screen::PLUS_TOOLTIP);
        let plus = node(tip, "chat_plus_button").expect("the + button");
        assert_eq!(plus["props"]["disabled"], json!({"@path": "/chatting/composer/sending"}));
        assert_eq!(plus["style"], json!({"width": 24, "height": 24, "marginTop": 4}));
        let popover = node(&tree, "chat_slash_popover").expect("the / menu's popover");
        assert_eq!(popover["component"], "Popover");
        assert_eq!(popover["props"]["anchor"], COMPOSER);
        assert_eq!(popover["props"]["side"], "top");
        assert_eq!(popover["props"]["open"], json!({"@path": "/chatMenu/open"}));
        assert!(node(popover, "chat_slash").is_some());
        let replies = node(&tree, "chat_reply_list").expect("the replies");
        assert_eq!(replies["style"]["flexWrap"], "wrap");
        assert_eq!(replies["style"]["gap"], 5.25);
    }
}
