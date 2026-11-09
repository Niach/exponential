//! #EXP-1725: a shell document mounted in place inside a host's own window
//! (the IDE's regions), beside [`crate::Surfaces`]' one-window-per-document
//! hosts. The surface header says nothing here: the host's layout gives the
//! region its box. The ground is the host's too, so the root's own background
//! goes (eDE's layer surfaces need it, #EDE-60).

use std::{rc::Rc, sync::Arc};

pub use ede_ui::Document;

use ede_ui::Body;
use eui_core::{
    surface::SurfaceSettings,
    theme::{Mode, ModeSetting, ResolvedTheme},
};
use eui_gpui::{
    host::HostPlugin,
    view::{SurfaceView, SurfaceViewOptions},
};
use gpui::{canvas, div, prelude::*, App, Entity, Window};
use uicore_data::Feed;

/// A factory document as a host window draws it: parsed, without the root
/// background.
pub fn document(doc: &uicore_docs::Doc) -> Document {
    let mut parsed = ede_ui::validate_document(doc.name, &doc.json()).expect("a factory document validates");
    if let Body::Nested(tree) = &mut parsed.body {
        if let Some(style) = tree.style.as_mut() {
            style.remove("backgroundColor");
        }
    }
    parsed
}

/// A popup document as a host window draws it: on its own ground.
pub fn popup(doc: &uicore_docs::Doc) -> Document {
    ede_ui::validate_document(doc.name, &doc.json()).expect("a factory document validates")
}

/// #EDE-65: what eDE does not draw of the sidebar the IDE shares
/// (#EXP-1725): the IDE's header (the team switcher, search, new and the
/// rail toggle) and its menu, the IDE's nav and its corners (eDE's start,
/// chat and new-issue buttons live on the taskbar).
pub const EDE_SIDEBAR_PRUNED: &[&str] = &["header", "switcher_menu", "ide_nav", "corner"];

/// Drops the nodes a host draws itself or has no use for.
pub fn without(doc: &mut Document, ids: &[&str]) {
    fn prune(node: &mut eui_core::types::NestedNode, ids: &[&str]) {
        if let Some(children) = &mut node.children {
            children.retain(|c| !ids.contains(&c.id.as_str()));
            children.iter_mut().for_each(|c| prune(c, ids));
        }
        if let Some(slots) = &mut node.slots {
            slots.values_mut().for_each(|c| prune(c, ids));
        }
    }
    if let Body::Nested(tree) = &mut doc.body {
        prune(tree, ids);
    }
}

/// The document's tree as JSON (a styleguide entry draws it as a surface).
pub fn root_json(doc: &Document) -> Option<serde_json::Value> {
    match &doc.body {
        Body::Nested(tree) => serde_json::to_value(tree).ok(),
        _ => None,
    }
}

/// Sets a literal prop of the node `id` (a prop the catalog does not let
/// bind).
pub fn set_prop(doc: &mut Document, id: &str, prop: &str, value: serde_json::Value) {
    fn walk(node: &mut eui_core::types::NestedNode, id: &str, prop: &str, value: &serde_json::Value) {
        if node.id == id {
            node.props.get_or_insert_with(Default::default).insert(prop.to_string(), value.clone());
        }
        node.children.iter_mut().flatten().for_each(|c| walk(c, id, prop, value));
    }
    if let Body::Nested(tree) = &mut doc.body {
        walk(tree, id, prop, &value);
    }
}

/// The theme a host window draws its regions in: the regions sit on the
/// host's one ground, so the Sidebar paints no fill of its own.
pub fn theme(base: &ResolvedTheme) -> Arc<ResolvedTheme> {
    let mut theme = base.clone();
    theme.recipes.shift_remove("Sidebar");
    Arc::new(theme)
}

/// The document's view for one region of the host's window.
pub fn mount(doc: &Document, feed: &Feed, theme: Arc<ResolvedTheme>, host: Rc<dyn HostPlugin>, mode: Mode, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let (doc, feed) = (doc.clone(), feed.clone());
    cx.new(|cx| {
        let settings = SurfaceSettings { locale: "en-US".into(), mode: ModeSetting::Dark, ..SurfaceSettings::default() };
        let mut view = SurfaceView::new(SurfaceViewOptions { surface_id: doc.name.clone(), theme: Some(theme), mode, host, settings: Some(settings), ..Default::default() }, window, cx);
        view.record_bounds(true);
        crate::load(&mut view, &doc, &feed, cx);
        view
    })
}

/// Redraws a mounted view from its edited document.
pub fn reload(view: &Entity<SurfaceView>, doc: &Document, cx: &mut App) {
    if let Body::Nested(tree) = &doc.body {
        view.update(cx, |view, cx| {
            view.set_nested(tree.clone(), cx);
        });
    }
}

/// #EDE-65 lane O: a row the host paints itself (an Embed it claims by its
/// painted id through `HostPlugin::host_leaf_height` / `host_leaf`, the way
/// `icon_element` claims an icon) changed height: the document lays out
/// around it again.
pub fn host_leaf_resized(view: &Entity<SurfaceView>, id: &str, cx: &mut App) {
    view.update(cx, |view, cx| view.host_leaf_resized(id, cx));
}

/// The document's view filling the box the host's layout gives it.
pub fn region(view: &Entity<SurfaceView>) -> impl IntoElement {
    let surface = view.clone();
    div().relative().size_full().child(view.clone()).child(
        canvas(
            move |bounds, _, cx| {
                let surface = surface.clone();
                let height = f32::from(bounds.size.height);
                cx.defer(move |cx| {
                    surface.update(cx, |view, cx| {
                        view.set_viewport_height(height, cx);
                        view.set_fill_height(true, cx);
                    })
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_window_draws_the_factory_sidebar_on_its_own_ground() {
        let doc = document(uicore_docs::doc("sidebar").expect("the sidebar"));
        let Body::Nested(root) = &doc.body else { panic!("a nested document") };
        assert!(root.style.as_ref().is_some_and(|s| !s.contains_key("backgroundColor")));
        let base = eui_core::themes::builtin_theme("exponential").expect("the theme");
        assert!(base.recipes.contains_key("Sidebar"));
        assert!(!theme(&base).recipes.contains_key("Sidebar"));
    }
}
