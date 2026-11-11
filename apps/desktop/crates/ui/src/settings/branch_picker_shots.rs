//! #EXP-1765: the bench's per-repository branch picker (`settings_bench`
//! `settings_repo_branch_eui`): genUI's `pill` Select in each of its states
//! (the pill, the skeleton rows, the empty list and the mono rows). The
//! IDE's own picker it was judged against went with Repositories' move to
//! it; `settings_repositories_branch_*` keep the in-pane comparison.
use std::collections::VecDeque;
use std::rc::Rc;

use gpui::{div, prelude::*, App, Entity, Window};
use serde_json::json;

use eui_gpui::view::SurfaceView;

/// What the picker shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PickerState {
    /// Closed, the trigger alone.
    Trigger,
    /// Closed, the trigger disabled (the pane is busy).
    Disabled,
    /// Open while the branches load.
    Loading,
    /// Open on a search that matches no branch ("No branches found.":
    /// the IDE lists the effective branch on any load, even a failed one).
    Empty,
    /// Open on `main` (GitHub's default, the effective one) + two more.
    Ready,
}

impl PickerState {
    fn open(self) -> bool {
        matches!(self, Self::Loading | Self::Empty | Self::Ready)
    }
}

const BRANCHES: [&str; 3] = ["main", "develop", "release/2026-11"];
/// The Empty state's search.
const NO_MATCH: &str = "zzz";

/// The picker as a genUI document: a `pill` Select, opened by pressing its
/// trigger.
pub(crate) struct EuiBranchPicker {
    view: Entity<SurfaceView>,
    presses: (VecDeque<String>, u8),
    /// Typed into the open popup's search once (the Empty state).
    typed: Option<&'static str>,
    flush: bool,
}

fn document(state: PickerState) -> serde_json::Value {
    let options: Vec<serde_json::Value> = match state {
        PickerState::Loading => Vec::new(),
        _ => BRANCHES
            .iter()
            .map(|b| {
                let mut option = json!({"label": b, "value": b, "mono": true});
                if *b == "main" {
                    option["trailing"] = json!("default");
                }
                option
            })
            .collect(),
    };
    json!({"id": "branch", "component": "Select",
        "props": {"triggerVariant": "pill", "name": "branch", "value": "main", "searchable": true, "searchPlaceholder": "Search branches...",
            "emptyText": "No branches found.", "loading": state == PickerState::Loading, "disabled": state == PickerState::Disabled,
            "accessibility": {"label": "Default branch for acme/web"}, "options": options},
        "on": {"open": {"event": {"name": "branches"}}}})
}

impl EuiBranchPicker {
    pub(crate) fn for_shots(state: PickerState) -> impl FnOnce(&mut Window, &mut gpui::Context<Self>) -> Self {
        move |window, cx| {
            let presses = if state.open() { VecDeque::from(["branch.trigger".to_string()]) } else { VecDeque::new() };
            Self { view: mount(document(state), window, cx), presses: (presses, 2), typed: (state == PickerState::Empty).then_some(NO_MATCH), flush: false }
        }
    }

    /// The trigger's press once it is laid out (two painted frames in); an
    /// open popup's enter fade is run out by hand (the headless clock
    /// stands still).
    fn press(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (queue, wait) = &mut self.presses;
        if queue.is_empty() {
            if !self.view.read(cx).layers().is_empty() {
                // Typed, then (a frame on, once the input reported the
                // edit) flushed past its debounce: the headless clock never
                // runs it out.
                if let Some(text) = self.typed.take() {
                    self.view.update(cx, |view, cx| {
                        view.type_into("branch.search", text, window, cx);
                    });
                    self.flush = true;
                    cx.notify();
                } else if std::mem::take(&mut self.flush) {
                    self.view.update(cx, |view, cx| view.flush_field("branch.search", None, window, cx));
                    cx.notify();
                }
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            return;
        }
        if *wait > 0 {
            *wait -= 1;
            cx.notify();
            return;
        }
        let id = queue.pop_front().unwrap_or_default();
        self.view.update(cx, |view, cx| {
            if let Some(index) = view.index_of(&id) {
                view.fire(index, "press", None, cx);
            }
        });
        cx.notify();
    }
}

fn mount(tree: serde_json::Value, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let options = eui_gpui::view::SurfaceViewOptions {
        surface_id: "bench-repo-branch".into(),
        theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: eui_core::theme::Mode::Dark,
        host: Rc::new(super::doc::ide_plugin()),
        lift_layers: true,
        settings: Some(eui_core::surface::SurfaceSettings { mode: eui_core::theme::ModeSetting::Dark, reduced_motion: theme::motion::reduce_motion(), ..Default::default() }),
        ..Default::default()
    };
    let tree: eui_core::NestedNode = serde_json::from_value(tree).expect("the branch document parses");
    cx.new(|cx| {
        let mut view = SurfaceView::new(options, window, cx);
        view.set_nested(tree, cx);
        view
    })
}

impl Render for EuiBranchPicker {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.press(window, cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}
