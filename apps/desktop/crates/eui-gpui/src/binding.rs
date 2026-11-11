//! #EXP-1629: the native host binding. A host that draws one of its OWN
//! screens as a document (a settings pane: live rows, mutations, toasts)
//! keeps the screen's state here instead of in a hand-built view:
//!
//! - **State → data model.** The binding owns the screen's state `S` and a
//!   projection `S → Value` written at one data-model path. Every change
//!   re-projects; an unchanged projection writes nothing. Entities the
//!   projection reads (a host's synced collections) are [`Binding::observe`]d,
//!   so a row that changes elsewhere repaints the screen.
//! - **Events → mutations.** A named action (`on.<event>.event.name`) or a
//!   committed text edit maps to a [`Mutation`]: the optimistic change applied
//!   at once, the host call run off the main thread, and on failure the
//!   change reverted and the failure toasted through the host's [`Toasts`].
//! - **Host-owned editors.** The surface's text fields are gpui-component
//!   `InputState`s already (`view::input`): the binding hears only their
//!   `Commit`s, and a commit older than the last one it took for that field
//!   is dropped (a slow blur never overwrites a newer Enter).
//!
//! The binding knows nothing of the host's transport: a commit is any
//! `FnOnce() -> Result<(), String> + Send` (a tRPC call, a file write). Mount
//! the document with a [`BindingHost`] as its [`HostPlugin`]: it routes the
//! actions and inputs here and everything else to the host's own plugin.

use std::{collections::HashMap, rc::Rc};

use gpui::{App, Context, Entity, Subscription, WeakEntity};
use serde_json::Value;

use crate::{
    host::{ActionEvent, FilePickRequest, FunctionCallEvent, HostPlugin, InputEvent, InputKind, MarkdownAttachment, MarkdownOptions, PaintError, UploadEvent},
    view::SurfaceView,
};

/// The host's toasts: what a failed commit says.
pub trait Toasts: 'static {
    /// `title` = the failure (`Couldn't update your notification
    /// preferences`), `detail` = the host call's error.
    fn failed(&self, title: &str, detail: Option<&str>, cx: &mut App);
}

type Edit<S> = Box<dyn FnOnce(&mut S)>;
type Commit = Box<dyn FnOnce() -> Result<(), String> + Send>;

/// One change a binding makes for an event: `apply` at once (optimistic),
/// then `commit` off the main thread; a failed commit runs `revert` and
/// toasts `failure`.
pub struct Mutation<S> {
    apply: Edit<S>,
    revert: Option<Edit<S>>,
    commit: Option<Commit>,
    failure: Option<String>,
    key: Option<String>,
}

impl<S: 'static> Mutation<S> {
    /// A change to the state alone (nothing to commit).
    pub fn local(apply: impl FnOnce(&mut S) + 'static) -> Self {
        Self { apply: Box::new(apply), revert: None, commit: None, failure: None, key: None }
    }

    /// An optimistic change committed by `commit`.
    pub fn new(apply: impl FnOnce(&mut S) + 'static, commit: impl FnOnce() -> Result<(), String> + Send + 'static) -> Self {
        Self { apply: Box::new(apply), revert: None, commit: Some(Box::new(commit)), failure: None, key: None }
    }

    /// What undoes `apply` when the commit fails.
    pub fn revert(mut self, revert: impl FnOnce(&mut S) + 'static) -> Self {
        self.revert = Some(Box::new(revert));
        self
    }

    /// The field this changes: a failed commit reverts only while no later
    /// mutation of the same key was applied (a newer edit stays).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// The toast's title when the commit fails; none = the failure is silent.
    pub fn on_failure(mut self, title: impl Into<String>) -> Self {
        self.failure = Some(title.into());
        self
    }
}

type ActionHandler<S> = Rc<dyn Fn(&S, &ActionEvent, &mut App) -> Option<Mutation<S>>>;
type InputHandler<S> = Rc<dyn Fn(&S, &InputEvent, &mut App) -> Option<Mutation<S>>>;
type Projection<S> = Box<dyn Fn(&S, &App) -> Value>;

/// A document's state, projected into its data model (see the module doc).
pub struct Binding<S: 'static> {
    view: Option<WeakEntity<SurfaceView>>,
    path: String,
    state: S,
    project: Projection<S>,
    actions: HashMap<String, ActionHandler<S>>,
    inputs: HashMap<String, InputHandler<S>>,
    /// The last committed revision per text field (its component id).
    revisions: HashMap<String, u64>,
    /// The latest mutation per [`Mutation::key`] (a sequence number).
    latest: HashMap<String, u64>,
    sequence: u64,
    toasts: Option<Rc<dyn Toasts>>,
    shown: Option<Value>,
    _subscriptions: Vec<Subscription>,
}

impl<S: 'static> Binding<S> {
    /// A binding writing `project(state)` at `path` of the view it is
    /// [`Self::attach`]ed to.
    pub fn new(path: impl Into<String>, state: S, project: impl Fn(&S, &App) -> Value + 'static) -> Self {
        Self {
            view: None,
            path: path.into(),
            state,
            project: Box::new(project),
            actions: HashMap::new(),
            inputs: HashMap::new(),
            revisions: HashMap::new(),
            latest: HashMap::new(),
            sequence: 0,
            toasts: None,
            shown: None,
            _subscriptions: Vec::new(),
        }
    }

    /// The view the projection is written into; writes it now.
    pub fn attach(&mut self, view: &Entity<SurfaceView>, cx: &mut Context<Self>) {
        self.view = Some(view.downgrade());
        self.shown = None;
        self.push(cx);
    }

    /// Where a failed commit is reported.
    pub fn set_toasts(&mut self, toasts: Rc<dyn Toasts>) {
        self.toasts = Some(toasts);
    }

    /// The handler of the action called `name`.
    pub fn on_action(&mut self, name: impl Into<String>, handler: impl Fn(&S, &ActionEvent, &mut App) -> Option<Mutation<S>> + 'static) {
        self.actions.insert(name.into(), Rc::new(handler));
    }

    /// The handler of a committed text edit of the field called `name` (its
    /// `name` prop).
    pub fn on_input(&mut self, name: impl Into<String>, handler: impl Fn(&S, &InputEvent, &mut App) -> Option<Mutation<S>> + 'static) {
        self.inputs.insert(name.into(), Rc::new(handler));
    }

    /// Re-project whenever `entity` notifies (a collection the projection
    /// reads).
    pub fn observe<E: 'static>(&mut self, entity: &Entity<E>, cx: &mut Context<Self>) {
        self._subscriptions.push(cx.observe(entity, |this, _, cx| this.push(cx)));
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    /// Changes the state (a load landed) and re-projects.
    pub fn update_state(&mut self, edit: impl FnOnce(&mut S), cx: &mut Context<Self>) {
        edit(&mut self.state);
        self.push(cx);
    }

    /// The projection as last written (`None` before the first write).
    pub fn shown(&self) -> Option<&Value> {
        self.shown.as_ref()
    }

    /// Writes the projection when it changed. Deferred: an event reaches the
    /// binding from inside the view's own update.
    pub fn push(&mut self, cx: &mut Context<Self>) {
        let value = (self.project)(&self.state, cx);
        if self.shown.as_ref() == Some(&value) {
            return;
        }
        self.shown = Some(value.clone());
        let Some(view) = self.view.clone() else { return };
        let path = self.path.clone();
        cx.defer(move |cx| {
            let _ = view.update(cx, |view, cx| view.set_data(&path, Some(value), cx));
        });
    }

    /// Runs `mutation`: the optimistic change, then its commit.
    pub fn run(&mut self, mutation: Mutation<S>, cx: &mut Context<Self>) {
        let Mutation { apply, revert, commit, failure, key } = mutation;
        self.sequence += 1;
        let sequence = self.sequence;
        if let Some(key) = &key {
            self.latest.insert(key.clone(), sequence);
        }
        apply(&mut self.state);
        self.push(cx);
        let Some(commit) = commit else { return };
        let task = cx.background_executor().spawn(async move { commit() });
        cx.spawn(async move |this, cx| {
            let Err(error) = task.await else { return };
            let _ = this.update(cx, |this, cx| {
                let superseded = key.as_ref().is_some_and(|key| this.latest.get(key) != Some(&sequence));
                if let Some(revert) = revert.filter(|_| !superseded) {
                    revert(&mut this.state);
                    this.push(cx);
                }
                if let (Some(title), Some(toasts)) = (failure, this.toasts.clone()) {
                    toasts.failed(&title, Some(&error).filter(|e| !e.is_empty()).map(String::as_str), cx);
                }
            });
        })
        .detach();
    }

    /// An action the surface fired; `false` = no handler took it.
    pub fn handle_action(&mut self, event: &ActionEvent, cx: &mut Context<Self>) -> bool {
        let Some(handler) = self.actions.get(&event.name).cloned() else { return false };
        if let Some(mutation) = handler(&self.state, event, cx) {
            self.run(mutation, cx);
        }
        true
    }

    /// A host-owned text edit; only a `Commit` newer than the field's last
    /// one runs. `false` = no handler took it.
    pub fn handle_input(&mut self, event: &InputEvent, cx: &mut Context<Self>) -> bool {
        let Some(handler) = self.inputs.get(&event.name).cloned() else { return false };
        if event.kind != InputKind::Commit {
            return true;
        }
        if self.revisions.get(&event.component_id).is_some_and(|last| *last >= event.revision) {
            return true;
        }
        self.revisions.insert(event.component_id.clone(), event.revision);
        if let Some(mutation) = handler(&self.state, event, cx) {
            self.run(mutation, cx);
        }
        true
    }
}

/// The [`HostPlugin`] of a bound document: its actions and inputs go to the
/// [`Binding`] (one it has no handler for, and everything else, to `inner`).
pub struct BindingHost<S: 'static> {
    binding: WeakEntity<Binding<S>>,
    inner: Rc<dyn HostPlugin>,
}

impl<S: 'static> BindingHost<S> {
    pub fn new(binding: &Entity<Binding<S>>, inner: Rc<dyn HostPlugin>) -> Self {
        Self { binding: binding.downgrade(), inner }
    }
}

impl<S: 'static> HostPlugin for BindingHost<S> {
    fn after_layout(&self, surface_id: &str, cx: &mut App) {
        self.inner.after_layout(surface_id, cx)
    }

    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        self.inner.icon(name)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        let taken = self.binding.update(cx, |binding, cx| binding.handle_action(event, cx)).unwrap_or(false);
        if !taken {
            self.inner.on_action(event, cx)
        }
    }

    fn on_hover(&self, component_id: &str, hovered: bool, cx: &mut App) {
        self.inner.on_hover(component_id, hovered, cx)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        self.inner.icon_element(name, size, color, cx)
    }

    fn host_leaf_height(&self, component_id: &str, width: f32, window: &mut gpui::Window, cx: &mut App) -> Option<f32> {
        self.inner.host_leaf_height(component_id, width, window, cx)
    }

    fn host_leaf(&self, component_id: &str, window: &mut gpui::Window, cx: &mut App) -> Option<gpui::AnyElement> {
        self.inner.host_leaf(component_id, window, cx)
    }

    fn host_leaf_prepaint(&self, component_id: &str, width: f32, window: &mut gpui::Window, cx: &mut App) -> bool {
        self.inner.host_leaf_prepaint(component_id, width, window, cx)
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        self.inner.on_function_call(event, cx)
    }

    fn on_function_call_then(&self, event: &FunctionCallEvent, then: Box<dyn FnOnce(&mut App)>, cx: &mut App) {
        self.inner.on_function_call_then(event, then, cx)
    }

    fn url_policy(&self) -> Option<eui_core::host::UrlPolicy> {
        self.inner.url_policy()
    }

    fn media_options(&self) -> eui_core::host::MediaOptions {
        self.inner.media_options()
    }

    fn media_allowed(&self, src: &str) -> bool {
        self.inner.media_allowed(src)
    }

    fn media_request(&self, src: &str) -> Option<eui_core::host::MediaRequest> {
        self.inner.media_request(src)
    }

    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        let taken = self.binding.update(cx, |binding, cx| binding.handle_input(event, cx)).unwrap_or(false);
        if !taken {
            self.inner.on_input(event, cx)
        }
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        self.inner.open_url(url, cx)
    }

    fn text_links(&self, text: &str, cx: &App) -> Option<Vec<eui_core::host::TextLink>> {
        self.inner.text_links(text, cx)
    }

    fn image_size(&self, src: &str, cx: &App) -> Option<(f32, f32)> {
        self.inner.image_size(src, cx)
    }

    fn image_pressable(&self, cx: &App) -> bool {
        self.inner.image_pressable(cx)
    }

    fn on_image_press(&self, src: &str, window: &mut gpui::Window, cx: &mut App) {
        self.inner.on_image_press(src, window, cx)
    }

    fn attachment(&self, href: &str, cx: &App) -> Option<MarkdownAttachment> {
        self.inner.attachment(href, cx)
    }

    fn markdown_hooks_version(&self, text: &str, cx: &App) -> Option<u64> {
        self.inner.markdown_hooks_version(text, cx)
    }

    fn markdown_visible_only(&self, cx: &App) -> bool {
        self.inner.markdown_visible_only(cx)
    }

    fn markdown_window_selection(&self, cx: &App) -> bool {
        self.inner.markdown_window_selection(cx)
    }

    fn chip_icon(&self, name: &str, size: f32, color: gpui::Hsla) -> Option<gpui::AnyElement> {
        self.inner.chip_icon(name, size, color)
    }

    fn tasks_pressable(&self, cx: &App) -> bool {
        self.inner.tasks_pressable(cx)
    }

    fn on_task_toggle(&self, ordinal: usize, checked: bool, window: &mut gpui::Window, cx: &mut App) {
        self.inner.on_task_toggle(ordinal, checked, window, cx)
    }

    fn markdown_options(&self, cx: &App) -> MarkdownOptions {
        self.inner.markdown_options(cx)
    }

    fn markdown_options_for(&self, text: &str, cx: &App) -> MarkdownOptions {
        self.inner.markdown_options_for(text, cx)
    }

    fn open_media_file(&self, path: &std::path::Path, cx: &mut App) {
        self.inner.open_media_file(path, cx)
    }

    fn on_paint_error(&self, error: &PaintError, cx: &mut App) {
        self.inner.on_paint_error(error, cx)
    }

    fn on_paint_retry(&self, surface_id: &str, component_id: &str, cx: &mut App) {
        self.inner.on_paint_retry(surface_id, component_id, cx)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn on_hosted_popups(&self, layers: &[eui_core::surface::Layer]) {
        self.inner.on_hosted_popups(layers)
    }

    fn app_files(&self) -> Option<std::sync::Arc<dyn crate::host::AppFiles>> {
        self.inner.app_files()
    }

    fn resolve_url(&self, src: &str) -> String {
        self.inner.resolve_url(src)
    }

    fn font_family(&self, family: &str) -> gpui::SharedString {
        self.inner.font_family(family)
    }

    fn announce(&self, text: &str, live: &str, cx: &mut App) {
        self.inner.announce(text, live, cx)
    }

    fn scroll_surface(&self, x: f32, y: f32, cx: &mut App) {
        self.inner.scroll_surface(x, y, cx)
    }

    fn qr_scanner_feed(&self, component_id: &str, formats: &[String], window: &mut gpui::Window, cx: &mut App) -> Option<gpui::AnyElement> {
        self.inner.qr_scanner_feed(component_id, formats, window, cx)
    }

    fn pick_files(&self, request: &FilePickRequest, cx: &mut App) -> bool {
        self.inner.pick_files(request, cx)
    }

    fn composer_submits_empty(&self, component_id: &str, cx: &App) -> bool {
        self.inner.composer_submits_empty(component_id, cx)
    }

    fn on_upload(&self, event: &UploadEvent, cx: &mut App) {
        self.inner.on_upload(event, cx)
    }

    fn markdown(&self, text: &str, text_style: &eui_core::measure::TextStyle, width: f32, window: &mut gpui::Window, cx: &mut App) -> Option<gpui::AnyElement> {
        self.inner.markdown(text, text_style, width, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{AppContext as _, TestAppContext};
    use serde_json::json;

    use super::*;

    struct Recorder(Rc<RefCell<Vec<String>>>);

    impl Toasts for Recorder {
        fn failed(&self, title: &str, detail: Option<&str>, _: &mut App) {
            self.0.borrow_mut().push(format!("{title}: {}", detail.unwrap_or_default()));
        }
    }

    fn binding(cx: &mut TestAppContext) -> (Entity<Binding<i32>>, Rc<RefCell<Vec<String>>>) {
        let toasts = Rc::new(RefCell::new(Vec::new()));
        let recorder = Rc::new(Recorder(toasts.clone()));
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/x", 0, |state: &i32, _| json!(state));
            binding.set_toasts(recorder);
            binding
        });
        (binding, toasts)
    }

    fn state(binding: &Entity<Binding<i32>>, cx: &mut TestAppContext) -> i32 {
        binding.read_with(cx, |binding, _| *binding.state())
    }

    #[gpui::test]
    fn a_failed_commit_reverts_and_toasts(cx: &mut TestAppContext) {
        let (binding, toasts) = binding(cx);
        binding.update(cx, |b, cx| b.run(Mutation::new(|s| *s = 1, || Err("offline".into())).revert(|s| *s = 0).on_failure("Couldn't save"), cx));
        assert_eq!(state(&binding, cx), 1, "optimistic");
        cx.run_until_parked();
        assert_eq!(state(&binding, cx), 0);
        assert_eq!(*toasts.borrow(), vec!["Couldn't save: offline".to_string()]);
        assert_eq!(binding.read_with(cx, |b, _| b.shown().cloned()), Some(json!(0)));
    }

    #[gpui::test]
    fn a_successful_commit_keeps_the_change_quietly(cx: &mut TestAppContext) {
        let (binding, toasts) = binding(cx);
        binding.update(cx, |b, cx| b.run(Mutation::new(|s| *s = 2, || Ok(())).revert(|s| *s = 0).on_failure("Couldn't save"), cx));
        cx.run_until_parked();
        assert_eq!(state(&binding, cx), 2);
        assert!(toasts.borrow().is_empty());
    }

    #[gpui::test]
    fn a_newer_edit_of_the_same_key_survives_an_older_failure(cx: &mut TestAppContext) {
        let (binding, toasts) = binding(cx);
        binding.update(cx, |b, cx| {
            b.run(Mutation::new(|s| *s = 1, || Err(String::new())).revert(|s| *s = 0).key("n").on_failure("Couldn't save"), cx);
            b.run(Mutation::new(|s| *s = 5, || Ok(())).revert(|s| *s = 1).key("n"), cx);
        });
        cx.run_until_parked();
        assert_eq!(state(&binding, cx), 5);
        assert_eq!(toasts.borrow().len(), 1, "the failure still says so");
    }

    #[gpui::test]
    fn only_newer_commits_of_a_field_run(cx: &mut TestAppContext) {
        let (binding, _) = binding(cx);
        binding.update(cx, |b, _| {
            b.on_input("n", |_, event, _| {
                let n = event.value.as_i64()? as i32;
                Some(Mutation::local(move |s| *s = n))
            })
        });
        let input = |revision: u64, value: i32, kind: InputKind| InputEvent {
            surface_id: "s".into(),
            component_id: "field".into(),
            name: "n".into(),
            path: None,
            value: json!(value),
            revision,
            kind,
        };
        binding.update(cx, |b, cx| {
            assert!(b.handle_input(&input(1, 7, InputKind::Change), cx));
        });
        assert_eq!(state(&binding, cx), 0, "a change is not a commit");
        binding.update(cx, |b, cx| {
            b.handle_input(&input(3, 9, InputKind::Commit), cx);
            b.handle_input(&input(2, 8, InputKind::Commit), cx);
        });
        assert_eq!(state(&binding, cx), 9, "the stale blur is dropped");
        let unknown = InputEvent { name: "other".into(), ..input(4, 1, InputKind::Commit) };
        assert!(!binding.update(cx, |b, cx| b.handle_input(&unknown, cx)));
    }
}
