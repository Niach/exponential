//! EXP-1280 — THE rich plan / question card's interactive half ×4
//! (`packages/domain-contract/fixtures/plan-card.json`; web
//! `packages/ui/src/session-thread-view.tsx` + `ask-card.tsx`). The thread's
//! cards (`crate::thread_screen::{plan_card, question_card}`), THE ask dock
//! and the session view's TOOL-posted card all paint through here:
//!
//! - the inline Exponential UI surface (`surface` {root, data}) through the
//!   SDK's [`SurfaceView`] with a card host ([`CardSurfaceHost`]: no
//!   transport, `set` the only function, media = `/api/attachments/*` on
//!   the instance with the account's auth + https);
//! - one block per question: the prompt (strong), `chooseOne` /
//!   `chooseAny`, the options as THE option rows (text only) or as picture
//!   TILES ([`crate::controls::choice_tiles`], `plan-card.json` `tiles`),
//!   and the `custom` free-text row (`copy.custom`);
//! - N-12: a PENDING card the viewer may answer opens as THE Dock's sheet
//!   ([`plan_sheet`] / [`question_sheet`]): the plan's pages (with
//!   `stepper.minQuestions`+ questions one page per question, "Question n
//!   of m" in the header), Back / Next in the footer, the last page's
//!   footer THE decision (No, keep planning · Yes ▾ fresh; the feedback
//!   field above it once rejecting), a question's footer Answer;
//! - ANSWERED = closed: the picked options highlighted, the rest inert, the
//!   decision label + the feedback.
//!
//! The person's picks live in a window-independent draft store keyed by the
//! card's message id ([`CardDrafts`]); the fields and the surface view are
//! created on the first frame that needs them (`Window::defer`: a card
//! renders with a `&Window`).

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use domain::plan_card::{self as model, AnswerIssue, CardAnswerInput, CardAnswers, OptionRows};
use domain::session_tree::{CardSurface, PlanQuestion, QuestionOption};
use exponential_ui::host::{media_request, MediaOptions, MediaRequest, UrlPolicy};
use exponential_ui_gpui::host::HostPlugin;
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, img, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _, SharedString, Styled as _,
    StyledImage as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    input::{InputEvent, InputState, Textarea, TextareaState},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use serde_json::{json, Value};

use crate::controls::{ChoiceTile, ChoiceTilesGeometry, TilePicture, WebText as _};
use crate::icons::registry;
use crate::markdown::{ImageCache, ImageSlot};

/// The question card's one "question" in the draft store.
const QUESTION_KEY: &str = "";

// ---------------------------------------------------------------------------
// Pure helpers (tested)
// ---------------------------------------------------------------------------

/// The markdown the card paints: an `![alt](upload:<label>)` embed whose
/// upload has not landed renders NOTHING (`plan-card.json`).
pub(crate) fn strip_pending_uploads(markdown: &str) -> String {
    let scheme = &model::PLAN_CARD.upload_scheme;
    let needle = format!("]({scheme}");
    if !markdown.contains(&needle) {
        return markdown.to_string();
    }
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    while let Some(at) = rest.find(&needle) {
        // The embed's `![` before the `](upload:`, on the same line.
        let head = &rest[..at];
        let open = head.rfind("![").filter(|open| !head[*open..].contains('\n'));
        let close = rest[at + needle.len()..].find(')').map(|end| at + needle.len() + end + 1);
        match (open, close) {
            (Some(open), Some(close)) => {
                out.push_str(&rest[..open]);
                rest = &rest[close..];
            }
            _ => {
                out.push_str(&rest[..at + needle.len()]);
                rest = &rest[at + needle.len()..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The card JSON `answer_issue` reads, rebuilt from a plan row.
pub(crate) fn plan_card_json(questions: &[PlanQuestion]) -> Value {
    json!({
        "kind": "plan",
        "plan": "",
        "questions": questions.iter().map(|question| json!({
            "id": question.id,
            "prompt": question.prompt,
            "multi": question.multi,
            "custom": question.custom,
            "options": question.options.iter().map(|option| json!({"label": option.label})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// The card JSON `answer_issue` reads, rebuilt from a question row.
pub(crate) fn question_card_json(options: &[QuestionOption], multi: bool, custom: bool) -> Value {
    json!({
        "kind": "question",
        "question": "",
        "options": options.iter().map(|option| option.label.clone()).collect::<Vec<_>>(),
        "multi": multi,
        "custom": custom,
    })
}

/// Why APPROVING `answers` cannot go out yet (`None` = fine).
pub(crate) fn approve_issue(questions: &[PlanQuestion], answers: &BTreeMap<String, Vec<String>>) -> Option<AnswerIssue> {
    let input = CardAnswerInput {
        decision: Some("approved".into()),
        answers: Some(CardAnswers::ByQuestion(answers.clone())),
        ..Default::default()
    };
    model::answer_issue(&plan_card_json(questions), &input)
}

/// Why a question card's `labels` cannot go out yet.
pub(crate) fn question_issue(options: &[QuestionOption], multi: bool, custom: bool, labels: &[String]) -> Option<AnswerIssue> {
    let input = CardAnswerInput {
        answers: Some(CardAnswers::Labels(labels.to_vec())),
        ..Default::default()
    };
    model::answer_issue(&question_card_json(options, multi, custom), &input)
}

/// How a question's options render (`optionRows`).
pub(crate) fn option_rows(options: &[QuestionOption]) -> OptionRows {
    let ids: Vec<Option<String>> = options.iter().map(|option| option.attachment_id.clone()).collect();
    model::option_rows(&ids)
}

/// A pick on `label`: single = that label alone (a second press clears
/// it), multi = toggled, kept in OPTION order.
pub(crate) fn toggle_pick(picked: &[String], label: &str, multi: bool, order: &[String]) -> Vec<String> {
    if !multi {
        return if picked.iter().any(|pick| pick == label) {
            Vec::new()
        } else {
            vec![label.to_string()]
        };
    }
    let mut next: Vec<String> = picked.to_vec();
    if let Some(at) = next.iter().position(|pick| pick == label) {
        next.remove(at);
    } else {
        next.push(label.to_string());
    }
    next.sort_by_key(|pick| order.iter().position(|option| option == pick).unwrap_or(usize::MAX));
    next
}

/// A question's answer: the picked labels, then the free text (a single
/// choice takes the free text INSTEAD of a pick).
pub(crate) fn question_answer(picked: &[String], custom_text: &str, multi: bool) -> Vec<String> {
    let text = custom_text.trim();
    if text.is_empty() {
        return picked.to_vec();
    }
    if !multi {
        return vec![text.to_string()];
    }
    let mut out = picked.to_vec();
    out.push(text.to_string());
    out
}

/// A closed question card's picks: `answers`, else a legacy `answer` that
/// names an option.
pub(crate) fn closed_picks(options: &[QuestionOption], answer: Option<&str>, answers: Option<&[String]>) -> Vec<String> {
    if let Some(answers) = answers {
        return answers.to_vec();
    }
    answer
        .filter(|answer| options.iter().any(|option| option.label == *answer))
        .map(|answer| vec![answer.to_string()])
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The draft store
// ---------------------------------------------------------------------------

/// One card's answer in progress.
#[derive(Default)]
struct CardDraft {
    /// Question id ([`QUESTION_KEY`] on a question card) → picked labels.
    picks: BTreeMap<String, Vec<String>>,
    /// Question id → its free-text field.
    custom: HashMap<String, Entity<InputState>>,
    /// THE feedback field (multi-line, `card.fieldMinHeight`).
    feedback: Option<Entity<TextareaState>>,
    surface: Option<Entity<SurfaceView>>,
    /// A `Window::defer` creating the missing pieces is on its way.
    building: bool,
    /// The surface view was attempted (a surface that does not parse is
    /// never retried).
    surface_tried: bool,
    /// N-12: the plan sheet's page (0 = the plan; local, never synced).
    step: usize,
    /// N-12: the reject button was picked: the feedback field shows.
    rejecting: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct CardDrafts(HashMap<String, CardDraft>);

impl gpui::Global for CardDrafts {}

fn draft<'a>(message_id: &str, cx: &'a App) -> Option<&'a CardDraft> {
    cx.try_global::<CardDrafts>()?.0.get(message_id)
}

fn draft_mut<'a>(message_id: &str, cx: &'a mut App) -> &'a mut CardDraft {
    cx.default_global::<CardDrafts>().0.entry(message_id.to_string()).or_default()
}

/// A sent card's draft goes (its fields, its surface view) once its answer
/// LANDED ([`crate::thread_screen::answer_card`]'s success): the card is
/// `Answering…` with its picks still in it, then closed — a refusal (409,
/// 400, the network) re-opens it with everything intact. Never at render
/// time: two views of one id must not fight over it.
pub(crate) fn drop_draft(message_id: &str, cx: &mut App) {
    if cx.try_global::<CardDrafts>().is_some_and(|drafts| drafts.0.contains_key(message_id)) {
        cx.default_global::<CardDrafts>().0.remove(message_id);
    }
}

fn picks(message_id: &str, key: &str, cx: &App) -> Vec<String> {
    draft(message_id, cx).and_then(|draft| draft.picks.get(key).cloned()).unwrap_or_default()
}

fn custom_text(message_id: &str, key: &str, cx: &App) -> String {
    draft(message_id, cx)
        .and_then(|draft| draft.custom.get(key))
        .map(|state| state.read(cx).value().to_string())
        .unwrap_or_default()
}

/// N-12: the plan sheet's page (0 = the plan).
fn current_step(message_id: &str, cx: &App) -> usize {
    draft(message_id, cx).map(|draft| draft.step).unwrap_or_default()
}

fn set_step(message_id: &str, step: usize, cx: &mut App) {
    draft_mut(message_id, cx).step = step;
    cx.refresh_windows();
}

fn feedback_text(message_id: &str, cx: &App) -> String {
    draft(message_id, cx)
        .and_then(|draft| draft.feedback.as_ref())
        .map(|state| state.read(cx).value().trim().to_string())
        .unwrap_or_default()
}

/// The surface's data model now (else the card's own `data`).
fn surface_data(message_id: &str, surface: Option<&CardSurface>, cx: &App) -> Option<Value> {
    let surface = surface?;
    if let Some(view) = draft(message_id, cx).and_then(|draft| draft.surface.as_ref()) {
        return Some(view.read(cx).surface().data().clone());
    }
    Some(surface.data.clone().unwrap_or_else(|| json!({})))
}

/// What a card needs built (fields, the surface view).
struct Needs {
    /// `(question key, single choice)` per free-text field.
    custom: Vec<(String, bool)>,
    feedback: bool,
    surface: Option<CardSurface>,
}

/// Create what `needs` lists and is missing, on the next frame.
fn ensure(message_id: &str, needs: Needs, window: &Window, cx: &mut App) {
    let missing = {
        let draft = draft(message_id, cx);
        let has_custom = |key: &str| draft.is_some_and(|draft| draft.custom.contains_key(key));
        needs.custom.iter().any(|(key, _)| !has_custom(key))
            || (needs.feedback && draft.is_none_or(|draft| draft.feedback.is_none()))
            || (needs.surface.is_some() && draft.is_none_or(|draft| !draft.surface_tried))
    };
    if !missing || draft(message_id, cx).is_some_and(|draft| draft.building) {
        return;
    }
    draft_mut(message_id, cx).building = true;
    let id = message_id.to_string();
    window.defer(cx, move |window, cx| {
        build(&id, needs, window, cx);
        cx.refresh_windows();
    });
}

fn build(message_id: &str, needs: Needs, window: &mut Window, cx: &mut App) {
    let copy = model::copy();
    for (key, single) in needs.custom {
        if draft(message_id, cx).is_some_and(|draft| draft.custom.contains_key(&key)) {
            continue;
        }
        let state = cx.new(|cx| InputState::new(window, cx).placeholder(copy.custom.clone()));
        let (id, question) = (message_id.to_string(), key.clone());
        // A single choice takes the free text INSTEAD of a pick.
        let subscription = cx.subscribe(&state, move |state, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                if single && !state.read(cx).value().trim().is_empty() {
                    draft_mut(&id, cx).picks.remove(&question);
                }
                cx.refresh_windows();
            }
        });
        let draft = draft_mut(message_id, cx);
        draft.custom.insert(key, state);
        draft._subscriptions.push(subscription);
    }
    if needs.feedback && draft(message_id, cx).is_none_or(|draft| draft.feedback.is_none()) {
        let placeholder = domain::ask_dock::ASK_DOCK.copy.feedback_placeholder.clone();
        let state = cx.new(|cx| crate::controls::web_textarea(1, 6, window, cx).placeholder(placeholder));
        draft_mut(message_id, cx).feedback = Some(state);
    }
    if let Some(surface) = needs.surface {
        if draft(message_id, cx).is_none_or(|draft| !draft.surface_tried) {
            let view = surface_view(message_id, &surface, window, cx);
            let draft = draft_mut(message_id, cx);
            draft.surface = view;
            draft.surface_tried = true;
        }
    }
    draft_mut(message_id, cx).building = false;
}

/// A closed card's surface shows what the person answered: its data model
/// = the stamped `surfaceData` (else the card's own `data`).
fn ensure_closed_surface(message_id: &str, surface: Option<&CardSurface>, surface_data: Option<&Value>, window: &Window, cx: &mut App) {
    let Some(surface) = surface else { return };
    let surface = CardSurface {
        root: surface.root.clone(),
        data: surface_data.cloned().or_else(|| surface.data.clone()),
    };
    ensure(message_id, Needs { custom: Vec::new(), feedback: false, surface: Some(surface) }, window, cx);
}

// ---------------------------------------------------------------------------
// The inline surface
// ---------------------------------------------------------------------------

/// The card's host: no transport (events stay in the card), `set` the only
/// function (the built-in data write; `openUrl` and host calls do nothing),
/// media = the instance's `/api/attachments/*` with the account's bearer, or
/// any https url.
pub(crate) struct CardSurfaceHost {
    instance: Option<String>,
    token: Option<Arc<dyn api::TokenProvider>>,
}

impl CardSurfaceHost {
    pub(crate) fn new(cx: &App) -> Self {
        let account = sync::Store::try_global(cx).and_then(|_| crate::queries::active_account(cx));
        let token = account.as_ref().and_then(|account| {
            cx.try_global::<crate::session::AuthContext>()
                .map(|auth| auth.auth.token_provider(&account.id))
        });
        Self {
            instance: account.map(|account| account.instance_url),
            token,
        }
    }

    /// The media policy: https anywhere, plus the instance's own scheme
    /// (a local http instance) for its attachments.
    fn options(&self) -> MediaOptions {
        let mut schemes = vec!["https".to_string()];
        if self.instance.as_deref().is_some_and(|url| url.starts_with("http://")) {
            schemes.push("http".to_string());
        }
        MediaOptions {
            base_url: self.instance.clone(),
            schemes: Some(schemes),
            allow_private: self.own_private_host(),
            ..Default::default()
        }
    }

    /// VAPP-107: private, loopback and link-local hosts are denied by
    /// default; a self-hosted LAN instance (`http://192.168.1.20:3000`,
    /// `http://localhost:3000`) still serves the card's surface and its
    /// pictures, so the card opts EXACTLY its own API host in — never the
    /// whole LAN (`*`).
    fn own_private_host(&self) -> Option<Vec<String>> {
        own_host(self.instance.as_deref()?).map(|host| vec![host])
    }
}

/// The instance url's host as the URL policy compares it (the WHATWG host,
/// lowercased; an IPv6 host keeps its brackets).
fn own_host(instance: &str) -> Option<String> {
    let host = url::Url::parse(instance).ok()?.host_str()?.to_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Whether a surface media `src` may load: an own attachment (relative) or
/// an https url.
pub(crate) fn card_media_allowed(src: &str) -> bool {
    src.starts_with("/api/attachments/") || src.starts_with("https://")
}

impl HostPlugin for CardSurfaceHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    fn url_policy(&self) -> Option<UrlPolicy> {
        Some(UrlPolicy {
            base_url: self.instance.clone(),
            allow_private: self.own_private_host(),
            ..Default::default()
        })
    }

    fn media_options(&self) -> MediaOptions {
        self.options()
    }

    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        if !card_media_allowed(src) {
            return None;
        }
        let mut request = media_request(src, &self.options())?;
        if src.starts_with("/api/attachments/") {
            if let Some(token) = self.token.as_ref().and_then(|provider| provider.token()) {
                request.headers.insert("Authorization".into(), format!("Bearer {token}"));
            }
        }
        Some(request)
    }

    fn open_url(&self, _url: &str, _cx: &mut App) {}

    /// Sandbox: a `Video` / `AudioPlayer` press plays nothing — web's host
    /// is inert too. The default would hand an agent-authored attachment
    /// (fetched with the bearer token) to the system opener.
    fn open_media_file(&self, _path: &std::path::Path, _cx: &mut App) {}

    /// Sandbox: a `FileUpload` never opens the native picker — handled,
    /// with nothing picked (web's inert host).
    fn pick_files(&self, _request: &exponential_ui_gpui::host::FilePickRequest, _cx: &mut App) -> bool {
        true
    }
}

fn surface_view(message_id: &str, surface: &CardSurface, window: &mut Window, cx: &mut App) -> Option<Entity<SurfaceView>> {
    let tree: exponential_ui::NestedNode = match serde_json::from_value(surface.root.clone()) {
        Ok(tree) => tree,
        Err(error) => {
            log::warn!("[plan card] the surface of {message_id} does not parse: {error}");
            return None;
        }
    };
    let options = SurfaceViewOptions {
        surface_id: format!("plan-card-{message_id}"),
        theme: exponential_ui::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: exponential_ui::theme::Mode::Dark,
        host: Rc::new(CardSurfaceHost::new(cx)),
        // `None`: the surface follows its frame's width (a fixed width
        // left the surface's equal tile columns short of the frame).
        width: None,
        ..Default::default()
    };
    let data = surface.data.clone();
    let view = cx.new(|cx| SurfaceView::new(options, window, cx));
    view.update(cx, |view, cx| {
        let outcome = view.set_nested(tree, cx);
        if !outcome.issues.is_empty() {
            log::warn!("[plan card] the surface of {message_id}: {:?}", outcome.issues);
        }
        if let Some(Value::Object(data)) = data {
            for (key, value) in data {
                let path = format!("/{}", key.replace('~', "~0").replace('/', "~1"));
                if let Err(error) = view.set_data(&path, Some(value), cx) {
                    log::warn!("[plan card] the surface data {path}: {error}");
                }
            }
        }
    });
    Some(view)
}

/// The surface block: the `Preview` label (medium) ABOVE the surface in
/// its own frame (`card.surface*`; a skeleton-height box until the view
/// exists).
fn surface_block(message_id: &str, cx: &App) -> AnyElement {
    let copy = model::copy();
    let card = model::card();
    let view = draft(message_id, cx).and_then(|draft| draft.surface.clone());
    v_flex()
        .id(SharedString::from(format!("{}-{message_id}", model::test_ids().surface)))
        .mt(px(card.block_gap))
        .w_full()
        .min_w_0()
        .gap(px(card.surface_label_gap))
        .child(caption(&copy.surface, cx).font_weight(FontWeight::MEDIUM))
        .child(
            div()
                .w_full()
                .min_w_0()
                .p(px(card.surface_padding))
                .rounded(px(card.surface_radius))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(theme::tokens::glass::FILL_ROW.to_hsla())
                .overflow_hidden()
                .map(|frame| match view {
                    Some(view) => frame.child(view),
                    None => frame.min_h(px(48.)),
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

fn caption(text: &str, cx: &App) -> gpui::Div {
    div()
        .text_2xs()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(text.to_string()))
}

/// An option's picture through the screen's [`ImageCache`] (cover).
fn picture(attachment_id: &str, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
    let url = format!("/api/attachments/{attachment_id}");
    match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
        ImageSlot::Ready(image) => img(image).size_full().object_fit(ObjectFit::Cover).into_any_element(),
        _ => div().size_full().into_any_element(),
    }
}

/// The geometry from `plan-card.json` `tiles`.
fn geometry() -> ChoiceTilesGeometry {
    let tiles = model::tiles();
    ChoiceTilesGeometry {
        columns: tiles.columns.max(1) as usize,
        gap: tiles.gap,
        picture_aspect: tiles.picture_aspect,
        radius: tiles.radius,
        selected_ring: tiles.selected_ring,
        padding: tiles.padding,
        picture_radius: tiles.picture_radius,
        stroke: tiles.stroke,
        placeholder_glyph: tiles.placeholder_glyph,
        label_gap: tiles.label_gap,
        inert_opacity: tiles.inert_opacity,
    }
}

/// A pick toggled on `key`: a single choice's pick clears its free text.
fn pick(message_id: &str, key: &str, label: &str, multi: bool, order: &[String], window: &mut Window, cx: &mut App) {
    let current = picks(message_id, key, cx);
    let next = toggle_pick(&current, label, multi, order);
    let draft = draft_mut(message_id, cx);
    draft.picks.insert(key.to_string(), next);
    if !multi {
        if let Some(state) = draft.custom.get(key).cloned() {
            state.update(cx, |state, cx| state.set_value("", window, cx));
        }
    }
    cx.refresh_windows();
}

/// The options of one question: THE option rows or the tiles; `None`
/// interactive = closed (picked highlighted, the rest inert).
#[allow(clippy::too_many_arguments)] // one question's facts
fn options_block(
    message_id: &str,
    key: &str,
    options: &[QuestionOption],
    multi: bool,
    picked: &[String],
    interactive: bool,
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> AnyElement {
    let ids = model::test_ids();
    let order: Rc<Vec<String>> = Rc::new(options.iter().map(|option| option.label.clone()).collect());
    if option_rows(options) == OptionRows::Tiles {
        let tiles = options
            .iter()
            .map(|option| ChoiceTile {
                label: SharedString::from(option.label.clone()),
                description: option.description.clone().map(SharedString::from),
                picture: Some(match option.attachment_id.as_deref() {
                    Some(id) => TilePicture::Element(picture(id, images, cx)),
                    None => TilePicture::Placeholder(Icon::new(registry::EDITOR_IMAGE)),
                }),
                selected: picked.contains(&option.label),
            })
            .collect();
        let on_pick = interactive.then(|| {
            let (id, key, order) = (message_id.to_string(), key.to_string(), order.clone());
            Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
                if let Some(label) = order.get(index) {
                    pick(&id, &key, label, multi, &order, window, cx);
                }
            }) as crate::controls::OnTilePick
        });
        return div()
            .mt(px(model::card().options_gap))
            .w_full()
            .child(crate::controls::choice_tiles(
                format!("{}-{message_id}-{key}", ids.tile),
                tiles,
                &geometry(),
                on_pick,
                cx,
            ))
            .into_any_element();
    }
    let mut rows = v_flex().mt(px(model::card().options_gap)).w_full().min_w_0().gap_1();
    for (index, option) in options.iter().enumerate() {
        let selected = picked.contains(&option.label);
        if !interactive {
            rows = rows.child(closed_option(option, selected, cx));
            continue;
        }
        let (id, key, label, order) = (message_id.to_string(), key.to_string(), option.label.clone(), order.clone());
        rows = rows.child(
            crate::steer_viewer::ask_choice_button(
                SharedString::from(format!("{}-{message_id}-{key}-{index}", ids.option)),
                option.label.clone(),
                option.description.clone(),
                selected,
                multi,
                cx,
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                pick(&id, &key, &label, multi, &order, window, cx);
            }),
        );
    }
    rows.into_any_element()
}

/// A closed option row: the leading emerald check on a pick, the rest
/// muted at `card.inertOpacity`; no fill (`plan-card.json` `card`).
fn closed_option(option: &QuestionOption, picked: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let card = model::card();
    gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(card.row_min_height))
        .gap_1p5()
        .items_center()
        .px_3()
        .py_1p5()
        .rounded(px(card.row_radius))
        .when(!picked, |row| row.opacity(card.inert_opacity))
        .child(
            div().w(px(14.)).flex_shrink_0().when(picked, |slot| {
                slot.child(Icon::new(registry::UI_CHECK).xsmall().text_color(theme::tokens::GREEN.to_hsla()))
            }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(if picked { cx.theme().foreground } else { muted })
                .child(SharedString::from(option.label.clone())),
        )
        .into_any_element()
}

/// The free-text row (`copy.custom`) under a question's options.
fn custom_field(message_id: &str, key: &str, window: &Window, cx: &App) -> Option<AnyElement> {
    let state = draft(message_id, cx)?.custom.get(key)?.clone();
    Some(
        div()
            .id(SharedString::from(format!("{}-{message_id}-{key}", model::test_ids().custom)))
            .mt_1()
            .w_full()
            .min_w_0()
            .child(
                crate::controls::glass_input(&state, window, cx)
                    .small()
                    // `Input::h` is multi-line only: the box's own min height.
                    .min_h(px(model::card().row_min_height))
                    .text_xs(),
            )
            .into_any_element(),
    )
}

/// One plan question: prompt (strong), the choose caption, its options,
/// the free-text row.
#[allow(clippy::too_many_arguments)] // one question's facts
fn question_block(
    message_id: &str,
    question: &PlanQuestion,
    picked: &[String],
    interactive: bool,
    top: f32,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let copy = model::copy();
    let choose = if question.multi { &copy.choose_any } else { &copy.choose_one };
    let options = options_block(message_id, &question.id, &question.options, question.multi, picked, interactive, images, cx);
    // A closed card's free-text answer (not an option) reads as a line.
    let free: Vec<String> = picked
        .iter()
        .filter(|pick| !question.options.iter().any(|option| option.label == **pick))
        .cloned()
        .collect();
    let card = model::card();
    v_flex()
        .id(SharedString::from(format!("{}-{message_id}-{}", model::test_ids().question, question.id)))
        .mt(px(top))
        .w_full()
        .min_w_0()
        .child(
            div()
                .w_full()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(SharedString::from(question.prompt.clone())),
        )
        .child(caption(choose, cx).mt(px(card.block_inner_gap)))
        .child(options)
        .when(interactive && question.custom, |block| {
            block.children(custom_field(message_id, &question.id, window, cx))
        })
        .when(!interactive && !free.is_empty(), |block| {
            block.child(crate::steer_viewer::ask_resolved_line(Some(&free.join(&model::PLAN_CARD.answer_text.joiner)), false, cx))
        })
        .into_any_element()
}

/// A plan draft's answers: picks + free text per question, answered only.
fn plan_answers(message_id: &str, questions: &[PlanQuestion], cx: &App) -> BTreeMap<String, Vec<String>> {
    questions
        .iter()
        .filter_map(|question| {
            let answer = question_answer(&picks(message_id, &question.id, cx), &custom_text(message_id, &question.id, cx), question.multi);
            (!answer.is_empty()).then(|| (question.id.clone(), answer))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The plan card
// ---------------------------------------------------------------------------

/// A plan row's facts.
pub(crate) struct PlanFacts<'a> {
    pub message_id: &'a str,
    pub questions: &'a [PlanQuestion],
    pub surface: Option<&'a CardSurface>,
    pub decision: Option<&'a str>,
    pub fresh: bool,
    pub answers: Option<&'a BTreeMap<String, Vec<String>>>,
    pub feedback: Option<&'a str>,
    /// The surface's data model as the person answered it.
    pub surface_data: Option<&'a Value>,
}

/// The plan card IN THE TRANSCRIPT (N-12): the surface + every question
/// block (`content`), then the closed lines, the answering line or nothing
/// (`controls`). A pending card the viewer may decide lives in THE Dock
/// ([`plan_sheet`]); here it is read-only.
pub(crate) fn plan_parts(facts: &PlanFacts, images: &Entity<ImageCache>, window: &Window, cx: &mut App) -> (Vec<AnyElement>, AnyElement) {
    let message_id = facts.message_id;
    let answering = crate::thread_screen::answering(message_id, cx);
    prepare_plan(facts, window, cx);
    let mut content: Vec<AnyElement> = Vec::new();
    if facts.surface.is_some() {
        content.push(surface_block(message_id, cx));
    }
    for question in facts.questions {
        let picked = plan_picked(facts, question, cx);
        content.push(question_block(message_id, question, &picked, false, model::card().block_gap, images, window, cx));
    }
    let controls = match facts.decision {
        Some(decision) => closed_decision(facts, decision, cx),
        None if answering => crate::thread_screen::card_answering(cx),
        None => div().into_any_element(),
    };
    (content, controls)
}

/// Builds what a plan card's form needs (the free-text fields, the
/// feedback field, the surface view), or a closed card's surface.
fn prepare_plan(facts: &PlanFacts, window: &Window, cx: &mut App) {
    if facts.decision.is_none() {
        ensure(
            facts.message_id,
            Needs {
                custom: facts.questions.iter().filter(|q| q.custom).map(|q| (q.id.clone(), !q.multi)).collect(),
                feedback: true,
                surface: facts.surface.cloned(),
            },
            window,
            cx,
        );
    } else {
        ensure_closed_surface(facts.message_id, facts.surface, facts.surface_data, window, cx);
    }
}

fn plan_picked(facts: &PlanFacts, question: &PlanQuestion, cx: &App) -> Vec<String> {
    if facts.decision.is_some() {
        facts.answers.and_then(|answers| answers.get(&question.id).cloned()).unwrap_or_default()
    } else {
        question_answer(&picks(facts.message_id, &question.id, cx), "", question.multi)
    }
}

/// A decided card's lines: `planDecisionLabel`, then the feedback.
fn closed_decision(facts: &PlanFacts, decision: &str, cx: &App) -> AnyElement {
    let label = domain::ask_dock::plan_decision_label(decision, facts.fresh);
    v_flex()
        .w_full()
        .min_w_0()
        .child(crate::steer_viewer::ask_resolved_line(Some(&label), false, cx))
        .when_some(facts.feedback, |column, feedback| {
            column.child(
                div()
                    .id(SharedString::from(format!("{}-{}", model::test_ids().feedback, facts.message_id)))
                    .mt_1()
                    .w_full()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(model::copy().feedback_line(feedback))),
            )
        })
        .into_any_element()
}

/// N-12: THE plan sheet (a DockCard opened, plan-card.json `stepper`): page
/// 0 = the plan, the surface and (below `minQuestions` questions) the
/// questions; else one page per question, "Question n of m" as the header's
/// meta; the footer = Back / Next, the last page THE decision
/// ([`plan_footer`]). The page is local (the draft), never synced.
pub(crate) fn plan_sheet(
    facts: &PlanFacts,
    plan: AnyElement,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> crate::dock::SheetContent {
    let message_id = facts.message_id;
    let decided = facts.decision.is_some();
    let answering = crate::thread_screen::answering(message_id, cx);
    let interactive = !decided && !answering;
    prepare_plan(facts, window, cx);
    let count = facts.questions.len();
    let stepped = !decided && model::stepper().steps(count);
    let pages = if stepped { 1 + count } else { 1 };
    let page = current_step(message_id, cx).min(pages - 1);
    let last = page + 1 == pages;
    let question = (stepped && page > 0).then(|| &facts.questions[page - 1]);
    let mut body = v_flex()
        .id(SharedString::from(format!("{}-{message_id}", model::test_ids().card)))
        .w_full()
        .min_w_0();
    match question {
        Some(question) => {
            let picked = plan_picked(facts, question, cx);
            body = body.child(question_block(message_id, question, &picked, interactive, 0., images, window, cx));
        }
        None => {
            body = body.child(plan);
            if facts.surface.is_some() {
                body = body.child(surface_block(message_id, cx));
            }
            if !stepped {
                for question in facts.questions {
                    let picked = plan_picked(facts, question, cx);
                    body = body.child(question_block(message_id, question, &picked, interactive, model::card().block_gap, images, window, cx));
                }
            }
        }
    }
    let mut footer = v_flex().w_full().min_w_0().gap_2();
    if interactive && last {
        footer = footer.child(plan_footer(facts, page, window, cx));
    } else {
        footer = footer.children(step_buttons(message_id, page, pages));
    }
    if answering {
        footer = footer.child(crate::thread_screen::card_answering(cx));
    } else if let Some(decision) = facts.decision {
        footer = footer.child(closed_decision(facts, decision, cx));
    }
    crate::dock::SheetContent {
        meta: question.map(|_| SharedString::from(model::copy().step_label(page, count))),
        body: crate::dock::sheet_body(SharedString::from(format!("{}-{message_id}", domain::dock::DOCK.test_ids.sheet_body)), body),
        footer: Some(crate::dock::sheet_footer(footer)),
    }
}

/// A footer button (small secondary, or the primary).
fn footer_button(id: String, label: &str, primary: bool) -> Button {
    let button = Button::new(SharedString::from(id)).small().cursor_pointer().label(label.to_string());
    if primary {
        button.primary()
    } else {
        button.with_variant(ButtonVariant::Secondary)
    }
}

/// N-12: Back / Next in the sheet's footer: small secondary buttons,
/// `stepper.buttonGap` apart; Back leads the row, Next trails it; Back
/// hidden on the first page, Next on the last.
fn step_buttons(message_id: &str, page: usize, pages: usize) -> Option<AnyElement> {
    let back = page > 0;
    let next = page + 1 < pages;
    if !back && !next {
        return None;
    }
    let ids = model::test_ids();
    let copy = model::copy();
    Some(
        h_flex()
            .w_full()
            .items_center()
            .gap(px(model::stepper().button_gap))
            .when(back, |row| row.child(step_button(message_id, &ids.step_back, &copy.back, page - 1)))
            .child(div().flex_1())
            .when(next, |row| row.child(step_button(message_id, &ids.step_next, &copy.next, page + 1)))
            .into_any_element(),
    )
}

fn step_button(message_id: &str, test_id: &str, label: &str, to: usize) -> Button {
    let id = message_id.to_string();
    footer_button(format!("{test_id}-{message_id}"), label, false).on_click(move |_, _, cx| {
        cx.stop_propagation();
        set_step(&id, to, cx);
    })
}

/// N-12 (web `PlanCardFooter`): the LAST page's footer = THE decision:
/// Back (when there are pages) leading; `reject` ("No, keep planning") as a
/// small secondary button, then `approve` ("Yes") as the small primary with
/// a trailing chevron menu holding `approve-fresh`; the primary is disabled
/// while a required answer is missing (the issue line under the row).
/// Picking the reject opens the feedback field above the row and the row
/// reads Cancel + the reject label as the primary.
fn plan_footer(facts: &PlanFacts, page: usize, window: &Window, cx: &mut App) -> AnyElement {
    let message_id = facts.message_id;
    let ids = model::test_ids();
    let copy = model::copy();
    let options = domain::ask_dock::plan_options();
    let find = |key: &str| options.iter().find(|option| option.key == key);
    let (Some(approve), Some(reject)) = (find("approve"), find("reject")) else {
        return div().into_any_element();
    };
    let fresh = find("approve-fresh");
    let rejecting = draft(message_id, cx).is_some_and(|draft| draft.rejecting);
    let answers = plan_answers(message_id, facts.questions, cx);
    let blocked = (!rejecting).then(|| approve_issue(facts.questions, &answers)).flatten();
    let questions: Rc<Vec<PlanQuestion>> = Rc::new(facts.questions.to_vec());
    let surface = facts.surface.cloned();
    let decide = {
        let (id, questions, surface) = (message_id.to_string(), questions.clone(), surface.clone());
        Rc::new(move |decision: &str, fresh: bool, cx: &mut App| {
            send_plan(&id, decision, fresh, &questions, surface.as_ref(), cx);
        })
    };
    let set_rejecting = |on: bool| {
        let id = message_id.to_string();
        move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            draft_mut(&id, cx).rejecting = on;
            cx.refresh_windows();
        }
    };
    let mut trailing = h_flex().items_center().gap(px(model::stepper().button_gap));
    if rejecting {
        let decide = decide.clone();
        trailing = trailing
            .child(footer_button(format!("thread-plan-{message_id}-cancel"), &copy.cancel, false).on_click(set_rejecting(false)))
            .child(
                footer_button(format!("thread-plan-{message_id}-decide"), &reject.label, true).on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    decide("rejected", false, cx);
                }),
            );
    } else {
        let approve_decide = decide.clone();
        let mut split = h_flex().id(SharedString::from(format!("{}-{message_id}", ids.decision))).items_center().child(
            footer_button(format!("thread-plan-{message_id}-decide"), &approve.label, true)
                .rounded_r_none()
                .disabled(blocked.is_some())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    approve_decide("approved", false, cx);
                }),
        );
        if let Some(fresh) = fresh {
            let (label, fresh_decide) = (fresh.label.clone(), decide.clone());
            // THE IDE menu (in-window: the Dock sheet is a layer of the
            // window, a sheet host), opening above the footer.
            use crate::controls::PointerMenuButtonExt as _;
            split = split.child(
                Button::new(SharedString::from(format!("thread-plan-{message_id}-fresh")))
                    .primary()
                    .small()
                    .cursor_pointer()
                    .rounded_l_none()
                    .border_l_1()
                    .border_color(cx.theme().primary_foreground.opacity(0.2))
                    .icon(Icon::new(registry::UI_CHEVRON_DOWN).with_size(px(14.)))
                    .tooltip(label.clone())
                    .disabled(blocked.is_some())
                    .pointer_menu(move |menu, _, _| {
                        let fresh_decide = fresh_decide.clone();
                        menu.item(crate::controls::pointer_label_item(label.clone(), false).on_click(move |_, _, cx| {
                            fresh_decide("approved", true, cx);
                        }))
                    })
                    .anchor(gpui::Anchor::BottomRight),
            );
        }
        trailing = trailing
            .child(footer_button(format!("thread-plan-{message_id}-reject"), &reject.label, false).on_click(set_rejecting(true)))
            .child(split);
    }
    let row = h_flex()
        .w_full()
        .items_center()
        .gap(px(model::stepper().button_gap))
        .when(page > 0, |row| row.child(step_button(message_id, &ids.step_back, &copy.back, page - 1)))
        .child(div().flex_1())
        .child(trailing);
    let mut column = v_flex().w_full().min_w_0().gap_2();
    if rejecting {
        if let Some(state) = draft(message_id, cx).and_then(|draft| draft.feedback.clone()) {
            column = column.child(
                div()
                    .id(SharedString::from(format!("{}-{message_id}", ids.feedback)))
                    .w_full()
                    .min_w_0()
                    .child(feedback_field(&state, window, cx)),
            );
        }
    }
    column = column.child(row);
    if let Some(issue) = &blocked {
        column = column.child(
            div()
                .id(SharedString::from(format!("{}-{message_id}", ids.issue)))
                .text_2xs()
                .text_color(theme::tokens::YELLOW.to_hsla())
                .child(SharedString::from(issue.copy().to_string())),
        );
    }
    column.into_any_element()
}

/// THE feedback field (web `PlanCardFooter`): the multi-line field inside
/// the composer card (radius xl, the card fill and hairline), borderless,
/// `card.fieldMinHeight` tall, `sm` text, the ask-dock placeholder.
fn feedback_field(state: &Entity<TextareaState>, _window: &Window, _cx: &App) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .p_2()
        .rounded(px(theme::tokens::radius::XL))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(crate::dock::card_fill())
        .child(
            Textarea::new(state)
                .appearance(false)
                .w_full()
                .min_h(px(model::card().field_min_height))
                .text_sm(),
        )
        .into_any_element()
}

fn send_plan(message_id: &str, decision: &str, fresh: bool, questions: &[PlanQuestion], surface: Option<&CardSurface>, cx: &mut App) {
    let answers = plan_answers(message_id, questions, cx);
    if decision == "approved" && approve_issue(questions, &answers).is_some() {
        return;
    }
    // NEW-6: the feedback field shows only for a reject; only then is it sent.
    let feedback = if decision == "rejected" { feedback_text(message_id, cx) } else { String::new() };
    let input = api::sessions::AnswerInput {
        message_id: message_id.to_string(),
        decision: Some(decision.to_string()),
        fresh: fresh.then_some(true),
        feedback: (!feedback.is_empty()).then_some(feedback),
        answers: (!answers.is_empty()).then(|| json!(answers)),
        surface_data: surface_data(message_id, surface, cx),
        ..Default::default()
    };
    crate::thread_screen::answer_card(input, cx);
}

// ---------------------------------------------------------------------------
// The question card
// ---------------------------------------------------------------------------

/// A question row's facts.
pub(crate) struct QuestionFacts<'a> {
    pub message_id: &'a str,
    pub options: &'a [QuestionOption],
    pub multi: bool,
    pub custom: bool,
    pub surface: Option<&'a CardSurface>,
    pub answer: Option<&'a str>,
    pub answers: Option<&'a [String]>,
    pub surface_data: Option<&'a Value>,
    pub answered: bool,
}

/// Builds what a question card's form needs, or a closed card's surface.
fn prepare_question(facts: &QuestionFacts, window: &Window, cx: &mut App) {
    if !facts.answered {
        ensure(
            facts.message_id,
            Needs {
                custom: if facts.custom { vec![(QUESTION_KEY.to_string(), !facts.multi)] } else { Vec::new() },
                feedback: false,
                surface: facts.surface.cloned(),
            },
            window,
            cx,
        );
    } else {
        ensure_closed_surface(facts.message_id, facts.surface, facts.surface_data, window, cx);
    }
}

/// The question's surface, choose caption, options and free-text row.
fn question_content(facts: &QuestionFacts, interactive: bool, images: &Entity<ImageCache>, window: &Window, cx: &mut App) -> (Vec<AnyElement>, Vec<String>) {
    let message_id = facts.message_id;
    let mut content: Vec<AnyElement> = Vec::new();
    if facts.surface.is_some() {
        content.push(surface_block(message_id, cx));
    }
    let picked = if facts.answered {
        closed_picks(facts.options, facts.answer, facts.answers)
    } else {
        picks(message_id, QUESTION_KEY, cx)
    };
    if facts.multi && !facts.answered && !facts.options.is_empty() {
        content.push(div().mt_2().child(caption(&model::copy().choose_any, cx)).into_any_element());
    }
    if !facts.options.is_empty() {
        content.push(
            div()
                .w_full()
                .child(options_block(message_id, QUESTION_KEY, facts.options, facts.multi, &picked, interactive, images, cx))
                .into_any_element(),
        );
    }
    if interactive && facts.custom {
        content.extend(custom_field(message_id, QUESTION_KEY, window, cx));
    }
    (content, picked)
}

/// The question card IN THE TRANSCRIPT (N-12): the surface and the options
/// (`content`), then the closed line, the answering line or nothing
/// (`controls`). A pending card the viewer may answer lives in THE Dock
/// ([`question_sheet`]); here it is read-only.
pub(crate) fn question_parts(facts: &QuestionFacts, images: &Entity<ImageCache>, window: &Window, cx: &mut App) -> (Vec<AnyElement>, AnyElement) {
    let answering = crate::thread_screen::answering(facts.message_id, cx);
    prepare_question(facts, window, cx);
    let (content, picked) = question_content(facts, false, images, window, cx);
    let controls = if facts.answered {
        closed_answer(facts, &picked, cx)
    } else if answering {
        crate::thread_screen::card_answering(cx)
    } else {
        div().into_any_element()
    };
    (content, controls)
}

/// An answered card's line: the picks are highlighted above; a free-text
/// answer (or a legacy card's answer naming no option) reads as THE closed
/// line.
fn closed_answer(facts: &QuestionFacts, picked: &[String], cx: &App) -> AnyElement {
    let free: Vec<&String> = picked
        .iter()
        .filter(|pick| !facts.options.iter().any(|option| option.label == **pick))
        .collect();
    if picked.is_empty() || facts.options.is_empty() {
        crate::steer_viewer::ask_resolved_line(facts.answer, false, cx)
    } else if !free.is_empty() {
        let text = free.iter().map(|pick| pick.as_str()).collect::<Vec<_>>().join(&model::PLAN_CARD.answer_text.joiner);
        crate::steer_viewer::ask_resolved_line(Some(&text), false, cx)
    } else {
        div().into_any_element()
    }
}

/// N-12: THE question sheet (a DockCard opened): the question, the inline
/// surface, the options and the free-text row in the body; the footer =
/// Answer (the small primary, right-aligned, disabled until an answer).
pub(crate) fn question_sheet(
    facts: &QuestionFacts,
    question: AnyElement,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> crate::dock::SheetContent {
    let message_id = facts.message_id;
    let answering = crate::thread_screen::answering(message_id, cx);
    let interactive = !facts.answered && !answering;
    prepare_question(facts, window, cx);
    let (content, picked) = question_content(facts, interactive, images, window, cx);
    let body = v_flex()
        .id(SharedString::from(format!("{}-{message_id}", model::test_ids().card)))
        .w_full()
        .min_w_0()
        .child(question)
        .children(content);
    let footer = if answering {
        crate::thread_screen::card_answering(cx)
    } else if facts.answered {
        closed_answer(facts, &picked, cx)
    } else {
        h_flex().w_full().justify_end().child(answer_button(facts, cx)).into_any_element()
    };
    crate::dock::SheetContent {
        meta: None,
        body: crate::dock::sheet_body(SharedString::from(format!("{}-{message_id}", domain::dock::DOCK.test_ids.sheet_body)), body),
        footer: Some(crate::dock::sheet_footer(footer)),
    }
}

/// The question card's labels now: picks + free text.
fn question_labels(facts: &QuestionFacts, cx: &App) -> Vec<String> {
    question_answer(&picks(facts.message_id, QUESTION_KEY, cx), &custom_text(facts.message_id, QUESTION_KEY, cx), facts.multi)
}

fn answer_button(facts: &QuestionFacts, cx: &mut App) -> AnyElement {
    let labels = question_labels(facts, cx);
    let blocked = question_issue(facts.options, facts.multi, facts.custom, &labels).is_some();
    let id = facts.message_id.to_string();
    let surface = facts.surface.cloned();
    let options = facts.options.to_vec();
    let (multi, custom) = (facts.multi, facts.custom);
    Button::new(SharedString::from(format!("thread-question-{}-answer", facts.message_id)))
        .primary()
        .small()
        .cursor_pointer()
        .label(model::copy().answer.clone())
        .disabled(blocked)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let labels = question_answer(&picks(&id, QUESTION_KEY, cx), &custom_text(&id, QUESTION_KEY, cx), multi);
            if question_issue(&options, multi, custom, &labels).is_some() {
                return;
            }
            let input = api::sessions::AnswerInput {
                message_id: id.clone(),
                answer: Some(labels.join(&model::PLAN_CARD.answer_text.joiner)),
                answers: Some(json!(labels)),
                surface_data: surface_data(&id, surface.as_ref(), cx),
                ..Default::default()
            };
            crate::thread_screen::answer_card(input, cx);
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sandbox: the card's host plays no media and opens no picker (web's
    /// inert host) — a `Video` / `AudioPlayer` press and a `FileUpload`
    /// do nothing.
    #[gpui::test]
    async fn the_card_host_is_inert_for_media_and_uploads(cx: &mut gpui::TestAppContext) {
        let host = CardSurfaceHost { instance: None, token: None };
        let request = exponential_ui_gpui::host::FilePickRequest {
            surface_id: "s".into(),
            component_id: "upload".into(),
            accept: None,
            multiple: false,
        };
        let handled = cx.update(|cx| host.pick_files(&request, cx));
        assert!(handled, "handled with nothing picked: the native picker never opens");
        cx.update(|cx| host.open_media_file(std::path::Path::new("/tmp/never-opened.mp4"), cx));
    }

    fn option(label: &str, attachment: Option<&str>) -> QuestionOption {
        QuestionOption {
            label: label.into(),
            description: None,
            attachment_id: attachment.map(str::to_string),
        }
    }

    fn question(id: &str, multi: bool, custom: bool, labels: &[&str]) -> PlanQuestion {
        PlanQuestion {
            id: id.into(),
            prompt: format!("{id}?"),
            multi,
            custom,
            options: labels.iter().map(|label| option(label, None)).collect(),
        }
    }

    #[test]
    fn a_pending_upload_renders_nothing() {
        assert_eq!(
            strip_pending_uploads("# Plan\n\n![before](/api/attachments/a1)\n\n![after](upload:after)\n\nEnd"),
            "# Plan\n\n![before](/api/attachments/a1)\n\n\n\nEnd"
        );
        assert_eq!(strip_pending_uploads("no pictures"), "no pictures");
        assert_eq!(strip_pending_uploads("a ](upload:x) b"), "a ](upload:x) b");
    }

    #[test]
    fn picks_toggle_single_and_multi() {
        let order: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!(toggle_pick(&[], "b", false, &order), ["b"]);
        assert_eq!(toggle_pick(&["b".into()], "a", false, &order), ["a"]);
        assert!(toggle_pick(&["b".into()], "b", false, &order).is_empty());
        assert_eq!(toggle_pick(&["c".into()], "a", true, &order), ["a", "c"]);
        assert_eq!(toggle_pick(&["a".into(), "c".into()], "a", true, &order), ["c"]);
    }

    #[test]
    fn free_text_answers() {
        assert_eq!(question_answer(&["a".into()], "  ", false), ["a"]);
        assert_eq!(question_answer(&["a".into()], "mine", false), ["mine"]);
        assert_eq!(question_answer(&["a".into()], "mine", true), ["a", "mine"]);
    }

    #[test]
    fn approving_waits_on_every_question() {
        let questions = vec![question("a", false, false, &["x", "y"]), question("b", true, true, &["p"])];
        let mut answers = BTreeMap::new();
        answers.insert("a".to_string(), vec!["x".to_string()]);
        assert_eq!(approve_issue(&questions, &answers), Some(AnswerIssue::Missing));
        assert_eq!(AnswerIssue::Missing.copy(), "Answer every question to approve");
        answers.insert("b".to_string(), vec!["p".to_string(), "free".to_string()]);
        assert_eq!(approve_issue(&questions, &answers), None);
        assert_eq!(approve_issue(&[], &BTreeMap::new()), None, "a plain plan approves at once");
    }

    #[test]
    fn question_cards_need_an_answer() {
        let options = vec![option("x", None), option("y", None)];
        assert_eq!(question_issue(&options, false, true, &[]), Some(AnswerIssue::Missing));
        assert_eq!(question_issue(&options, false, false, &["z".into()]), Some(AnswerIssue::Unknown));
        assert_eq!(question_issue(&options, true, false, &["x".into(), "y".into()]), None);
    }

    #[test]
    fn options_with_a_picture_are_tiles() {
        assert_eq!(option_rows(&[option("a", None)]), OptionRows::List);
        assert_eq!(option_rows(&[option("a", None), option("b", Some("att"))]), OptionRows::Tiles);
    }

    #[test]
    fn closed_picks_read_answers_then_a_matching_answer() {
        let options = vec![option("Web", None), option("iOS", None)];
        assert_eq!(closed_picks(&options, Some("Web, iOS"), Some(&["Web".into(), "iOS".into()])), ["Web", "iOS"]);
        assert_eq!(closed_picks(&options, Some("Web"), None), ["Web"]);
        assert!(closed_picks(&options, Some("Only on staging"), None).is_empty());
    }

    #[test]
    fn surface_media_is_attachments_or_https() {
        assert!(card_media_allowed("/api/attachments/a1"));
        assert!(card_media_allowed("https://example.com/x.png"));
        assert!(!card_media_allowed("http://example.com/x.png"));
        assert!(!card_media_allowed("file:///etc/passwd"));
        assert!(!card_media_allowed("/api/other"));
    }

    /// VAPP-107: a self-hosted LAN instance still serves the card its own
    /// attachments and links; the opt-in is exactly its own host, never
    /// the rest of the LAN.
    #[test]
    fn a_lan_instance_opts_exactly_its_own_host_in() {
        use exponential_ui::host::decide_url;
        let host = CardSurfaceHost { instance: Some("http://192.168.1.20:3000".into()), token: None };
        let media = |src: &str| media_request(src, &host.options()).map(|request| request.url);
        assert_eq!(media("/api/attachments/a1").as_deref(), Some("http://192.168.1.20:3000/api/attachments/a1"));
        assert_eq!(media("http://192.168.1.21:3000/api/attachments/a1"), None, "a neighbour on the LAN stays denied");
        assert_eq!(media("http://127.0.0.1/x.png"), None);
        let policy = host.url_policy();
        assert!(decide_url(policy.as_ref(), "/api/attachments/a1").allowed);
        assert!(!decide_url(policy.as_ref(), "http://10.0.0.1/").allowed);
        let local = CardSurfaceHost { instance: Some("http://localhost:3000".into()), token: None };
        assert_eq!(local.own_private_host(), Some(vec!["localhost".to_string()]));
        let cloud = CardSurfaceHost { instance: Some("https://app.exponential.at".into()), token: None };
        assert!(!decide_url(cloud.url_policy().as_ref(), "http://192.168.1.20/").allowed);
    }
}
