//! ONE action as a page (SLOP-2) — the web `t/$teamSlug/actions/$actionId`
//! page. EXP-1294: an action is a SAVED PROMPT, so the page is its header
//! (back · glyph + name · pin · the owner's ⋯ · Run) over its Prompt section
//! ([`crate::action_prompt_form`], which replaced the "Edit action" dialog).
//! Run opens the composer with the action as its subject; triggers live on
//! agents now and a run's sessions on its thread.
//!
//! EXP-832: `render` builds elements and NOTHING else — the row is read once
//! per data change in [`ActionView::refresh`].

use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, FontWeight, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{button::Button, menu::DropdownMenu as _, ActiveTheme as _, Disableable as _, Icon, Sizable as _};

use crate::action_prompt_form::ActionPromptForm;
use crate::actions_view::page_scaffold;
use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{nav_for_window, resolved_screen, Navigation, Screen};
use crate::surface::glass_section_header;

pub struct ActionView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// The action on screen; `None` until the first [`Screen::Action`].
    action_id: Option<String>,
    /// The Prompt section — built once the action's row is synced, rebuilt
    /// when the page is pointed at another action.
    prompt: Option<Entity<ActionPromptForm>>,
    derived: ActionDerived,
    _subscriptions: Vec<Subscription>,
}

/// The page's data, ready to draw.
#[derive(Default)]
struct ActionDerived {
    /// The action's synced row; `None` while it has not landed — or once it
    /// is gone ([`Self::ready`] tells the two apart).
    action: Option<api::prompts::Prompt>,
    /// The `actions` shape's readiness.
    ready: bool,
}

impl ActionDerived {
    fn compute(cx: &App, action_id: Option<&str>) -> Self {
        let (Some(action_id), Some(store)) = (action_id, sync::Store::try_global(cx)) else {
            return Self::default();
        };
        let actions = store.collections().prompts.read(cx);
        let ready = actions.is_ready();
        Self {
            action: actions.get(action_id).map(api::prompts::from_row),
            ready,
        }
    }
}

impl ActionView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let mut subscriptions =
            vec![cx.observe_in(&nav, window, |this, _, window, cx| this.sync_screen(window, cx))];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.prompts.clone(), collections.pins.clone())
        });
        if let Some((actions, pins)) = watched {
            subscriptions.push(cx.observe_in(&actions, window, |this, _, window, cx| {
                this.on_actions_changed(window, cx)
            }));
            // EXP-778: the header's pin toggle reads the per-user pins rows.
            subscriptions.push(cx.observe(&pins, |_, _, cx| cx.notify()));
        }
        let mut this = Self {
            nav,
            scroll: ScrollHandle::new(),
            action_id: None,
            prompt: None,
            derived: ActionDerived::default(),
            _subscriptions: subscriptions,
        };
        this.sync_screen(window, cx);
        this
    }

    /// Follow the window's screen: a [`Screen::Action`] naming another action
    /// re-points the page (a fresh Prompt form, the scroll back at the top).
    fn sync_screen(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(Screen::Action { action_id }) = resolved_screen(&self.nav, cx) else {
            return;
        };
        if self.action_id.as_deref() == Some(action_id.as_str()) {
            return;
        }
        self.action_id = Some(action_id);
        self.prompt = None;
        self.scroll = ScrollHandle::new();
        self.derived = ActionDerived::compute(cx, self.action_id.as_deref());
        self.ensure_prompt(window, cx);
        cx.notify();
    }

    /// Build the Prompt form once the action's row is synced (a deep link
    /// can land here before the `actions` shape does).
    fn ensure_prompt(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.prompt.is_some() {
            return;
        }
        let Some(action) = self.derived.action.clone() else {
            return;
        };
        self.prompt = Some(cx.new(|cx| ActionPromptForm::new(action, window, cx)));
    }

    /// The `actions` rows moved: re-derive, build the form if the row just
    /// landed — and leave for the list if the action on screen was DELETED
    /// (by this page's own menu, or by a teammate).
    fn on_actions_changed(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let had_action = self.derived.action.is_some();
        self.refresh(cx);
        self.ensure_prompt(window, cx);
        let on_screen = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::Action { action_id }) if Some(&action_id) == self.action_id.as_ref()
        );
        // Only a READY shape can say "gone" — a refetch empties it in between.
        if had_action && self.derived.action.is_none() && self.derived.ready && on_screen {
            crate::navigation::go_back_to(window, cx, Screen::Actions);
        }
    }

    /// Re-derive the page off the collections and repaint — the ONE place
    /// the rows are computed.
    fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.derived = ActionDerived::compute(cx, self.action_id.as_deref());
        cx.notify();
    }

    /// The prompts fixture's `delete-action` (EXP-1230) behind the shared    /// The prompts fixture's `delete-action` (EXP-1230) behind the shared
    /// alert window. The page leaves for the list once the synced row is
    /// gone ([`Self::on_actions_changed`]).
    fn prompt_delete_action(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let spec = AlertSpec::from_prompt("Delete action", &domain::prompts::delete_action(&name))
        .on_ok(move |_, cx| {
            crate::actions_view::spawn_action_delete(cx, action_id.clone());
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- render ---------------------------------------------------------------

    /// The header: back · the action's glyph + name · pin · the owner's ⋯
    /// (Delete) · Run.
    fn render_header(
        &self,
        action: &api::prompts::Prompt,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;

        // EXP-367: no agent CLI → Run disabled with the reason, never hidden.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let run_id = action.id.clone();
        let run = crate::controls::glass_icon_button(
            "action-page-run",
            Icon::from(registry::ACTION_RUN),
            cx,
        )
        .tooltip(no_agent.clone().unwrap_or_else(|| "Run".into()))
        .disabled(no_agent.is_some())
        // EXP-825: the composer with this action as the subject chip.
        .on_click(move |_: &ClickEvent, window, cx| {
            crate::navigation::navigate_to_chat(
                window,
                cx,
                crate::navigation::ChatSeed::action(run_id.clone()),
            );
        });

        let menu = is_owner.then(|| {
            let view = cx.entity().downgrade();
            let delete_id = action.id.clone();
            let delete_name = action.name.clone();
            crate::controls::ghost_icon_button("action-page-menu", Icon::from(registry::UI_MORE), cx)
                .dropdown_menu(move |menu, _window, cx| {
                    let view = view.clone();
                    let delete_id = delete_id.clone();
                    let delete_name = delete_name.clone();
                    menu.item(
                        crate::controls::danger_menu_item(
                            "Delete",
                            Icon::from(registry::UI_DELETE),
                            cx,
                        )
                        .on_click(move |_, window, cx| {
                            let Some(view) = view.upgrade() else {
                                return;
                            };
                            let (id, name) = (delete_id.clone(), delete_name.clone());
                            view.update(cx, |this, cx| {
                                this.prompt_delete_action(id, name, window, cx);
                            });
                        }),
                    )
                })
        });

        gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .child(
                crate::controls::ghost_icon_button(
                    "action-page-back",
                    Icon::from(registry::UI_BACK),
                    cx,
                )
                .tooltip("Back to actions")
                .on_click(|_: &ClickEvent, window, cx| {
                    crate::navigation::go_back_to(window, cx, Screen::Actions);
                }),
            )
            .child(
                crate::icons::action_icon(action.icon.as_deref())
                    .small()
                    .flex_shrink_0()
                    .text_color(muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_base()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(foreground)
                    .child(SharedString::from(action.name.clone())),
            )
            // EXP-778: the personal pin toggle.
            .child(crate::pins::pin_toggle_button(
                "action-page-pin",
                action.team_id.clone(),
                domain::contract::PIN_KIND_ACTION,
                action.id.clone(),
                cx,
            ))
            .children(menu)
            .child(run)
            .into_any_element()
    }

    /// The page while its action is not there: still syncing, or gone.
    fn render_missing(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        // EXP-1259 D8: still syncing = the page's rows as placeholders.
        if !self.derived.ready {
            return crate::feedback::skeleton_rows(
                "action-page-skeleton",
                crate::feedback::SKELETON_ROWS,
                crate::settings::SETTINGS_ROW_H,
                cx,
            );
        }
        gpui_component::v_flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("This action no longer exists."),
            )
            .child(
                Button::new("action-page-missing-back")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label("Back to actions")
                    .on_click(|_, window, cx| {
                        crate::navigation::go_back_to(window, cx, Screen::Actions);
                    }),
            )
            .into_any_element()
    }
}

impl Render for ActionView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(action) = self.derived.action.clone() else {
            let missing = self.render_missing(cx);
            return page_scaffold(
                "action-screen-scroll",
                &self.scroll,
                gpui_component::v_flex().child(missing),
            )
            .into_any_element();
        };
        let is_owner = crate::settings::is_owner(cx, &action.team_id);

        let header = self.render_header(&action, is_owner, cx);
        // NO gap on a headed section (EXP-697): the band's own margin IS the
        // spacing to what sits under it.
        let prompt = gpui_component::v_flex()
            .min_w_0()
            .child(glass_section_header("Prompt", None, cx))
            .children(self.prompt.clone().map(|form| div().pt_1().child(form)));
        page_scaffold(
            "action-screen-scroll",
            &self.scroll,
            gpui_component::v_flex().gap_6().child(header).child(prompt),
        )
        .into_any_element()
    }
}
