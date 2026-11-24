//! #EXP-2077 (`fixtures/chat-express.json` WHICH ROWS, DRAW): the chat
//! document draws the Express blocks of an agent's text like the IDE's main
//! lane (`ui::express_view`). The split and the per-block streams are THE
//! SDK half the IDE runs ([`express_blocks`], `coding`'s own source: the
//! shell never links the launcher, so it compiles that one file); a block's
//! surface reaches the document as an Embed card (`{version, root, data}`),
//! which [`embed_surface`] nests from the A2UI messages the block's stream
//! produced. The shell's rows are synced `message` rows, so every row is
//! FINAL; a row without a block never builds a row model (the gate first).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use domain::chat_document::{ExpressBlock, ExpressSegment};
use domain::chat_express::has_express_block;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::FlatComponent;
use serde_json::{json, Map, Value};

#[allow(dead_code)]
#[path = "../../coding/src/express_blocks.rs"]
mod express_blocks;

use express_blocks::{BlockState, BlockStatus, ExpressRow, RowSegment};

/// A row's split, by its key and the text it was made from (a synced row
/// only changes by an S-3 in-place edit).
static ROWS: Mutex<Option<HashMap<String, (String, Option<Vec<ExpressSegment>>)>>> = Mutex::new(None);

/// Rows kept before the cache starts over (a chat rarely holds this many
/// rows WITH blocks; the open chat's rows re-split on the next build).
const KEPT: usize = 256;

/// The row's segments (`None` = no block: the row draws as it is), the
/// `split` of `domain::chat_document::express`.
pub fn split(row_key: &str, text: &str) -> Option<Vec<ExpressSegment>> {
    if !has_express_block(text) {
        return None;
    }
    let mut rows = ROWS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let rows = rows.get_or_insert_with(HashMap::new);
    if let Some((of, segments)) = rows.get(row_key) {
        if of == text {
            return segments.clone();
        }
    }
    let segments = segments(row_key, text);
    if rows.len() >= KEPT {
        rows.clear();
    }
    rows.insert(row_key.to_string(), (text.to_string(), segments.clone()));
    segments
}

/// One FINAL row split and compiled (no cache).
fn segments(row_key: &str, text: &str) -> Option<Vec<ExpressSegment>> {
    let segments = ExpressRow::new(row_key).update(text, true)?;
    Some(
        segments
            .into_iter()
            .map(|segment| match segment {
                RowSegment::Prose(text) => ExpressSegment::Prose(text),
                RowSegment::Block(block) => ExpressSegment::Block(ExpressBlock {
                    surfaces: block.surfaces.iter().filter_map(|id| Some((id.clone(), embed_surface(&block, id)?))).collect(),
                    open: block.status == BlockStatus::Open,
                    failed: block.status.marks(),
                    key: block.key,
                }),
            })
            .collect(),
    )
}

/// The surface a server→client message is about.
fn message_surface(message: &Value) -> Option<&str> {
    message.as_object()?.values().find_map(|body| body.get("surfaceId")?.as_str())
}

/// Surface `surface_id` of `block` as the Embed takes it: its components
/// nested from `root`, its data model as the block's stream left it.
/// `None` = no root yet (nothing to draw).
pub fn embed_surface(block: &BlockState, surface_id: &str) -> Option<Value> {
    // No theme: only the component list and the data model are read.
    let mut surface = Surface::new(surface_id, SurfaceOptions { theme: None, ..SurfaceOptions::default() });
    for message in block.messages.iter().filter(|message| message_surface(message) == Some(surface_id)) {
        if let Err(error) = surface.apply(message) {
            tracing::warn!("[express] {}: {error}", block.key);
        }
    }
    let by_id: HashMap<&str, &FlatComponent> = surface.components().iter().map(|flat| (flat.id.as_str(), flat)).collect();
    let root = nest(&by_id, "root", &mut HashSet::new(), 1)?;
    let data = match surface.data() {
        Value::Object(data) => Value::Object(data.clone()),
        _ => Value::Object(Map::new()),
    };
    Some(json!({"version": eui_core::catalog::A2UI_VERSION, "root": root, "data": data}))
}

/// The Embed's depth bound (`eui_core::embed`'s sound node).
const MAX_DEPTH: usize = 64;

/// A flat component as the nested authoring form: its props under
/// `props`, its children and slots by value. A data `template` keeps its
/// `{component, path, key}` and lists the template's node as its child
/// (the reducer lifts it out, Round 2 §4). An id placed once already, an
/// unknown one or one past the depth bound is left out.
fn nest(by_id: &HashMap<&str, &FlatComponent>, id: &str, placed: &mut HashSet<String>, depth: usize) -> Option<Value> {
    let flat = by_id.get(id)?;
    if depth > MAX_DEPTH || !placed.insert(id.to_string()) {
        return None;
    }
    let mut node = Map::new();
    node.insert("id".into(), json!(flat.id));
    node.insert("component".into(), json!(flat.component));
    if !flat.rest.is_empty() {
        node.insert("props".into(), Value::Object(flat.rest.clone()));
    }
    if let Some(style) = &flat.style {
        node.insert("style".into(), Value::Object(style.clone()));
    }
    if let Some(visible) = &flat.visible {
        node.insert("visible".into(), visible.clone());
    }
    if let Some(on) = &flat.on {
        node.insert("on".into(), json!(on));
    }
    if let Some(accessibility) = &flat.accessibility {
        node.insert("accessibility".into(), accessibility.clone());
    }
    if let Some(catalog_id) = &flat.catalog_id {
        node.insert("catalogId".into(), json!(catalog_id));
    }
    if let Some(metadata) = &flat.metadata {
        node.insert("metadata".into(), metadata.clone());
    }
    let children: Vec<Value> = match &flat.children {
        None => Vec::new(),
        Some(eui_core::types::FlatChildren::Ids(ids)) => ids.iter().filter_map(|child| nest(by_id, child, placed, depth + 1)).collect(),
        Some(template @ eui_core::types::FlatChildren::Template { component_id, .. }) => {
            node.insert("template".into(), json!(template.template()));
            nest(by_id, component_id, placed, depth + 1).into_iter().collect()
        }
    };
    if !children.is_empty() {
        node.insert("children".into(), Value::Array(children));
    }
    if let Some(slots) = &flat.slots {
        let slots: Map<String, Value> = slots.iter().filter_map(|(slot, child)| Some((slot.clone(), nest(by_id, child, placed, depth + 1)?))).collect();
        if !slots.is_empty() {
            node.insert("slots".into(), Value::Object(slots));
        }
    }
    Some(Value::Object(node))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = "Here is the build:\n<a2ui>\nroot = Stack(children=[title, ship])\ntitle = Text(text=\"Build 1482 is green\")\nship = Button(label=\"Ship it\", on={press: Event(\"reply\", {text: \"Ship it\"})})\n</a2ui>\nShip it?";

    fn block(segments: &[ExpressSegment], at: usize) -> &ExpressBlock {
        match &segments[at] {
            ExpressSegment::Block(block) => block,
            other => panic!("{other:?}"),
        }
    }

    /// Prose without a tag never builds a row model.
    #[test]
    fn a_row_without_a_block_draws_as_it_is() {
        assert_eq!(split("m:plain", "Merged **#212**, the build is green."), None);
        assert_eq!(split("m:inline", "Wrap a view in `<a2ui>` tags."), None);
    }

    /// The row's prose and its block; the block's surface is a card the
    /// Embed mounts (a sound nested root), its press an event.
    #[test]
    fn a_row_with_a_block_splits_into_prose_and_a_mountable_card() {
        let segments = segments("m:m1", ROW).expect("a block");
        assert_eq!(segments[0], ExpressSegment::Prose("Here is the build:".into()));
        assert_eq!(segments[2], ExpressSegment::Prose("Ship it?".into()));
        let block = block(&segments, 1);
        assert_eq!((block.key.as_str(), block.open, block.failed), ("m:m1#1", false, false));
        let (id, surface) = &block.surfaces[0];
        assert_eq!(id, "m:m1#1", "the default surface is the block's key");
        let card = eui_core::embed::embed_card(surface).expect("the Embed takes it");
        let node = card.node().expect("a nested node");
        assert_eq!((node.id.as_str(), node.component.as_str()), ("root", "Stack"));
        let children = node.children.expect("children");
        assert_eq!(children.iter().map(|child| child.id.as_str()).collect::<Vec<_>>(), ["title", "ship"]);
        assert_eq!(children[0].props.as_ref().unwrap()["text"], "Build 1482 is green");
        let press = &children[1].on.as_ref().unwrap()["press"];
        assert_eq!(press["event"]["name"], "reply");
        assert_eq!(press["event"]["context"]["text"], "Ship it");
        // The nested form reduces without an issue (the same tree the
        // block's own stream drew).
        let reduced = eui_core::reducer::reduce_nested(&card.node().unwrap(), &eui_core::reducer::ReduceOptions::new(eui_core::catalog::CORE_CATALOG_ID));
        assert!(reduced.issues.is_empty(), "{:?}", reduced.issues);
    }

    /// A final row whose block never closed is failed: THE marker.
    #[test]
    fn an_unclosed_block_of_a_synced_row_is_failed() {
        let segments = segments("m:m2", "<a2ui>\nroot = Text(text=\"x\")").expect("a block");
        let block = block(&segments, 0);
        assert!(block.failed && !block.open);
    }

    /// A block's data model and a template list ride into the card.
    #[test]
    fn data_and_a_template_ride_into_the_card() {
        let text = "<a2ui>\n$/deals = [{name: \"Acme\"}, {name: \"Globex\"}]\nroot = List(children=_template($/deals, deal, key=\"name\"))\ndeal = Text(text=$name)\n</a2ui>";
        let segments = segments("m:m3", text).expect("a block");
        let block = block(&segments, 0);
        assert!(!block.failed, "{block:?}");
        let surface = &block.surfaces[0].1;
        assert_eq!(surface["data"]["deals"][1]["name"], "Globex");
        assert_eq!(surface["root"]["template"]["component"], "deal");
        assert_eq!(surface["root"]["children"][0]["id"], "deal", "the template's node rides as a child (lifted by the reducer)");
        assert!(eui_core::embed::embed_card(surface).is_some());
    }

    /// The cache answers the same text again and re-splits an edited row.
    #[test]
    fn the_cache_follows_the_text() {
        let first = split("m:cache", ROW).expect("a block");
        assert_eq!(split("m:cache", ROW).as_ref(), Some(&first));
        let edited = ROW.replace("Ship it?", "Ready?");
        let second = split("m:cache", &edited).expect("a block");
        assert_eq!(second[2], ExpressSegment::Prose("Ready?".into()));
    }
}
