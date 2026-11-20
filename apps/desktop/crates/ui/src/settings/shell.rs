//! #EXP-1629: the settings SHELL as eUI documents: the detail column (the
//! web settings route's own chrome, `routes/t/$teamSlug/settings/route.tsx`
//! `WideSettingsLayout`) and the left nav (web `SETTINGS_NAV`, the
//! settings sidebar).
//!
//! The detail column: "Settings" (EXP-1550: no explanatory subtitle), a
//! hairline, then the selected pane, all in one centered `max-w-4xl` column
//! (EXP-771). The pane is the host's own view in the column's Embed slot
//! (`host_leaf`): measured at the column's width before it paints. The
//! SCROLL REGION stays full width with the scrollbar at the viewport edge —
//! only the content column is capped and centered, the page-scaffold recipe
//! every other full-page screen uses (`scroll_pane::page_scaffold_with`).
//!
//! The nav: the back row over its rule (a fixed document; #EXP-2027: `←
//! Settings` in the personal nav, `←` + the team's mark + name in a team's),
//! then the ONE group (a second document in the column's scroller): one flat
//! template list of the group label and rows; the rows' routing stays the
//! Rust model's ([`super::NavInputs`]).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{div, px, AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use gpui_component::{ActiveTheme as _, IconNamed as _};
use serde_json::{json, Value};

use eui_gpui::binding::Binding;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;

use super::doc::{eq, hex, not, path, LINE, SM, XS};
use super::{nav_lit, section_visible, NavInputs, NavItem, NavTeam, SettingsSection, PERSONAL_NAV, TEAM_NAV};

/// The selected pane, built for the frame that draws it.
pub(crate) type PaneBuild = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A pane entity as the column's [`PaneBuild`].
pub(crate) fn pane_of<V: Render>(pane: &Entity<V>) -> PaneBuild {
    let view: gpui::AnyView = pane.clone().into();
    Rc::new(move |_, _| view.clone().into_any_element())
}

/// No pane (a hand-over in flight).
pub(crate) fn empty_pane() -> PaneBuild {
    Rc::new(|_, _| div().into_any_element())
}

// ---------------------------------------------------------------------------
// The detail column
// ---------------------------------------------------------------------------

/// The column's Embed: the selected pane.
const PANE_SLOT: &str = "settings_pane";

/// What the column's host paints in [`PANE_SLOT`].
#[derive(Default)]
struct PaneSlot {
    pane: RefCell<Option<PaneBuild>>,
    height: Cell<f32>,
}

impl PaneSlot {
    fn build(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let pane = self.pane.borrow().clone()?;
        Some(pane(window, cx))
    }
}

/// The IDE's plugin plus the pane slot.
struct ShellPlugin {
    inner: crate::exponential_ui_host::IdePlugin,
    slot: Rc<PaneSlot>,
}

impl HostPlugin for ShellPlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn host_leaf_height(&self, component_id: &str, _width: f32, _window: &mut Window, _cx: &mut App) -> Option<f32> {
        component_id.ends_with(PANE_SLOT).then(|| self.slot.height.get())
    }

    fn host_leaf_prepaint(&self, component_id: &str, width: f32, window: &mut Window, cx: &mut App) -> bool {
        if !component_id.ends_with(PANE_SLOT) {
            return false;
        }
        let Some(pane) = self.slot.build(window, cx) else { return false };
        let height = crate::chat_tab_transcript::measure(pane, width, window, cx);
        let old = self.slot.height.replace(height);
        (old - height).abs() > 0.01
    }

    fn host_leaf(&self, component_id: &str, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        if !component_id.ends_with(PANE_SLOT) {
            return None;
        }
        self.slot.build(window, cx)
    }
}

/// The column's tree: the header, its hairline, the pane slot, in the
/// web's `mx-auto w-full max-w-4xl space-y-6 p-6` (laid out at the web's
/// 16px rem, EXP-1557: `text-2xl` = 24px).
fn detail_document(foreground: &str, stroke: &str) -> Value {
    let size = 24.;
    let header = json!({"id": "header", "component": "Text", "props": {"text": "Settings"},
        "style": {"fontSize": size, "lineHeight": size * LINE, "fontWeight": 700, "color": foreground, "minWidth": 0}});
    // `flexShrink: 0`: a 1px child in a flex column is the first thing the
    // layout eats.
    let separator = json!({"id": "separator", "component": "Box", "style": {"height": 1, "flexShrink": 0, "width": "100%", "backgroundColor": stroke}});
    let pane = json!({"id": PANE_SLOT, "component": "Embed", "props": {"surface": {}}, "style": {"width": "100%"}});
    json!({"id": "shell", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "center", "width": "100%"},
        "children": [{"id": "column", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "maxWidth": 896, "minWidth": 0, "padding": 24, "gap": 24},
            "children": [header, separator, pane]}]})
}

/// The detail column: the document and its ONE scroll offset (the header
/// and the pane share a scroll region, so a section switch rewinds it —
/// the web's route change lands at the top of the page, never mid-list).
pub(crate) struct DetailShell {
    scroll: gpui::ScrollHandle,
    /// EXP-1558 (DEV-ONLY, `EXP_DEV_SCROLL_TO=end`): the column's max offset
    /// when it was last parked at its end ([`crate::screens::dev_park_at_end`]).
    dev_parked_end: Cell<gpui::Pixels>,
    slot: Rc<PaneSlot>,
    view: Entity<SurfaceView>,
    _binding: Entity<Binding<Value>>,
}

impl DetailShell {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let slot = Rc::new(PaneSlot::default());
        let binding = cx.new(|_| Binding::new("/shell", Value::Null, |state: &Value, _| state.clone()));
        let tree = detail_document(&hex(cx.theme().foreground), &hex(super::row_stroke(cx)));
        let plugin: Rc<dyn HostPlugin> = Rc::new(ShellPlugin { inner: super::doc::ide_plugin(), slot: slot.clone() });
        let view = super::doc::mount_with("settings-detail", tree, &binding, plugin, false, window, cx);
        Self { scroll: gpui::ScrollHandle::new(), dev_parked_end: Default::default(), slot, view, _binding: binding }
    }

    /// Back to the top (a section switch).
    pub(crate) fn rewind(&self) {
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
    }

    /// The column showing `pane`; `key` names the scroll region (one per
    /// section).
    pub(crate) fn render(&mut self, key: SharedString, pane: PaneBuild, window: &mut Window, cx: &mut App) -> AnyElement {
        *self.slot.pane.borrow_mut() = Some(pane);
        // EXP-1558 (DEV-ONLY): `settings-assistant-meeting` = the Assistant
        // pane at its end (the Meeting mode rows), web's `scrollToMeetingMode`.
        crate::screens::dev_park_at_end(&self.scroll, &self.dev_parked_end, window);
        gpui_component::v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(crate::scroll_pane::v_scroll_pane(
                key,
                &self.scroll,
                // EXP-1557: the column lays out at the web's 16px rem, so the
                // panes' type, rows and controls match web's settings page
                // size for size (the IDE's 14px rem drew them ~12% small).
                div()
                    .w_full()
                    .min_w_0()
                    .child(crate::controls::web_rem(div().w_full().min_w_0().child(self.view.clone())))
                    // #EXP-1860: the last row scrolls clear of the Assistant FAB.
                    .child(crate::assistant_fab::page_end(window, cx)),
            ))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// The nav
// ---------------------------------------------------------------------------

/// The host's name for the brand mark (the Assistant row): the IDE's own
/// `logo` asset, which no catalog icon name carries.
const LOGO: &str = "ide:logo";

/// What a nav row does.
#[derive(Clone)]
pub(crate) enum NavAction {
    Section(SettingsSection),
    /// EXP-630: fold `label`'s sub-pages (`true`) or unfold them.
    Fold(&'static str, bool),
}

/// The inks the nav writes in.
pub(crate) struct NavPaint {
    rem: f32,
    /// The column's text (`sidebar_foreground`).
    ink: String,
    muted: String,
    foreground: String,
    fill_active: String,
    fill_row: String,
    stroke_row: String,
    border: String,
    radius: f32,
}

impl NavPaint {
    pub(crate) fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            rem: f32::from(theme.font_size),
            ink: hex(theme.sidebar_foreground),
            muted: hex(theme.muted_foreground),
            foreground: hex(theme.foreground),
            fill_active: hex(theme::tokens::glass::FILL_ACTIVE.to_hsla()),
            fill_row: hex(theme::tokens::glass::FILL_ROW.to_hsla()),
            stroke_row: hex(theme::tokens::glass::STROKE_ROW.to_hsla()),
            border: hex(theme.sidebar_border),
            radius: f32::from(theme.radius),
        }
    }
}

/// The concept every static page's row wears (EXP-317: the web nav's
/// `settings-*` concepts, `settings/-shared.tsx`; locked by the web
/// `lib/icons.test.ts` settings-nav parity check).
pub(crate) fn section_icon(section: &SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => "settings-general",
        SettingsSection::Members => "settings-members",
        SettingsSection::Issues => "settings-issues",
        SettingsSection::Labels => "settings-labels",
        SettingsSection::Statuses => "settings-statuses",
        SettingsSection::Storage => "settings-storage",
        SettingsSection::AllBoards => "settings-boards",
        SettingsSection::Board(_) => "settings-boards",
        SettingsSection::Repositories | SettingsSection::Repository(_) => "settings-repositories",
        SettingsSection::Widget => "settings-widget",
        SettingsSection::Mcp | SettingsSection::McpActivity => "settings-mcp",
        SettingsSection::Tools => "settings-tools",
        SettingsSection::AgentClis | SettingsSection::Agents | SettingsSection::Agent(_) | SettingsSection::NewAgent => "settings-agents",
        SettingsSection::App(_) => "nav-apps",
        SettingsSection::Prompts | SettingsSection::Prompt(_) | SettingsSection::NewPrompt => "nav-actions",
        SettingsSection::Skills | SettingsSection::Skill(_) | SettingsSection::NewSkill => "ui-docs",
        SettingsSection::LocalRepos => "settings-local-repos",
        SettingsSection::Sessions => "settings-sessions",
        SettingsSection::Account => "settings-account",
        SettingsSection::Notifications => "settings-notifications",
        SettingsSection::Voice => "settings-voice",
        SettingsSection::ApiKeys => "settings-api",
        SettingsSection::About => "settings-about",
    }
}

/// One list entry under construction.
struct Entries<'a> {
    paint: &'a NavPaint,
    items: Vec<Value>,
    actions: HashMap<String, NavAction>,
}

impl Entries<'_> {
    fn push(&mut self, mut item: Value) {
        item["gap"] = json!(if self.items.is_empty() { 0. } else { self.paint.rem * 0.125 });
        for key in ["label", "icon", "tint", "color", "fill", "fold", "foldKey", "foldLabel", "chevron"] {
            if item.get(key).is_none() {
                item[key] = json!("");
            }
        }
        if item.get("indent").is_none() {
            item["indent"] = json!(self.paint.rem * 0.5);
        }
        for key in ["lit", "foldOpen"] {
            if item.get(key).is_none() {
                item[key] = json!(false);
            }
        }
        self.items.push(item);
    }

    fn label(&mut self, group: &str, label: &str) {
        self.push(json!({"key": format!("label:{group}"), "kind": "label", "label": label}));
    }

    /// A row: `tint` = its glyph's own colour (the row's ink without one),
    /// `muted` = the New rows' ink.
    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, key: String, label: &str, icon: &str, tint: Option<String>, lit: bool, muted: bool, action: NavAction) {
        let ink = if muted { &self.paint.muted } else { &self.paint.ink };
        let fill = if lit { self.paint.fill_active.clone() } else { "#00000000".to_string() };
        self.push(json!({"key": key, "kind": "row", "label": label, "icon": icon, "tint": tint.unwrap_or_else(|| ink.clone()), "color": ink, "fill": fill, "lit": lit, "fold": "none"}));
        self.actions.insert(key, action);
    }
}

/// The nav's list for `inputs`: the entries (in order) and what each
/// key's press does. `folded` = the nested entries' fold state (unset =
/// unfolded while the parent or one of its sub-pages is selected).
/// #EXP-2027 (settings-nav.json `desktopNavs`): ONE group — the team's
/// (`inputs.team` Some) or the personal one; no board, agent or app rows.
pub(crate) fn nav_entries(inputs: &NavInputs, folded: &HashMap<&'static str, bool>, paint: &NavPaint, cx: &App) -> (Vec<Value>, HashMap<String, NavAction>) {
    let NavInputs { owner, team, scope_team, effective } = inputs;
    let owner = *owner;
    let mut out = Entries { paint, items: Vec::new(), actions: HashMap::new() };
    let group = if team.is_some() { &TEAM_NAV } else { &PERSONAL_NAV };
    out.label(group.id, group.label);
    for item in group.items.iter().filter(|item| section_visible(&item.section, owner)) {
        let lit = nav_lit(&item.section, effective);
        let children: Vec<&NavItem> = item.children.iter().filter(|child| section_visible(&child.section, owner)).collect();
        // EXP-630: a nested entry starts unfolded while it or one of its
        // sub-pages is open; the chevron overrides that.
        let within = lit || children.iter().any(|child| child.section == *effective);
        let open = folded.get(item.label).map(|folded| !folded).unwrap_or(within);
        if item.id == "assistant" {
            // #EXP-1987 (settings-nav.json `pages.assistant`): the
            // viewer's ONE Assistant, the brand mark as its glyph; lit on
            // its agent page (`Voice` resolves to it).
            let assistant = crate::session_data::my_assistant_anywhere(scope_team.as_deref(), cx);
            let lit = lit || assistant.is_some_and(|agent| *effective == SettingsSection::Agent(agent.id));
            out.row(item.id.to_string(), item.label, LOGO, Some(paint.foreground.clone()), lit, false, NavAction::Section(item.section.clone()));
            continue;
        }
        out.row(item.id.to_string(), item.label, section_icon(&item.section), None, lit, false, NavAction::Section(item.section.clone()));
        if !children.is_empty() {
            let last = out.items.last_mut().expect("the row");
            last["fold"] = json!(if open { "open" } else { "closed" });
            last["chevron"] = json!(if open { "ui-chevron-down" } else { "ui-chevron-right" });
            last["foldKey"] = json!(format!("fold:{}", item.id));
            last["foldOpen"] = json!(open);
            last["foldLabel"] = json!(if open { format!("Collapse {}", item.label) } else { format!("Expand {}", item.label) });
            out.actions.insert(format!("fold:{}", item.id), NavAction::Fold(item.label, open));
        }
        if open {
            for child in children {
                let lit = child.section == *effective;
                out.row(child.id.to_string(), child.label, section_icon(&child.section), None, lit, false, NavAction::Section(child.section.clone()));
                out.items.last_mut().expect("the row")["indent"] = json!(paint.rem * 1.5);
            }
        }
    }
    (out.items, out.actions)
}

/// gpui rounds a label's `text_xs` line (16.99px at the IDE's rem) to the
/// whole pixel, the document's text box keeps the fraction: the label's
/// bottom padding gives the half point back (the bench: 0 px).
const LABEL_TRIM: f32 = 0.5;

/// The nav list's tree: one flat template list at `/nav/items` (`p_2`,
/// `gap_0p5`: each entry after the first carries the 2px as its top
/// margin), each entry drawing its kind.
pub(crate) fn nav_document(p: &NavPaint) -> Value {
    let kind = |k: &str| eq(path("kind"), json!(k));
    let (sm, xs) = (p.rem * SM, p.rem * XS);
    let glyph = p.rem * 0.75;
    let r = |rems: f32| p.rem * rems;
    // `group_label`: `px_2` / `pt_2` / `pb_0p5`, `text_xs` SEMIBOLD muted.
    let label = json!({"id": "nav_label", "component": "Text", "visible": kind("label"), "props": {"text": path("label")},
        "style": {"fontSize": xs, "lineHeight": xs * LINE, "fontWeight": 600, "color": p.muted, "paddingHorizontal": r(0.5), "paddingTop": r(0.5), "paddingBottom": r(0.125) - LABEL_TRIM}});
    // `row`: `px_2` / `py_1`, `gap_2`, the theme radius, `text_sm`; the
    // lit fill, the hover wash; the `xsmall` glyph, the truncated label,
    // the fold chevron (`size_3`, muted).
    let chevron = json!({"id": "nav_fold", "component": "Box", "visible": not(eq(path("fold"), json!("none"))), "props": {"pressable": true},
        "accessibility": {"label": path("foldLabel"), "expanded": path("foldOpen")},
        "on": {"press": {"event": {"name": "pick", "context": {"key": path("foldKey")}}}},
        "style": {"display": "flex", "flexShrink": 0, "cursor": "pointer"},
        "children": [{"id": "nav_fold_icon", "component": "Icon", "props": {"name": path("chevron")}, "style": {"width": glyph, "height": glyph, "color": p.muted, "flexShrink": 0}}]});
    let row = json!({"id": "nav_row", "component": "Box", "visible": kind("row"), "props": {"pressable": true},
        "accessibility": {"label": path("label"), "selected": path("lit")},
        "on": {"press": {"event": {"name": "pick", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": r(0.5), "paddingLeft": path("indent"), "paddingRight": r(0.5), "paddingVertical": r(0.25),
            "borderRadius": p.radius, "backgroundColor": path("fill"), "cursor": "pointer", ":hover": {"backgroundColor": p.fill_row}},
        "children": [
            {"id": "nav_row_icon", "component": "Icon", "props": {"name": path("icon")}, "style": {"width": glyph, "height": glyph, "color": path("tint"), "flexShrink": 0}},
            {"id": "nav_row_label", "component": "Text", "props": {"text": path("label"), "lines": 1}, "style": {"fontSize": sm, "lineHeight": sm * LINE, "color": path("color"), "flexGrow": 1, "flexShrink": 1, "minWidth": 0}},
            chevron,
        ]});
    let item = json!({"id": "nav_item", "component": "Box",
        "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "marginTop": path("gap")}, "children": [label, row]});
    json!({"id": "nav", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": r(0.5)},
        "children": [{"id": "nav_list", "component": "List", "accessibility": {"label": "Settings"}, "props": {"gap": "none"},
            "template": {"component": "nav_item", "path": "/nav/items", "key": "key"}, "children": [item]}]})
}

/// The back row over its rule (web `SidebarBackRow` + its `Separator`,
/// EXP-870): an 8px inset, the 40px row (the bare 16px back glyph, the
/// `text_sm` SEMIBOLD label), the hover wash; the sidebar-border rule
/// under it at the column's inset. #EXP-2027 (settings-nav.json
/// `desktopNavs`): a team's nav puts the team's mark (`team-image.json`: its
/// image, else the team glyph) between the glyph and the label, and the
/// label is the team's name; the personal nav's label is "Settings"
/// ([`back_state`]).
pub(crate) fn back_document(p: &NavPaint) -> Value {
    let sm = p.rem * SM;
    let r = |rems: f32| p.rem * rems;
    let mark = &domain::team_image::TEAM_IMAGE.mark;
    let side = 16.;
    json!({"id": "back_root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            {"id": "back", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Back"},
                "on": {"press": {"event": {"name": "back"}}},
                "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "height": 40, "marginHorizontal": r(0.5), "marginTop": r(0.25), "paddingHorizontal": r(0.5), "gap": r(0.5),
                    "borderRadius": p.radius, "cursor": "pointer", ":hover": {"backgroundColor": p.fill_row}},
                "children": [
                    {"id": "back_icon", "component": "Icon", "props": {"name": "ui-back"}, "style": {"width": 16, "height": 16, "color": p.ink, "flexShrink": 0}},
                    {"id": "back_team_picture", "component": "Image", "visible": path("/back/hasImage"), "props": {"src": path("/back/src"), "alt": "", "fit": "cover", "fallback": mark.glyph},
                        "style": {"width": side, "height": side, "flexShrink": 0, "borderRadius": side * mark.radius_fraction}},
                    {"id": "back_team_glyph", "component": "Icon", "visible": path("/back/glyph"), "props": {"name": mark.glyph}, "style": {"width": side, "height": side, "color": p.ink, "flexShrink": 0}},
                    {"id": "back_label", "component": "Text", "props": {"text": path("/back/label"), "lines": 1}, "style": {"fontSize": sm, "lineHeight": sm * LINE, "fontWeight": 600, "color": p.ink, "minWidth": 0}},
                ]},
            {"id": "back_rule_inset", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "flexShrink": 0, "paddingHorizontal": r(0.5)},
                "children": [{"id": "back_rule", "component": "Box", "style": {"height": 1, "width": "100%", "flexShrink": 0, "marginVertical": r(0.25), "backgroundColor": p.border}}]},
        ]})
}

/// The personal nav's header label.
pub(crate) const BACK_LABEL: &str = "Settings";

/// #EXP-2027: the header's state (the `/back` binding): `team` = the team
/// whose nav shows (its mark + name), None = `← Settings`.
pub(crate) fn back_state(team: Option<&NavTeam>) -> Value {
    match team {
        Some(team) => json!({"label": team.name, "hasImage": team.image.is_some(), "glyph": team.image.is_none(), "src": team.image.clone().unwrap_or_default()}),
        None => json!({"label": BACK_LABEL, "hasImage": false, "glyph": false, "src": ""}),
    }
}

/// The nav's plugin: the IDE's, plus the brand mark.
pub(crate) struct NavPlugin {
    pub(crate) inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for NavPlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        if name == LOGO {
            return Some(crate::icons::ExpIcon::Logo.path());
        }
        self.inner.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    /// #EXP-2027: the header's team image goes through the IDE's media rule.
    fn media_allowed(&self, src: &str) -> bool {
        self.inner.media_allowed(src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint() -> NavPaint {
        NavPaint {
            rem: 14.,
            ink: "#fafafaff".into(),
            muted: "#a1a1aaff".into(),
            foreground: "#ffffffff".into(),
            fill_active: "#ffffff1a".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            border: "#ffffff1a".into(),
            radius: 10.,
        }
    }

    /// The shell's documents raise no issue a debug build's mount check
    /// refuses (#EXP-1767).
    #[test]
    fn the_documents_raise_no_unexpected_issue() {
        for (id, tree) in [("settings-detail", detail_document("#ffffffff", "#ffffff14")), ("settings-nav", nav_document(&paint())), ("settings-nav-back", back_document(&paint()))] {
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new(id, Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{id}: {issues:?}");
        }
    }

    /// Every static page's icon is a concept the IDE resolves.
    #[test]
    fn every_section_icon_resolves() {
        for item in super::super::flat_nav_items() {
            assert!(crate::exponential_ui_host::icon_path(section_icon(&item.section)).is_some(), "{}", item.id);
        }
    }
}
