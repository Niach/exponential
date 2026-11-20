//! #EXP-2027 (sidebar-tree.json TEAM MENU): a team menu's New › Board |
//! Agent | App as a PAGE in the content card (never a dialog), the web's
//! `/t/$teamSlug/new/<kind>`: the page title over its hairline (the settings
//! detail column's), then the create-board form (hosted the way the
//! onboarding wizard hosts it), the agent page in create mode, or the apps
//! New menu's entries as rows (apps.json `run.newMenu`: New app, Import…).
//! Owner-only: anyone else lands on Home. New › Session is Home with the
//! team as its Teams pick (`shell_regions::team_menu`).

use gpui::{div, px, AnyElement, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::create_board_dialog::{BoardCreated, CreateBoardDialogView};
use crate::icons::registry;
use crate::navigation::{navigate, NewItemKind, Screen};

/// The page titles (web `NEW_AGENT_LABEL`, apps.json `copy.newApp`, the
/// board form's).
pub(crate) fn new_item_title(kind: NewItemKind) -> &'static str {
    match kind {
        NewItemKind::Board => "New board",
        NewItemKind::Agent => "New agent",
        NewItemKind::App => "New app",
    }
}

enum Body {
    None,
    Board(Entity<CreateBoardDialogView>),
    Agent(Entity<crate::settings::new_agent::NewAgentPane>),
    App,
}

pub(crate) struct NewItemPage {
    target: Option<(String, NewItemKind)>,
    body: Body,
    scroll: ScrollHandle,
    _subscription: Option<Subscription>,
}

impl NewItemPage {
    pub(crate) fn new(_window: &mut Window, _cx: &mut gpui::Context<Self>) -> Self {
        Self { target: None, body: Body::None, scroll: ScrollHandle::new(), _subscription: None }
    }

    /// Point the page at `kind` in `team_id`; a new target starts a fresh
    /// form. A member (not an owner) lands on Home.
    pub(crate) fn set_target(&mut self, team_id: String, kind: NewItemKind, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let target = Some((team_id.clone(), kind));
        if self.target == target {
            return;
        }
        if !crate::settings::is_owner(cx, &team_id) && crate::navigation::shapes_ready(cx) {
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, cx| navigate(window, cx, Screen::Home));
            });
            return;
        }
        self.target = target;
        self._subscription = None;
        self.body = match kind {
            NewItemKind::Board => {
                let form = cx.new(|cx| CreateBoardDialogView::new(team_id, true, window, cx));
                self._subscription = Some(cx.subscribe_in(&form, window, |page, _, _: &BoardCreated, window, cx| {
                    // The form scoped the window to the new board: its list.
                    page.target = None;
                    navigate(window, cx, Screen::BoardIssues { board_id: String::new() });
                }));
                Body::Board(form)
            }
            NewItemKind::Agent => Body::Agent(cx.new(|cx| crate::settings::new_agent::NewAgentPane::new(team_id, window, cx))),
            NewItemKind::App => Body::App,
        };
        self.scroll.set_offset(gpui::Point::default());
        cx.notify();
    }

    /// The apps New menu's entries as rows (web `new/app.tsx`).
    fn app_rows(&self, team_id: &str, cx: &mut gpui::Context<Self>) -> AnyElement {
        let row = |id: &'static str, icon: crate::icons::ExpIcon, label: &'static str, cx: &gpui::App| {
            h_flex()
                .id(id)
                .w_full()
                .gap_3()
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .hover(|row| row.bg(cx.theme().secondary))
                .child(Icon::from(icon).small().text_color(cx.theme().muted_foreground))
                .child(div().flex_1().min_w_0().truncate().child(SharedString::from(label)))
        };
        let (new_team, import_team) = (team_id.to_string(), team_id.to_string());
        crate::surface::glass_group_rows(vec![
            div().w_full().child(row("new-app-newApp", registry::UI_ADD, "New app", cx).on_click(move |_, window, cx| crate::apps_view::new_app(new_team.clone(), window, cx))),
            div().w_full().child(row("new-app-import", registry::UI_FILE, "Import…", cx).on_click(move |_, window, cx| crate::app_import_dialog::open(window, cx, import_team.clone()))),
        ])
        .into_any_element()
    }
}

impl Render for NewItemPage {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "NewItemPage").entered();
        let Some((team_id, kind)) = self.target.clone() else {
            return div().size_full().into_any_element();
        };
        let body: Option<AnyElement> = match &self.body {
            Body::Board(form) => Some(form.clone().into_any_element()),
            Body::Agent(pane) => Some(pane.clone().into_any_element()),
            Body::App => Some(self.app_rows(&team_id, cx)),
            Body::None => None,
        };
        // The settings detail column's head (settings/shell.rs
        // `detail_document`): the title at 24px bold over its hairline.
        let column = v_flex()
            .min_w_0()
            .gap(px(24.))
            .child(div().text_size(px(24.)).font_weight(gpui::FontWeight::BOLD).child(SharedString::from(new_item_title(kind))))
            .child(div().h(px(1.)).flex_shrink_0().w_full().bg(crate::settings::row_stroke(cx)))
            .children(body);
        crate::scroll_pane::page_scaffold_with("new-item-scroll", &self.scroll, column, 896., window, cx).into_any_element()
    }
}
