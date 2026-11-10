//! The shapes shared by every module: the catalog source (`core.catalog.json`),
//! a normalized surface node, the nested authoring form, the A2UI wire form
//! and an extension catalog. Mirrors `packages/exponential-ui/src/types.ts`.
//!
//! Objects are `serde_json::Map` (insertion order); an `IndexMap` keeps the
//! source order of the typed tables (slots, catalog definitions, macro
//! templates).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A props / style object.
pub type Props = Map<String, Value>;

/// A data-driven child list: one `component` per item at `path`. Round 1:
/// `key` = a pointer RELATIVE to each item whose value identifies it, so a
/// reorder keeps the item's component state (the index when absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub component: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Which macro part a node came from; the theme keys its recipes on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    #[serde(rename = "macro")]
    pub macro_: String,
    pub part: String,
    pub props: Props,
    /// VAPP-163 (expansion roots only): the top-level style keys written ON
    /// the macro node, sorted; they win over the part recipe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authored: Vec<String>,
}

/// A node of the NORMALIZED tree every painter reads: core vocabulary, props
/// under `props`, children nested, macros expanded. An A2UI `Action` (the
/// `on` values) rides verbatim as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UiNode {
    pub id: String,
    pub component: String,
    #[serde(default)]
    pub props: Props,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Props>,
    /// Round 1: `true | false | {path} | {call}`; falsy once resolved = not
    /// rendered, no layout, not in the a11y tree. Absent = visible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<IndexMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<Value>,
    #[serde(default)]
    pub children: Vec<UiNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<IndexMap<String, UiNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<Template>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<Recipe>,
    /// Round 9 §5.1: the entry's `ComponentCommon.metadata`, stored and
    /// exposed, never painted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl UiNode {
    pub fn new(id: impl Into<String>, component: impl Into<String>) -> UiNode {
        UiNode { id: id.into(), component: component.into(), ..Default::default() }
    }

    /// Slots (in source order) then children: the pre-order every consumer uses.
    pub fn walk<'a>(&'a self, visit: &mut dyn FnMut(&'a UiNode)) {
        visit(self);
        if let Some(slots) = &self.slots {
            for slot in slots.values() {
                slot.walk(visit);
            }
        }
        for child in &self.children {
            child.walk(visit);
        }
    }
}

/// The nested authoring form (fixtures, the kitchen sink): a `UiNode` minus
/// what the reducer fills in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestedNode {
    pub id: String,
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub props: Option<Props>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Props>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<IndexMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<NestedNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<IndexMap<String, NestedNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<Template>,
    /// Round 9 §5.5, §13: the node's own catalog.
    #[serde(default, rename = "catalogId", skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// A2UI `children`: ids, or a template `{componentId, path, key?}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlatChildren {
    Ids(Vec<String>),
    Template {
        #[serde(rename = "componentId")]
        component_id: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
    },
}

impl FlatChildren {
    /// The normalized template of a `{componentId, path, key?}` list.
    pub fn template(&self) -> Option<Template> {
        match self {
            FlatChildren::Template { component_id, path, key } => Some(Template { component: component_id.clone(), path: path.clone(), key: key.clone() }),
            FlatChildren::Ids(_) => None,
        }
    }
}

/// A node as it rides A2UI's `updateComponents`: props at the top level,
/// children by id. `rest` holds the props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlatComponent {
    pub id: String,
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<FlatChildren>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<IndexMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Props>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<Value>,
    /// Round 9 §5.5: the entry's own catalog (else the surface default).
    #[serde(default, rename = "catalogId", skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    /// Round 9 §5.1: `{extensions?}`, stored, never painted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(flatten)]
    pub rest: Props,
}

/// One reducer complaint, by node id. Round 9 §5.2: with the error code the
/// runtime SENDS and the path (`/components/<authored id>` + the prop pointer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReduceIssue {
    pub id: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
}

impl ReduceIssue {
    /// A VALIDATION_FAILED issue at `/components/<id>`.
    pub fn new(id: impl Into<String>, message: impl Into<String>) -> ReduceIssue {
        ReduceIssue::at(id, message, "", "VALIDATION_FAILED")
    }

    /// An issue with its code, at `/components/<id>` + `pointer`.
    pub fn at(id: impl Into<String>, message: impl Into<String>, pointer: &str, code: &str) -> ReduceIssue {
        let id = id.into();
        let path = format!("/components/{}{pointer}", id.replace('~', "~0").replace('/', "~1"));
        ReduceIssue { id, message: message.into(), code: code.to_string(), path }
    }
}

// ---------------------------------------------------------------------------
// The catalog source
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropSchema {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bindable: Option<bool>,
    /// Round 1: ALSO accepts `{base, sm?, md?, lg?, xl?}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsive: Option<bool>,
    #[serde(default, rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<Value>>,
    /// `number`: the inclusive bounds a literal must respect (R8 F51).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropSchema>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefSchema {
    #[serde(default)]
    pub description: String,
    pub properties: IndexMap<String, PropSchema>,
}

/// An extension native may declare its recipe parts + `when` props.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartSpec {
    pub parts: Vec<String>,
    pub props: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDef {
    /// `native` | `macro`
    pub kind: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub lite: bool,
    /// `none` | `one` | `many`
    #[serde(default = "default_children")]
    pub children: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<Vec<String>>,
    /// Round 9 §5.6: `allowedParents` (absent = any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parents: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub props: IndexMap<String, PropSchema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<Props>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<PartSpec>,
    /// `row` (Table): the slots are cell templates bound once per ROW
    /// (`data::bind_row_slot`), never against the surface.
    #[serde(default, rename = "slotScope", skip_serializing_if = "Option::is_none")]
    pub slot_scope: Option<String>,
}

fn default_children() -> String {
    "none".into()
}

impl ComponentDef {
    pub fn is_macro(&self) -> bool {
        self.kind == "macro"
    }
    pub fn is_hidden(&self) -> bool {
        self.hidden == Some(true)
    }
    /// True for a component a model is offered: anything but the hidden
    /// placeholder.
    pub fn is_offered(&self) -> bool {
        !self.is_hidden()
    }
    /// True when the slots are ROW-SCOPED (`slotScope: "row"`, Table).
    pub fn has_row_slots(&self) -> bool {
        self.slot_scope.as_deref() == Some("row")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogFunctions {
    pub names: Vec<String>,
    /// Round 1: the core functions (`percent`, `add`, …, `set`) by name →
    /// `{description, args, returns}`.
    #[serde(default)]
    pub core: IndexMap<String, Value>,
    /// Round 4: the basic functions' signatures (`{args, returns}`).
    #[serde(default)]
    pub basic: IndexMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogSource {
    pub id: String,
    #[serde(rename = "liteId")]
    pub lite_id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "unknownComponent")]
    pub unknown_component: String,
    pub enums: IndexMap<String, Vec<String>>,
    pub defs: IndexMap<String, DefSchema>,
    pub functions: CatalogFunctions,
    /// Round 1: `<Component>.<part>[.<variant>]` → the icons.json name a
    /// renderer draws for a part it owns.
    #[serde(default, rename = "builtinIcons")]
    pub builtin_icons: IndexMap<String, Value>,
    pub components: IndexMap<String, ComponentDef>,
}

// ---------------------------------------------------------------------------
// Macro templates (catalog/macros.json)
// ---------------------------------------------------------------------------

/// A child of a template: another template node, the `"$children"` splice
/// or an author slot spliced in place (`"$slot:name"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateChild {
    Splice(String),
    /// #VAPP-277: `{"$slot": name, "$if"?, "$any"?}`, an author slot spliced
    /// only while the conditions hold (`MacroSlotSplice` in types.ts).
    Slot(MacroSlotSplice),
    Node(Box<MacroTemplate>),
}

/// #VAPP-277: a conditional slot splice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroSlotSplice {
    #[serde(rename = "$slot")]
    pub slot: String,
    #[serde(default, rename = "$if", skip_serializing_if = "Option::is_none")]
    pub if_: Option<Value>,
    #[serde(default, rename = "$any", skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<Value>>,
}

/// A template `slots` entry: `"$slot:name"` copies the author's slot; a
/// template node builds a part (AlertDialog's footer).
pub type SlotRef = TemplateChild;

/// Round 1 two-way binding: the macro prop a bound author value is written
/// back to (`set`) when the part's event fires, and the value it takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSpec {
    pub prop: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroTemplate {
    pub part: String,
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub props: Option<Props>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Props>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TemplateChild>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<IndexMap<String, SlotRef>>,
    #[serde(default, rename = "$if", skip_serializing_if = "Option::is_none")]
    pub if_: Option<Value>,
    #[serde(default, rename = "$any", skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<Value>>,
    #[serde(default, rename = "$each", skip_serializing_if = "Option::is_none")]
    pub each: Option<Value>,
    #[serde(default, rename = "$as", skip_serializing_if = "Option::is_none")]
    pub as_: Option<String>,
    #[serde(default, rename = "$on", skip_serializing_if = "Option::is_none")]
    pub on: Option<IndexMap<String, MacroRoute>>,
    #[serde(default, rename = "$context", skip_serializing_if = "Option::is_none")]
    pub context: Option<Props>,
    #[serde(default, rename = "$recipe", skip_serializing_if = "Option::is_none")]
    pub recipe: Option<Props>,
    #[serde(default, rename = "$set", skip_serializing_if = "Option::is_none")]
    pub set: Option<IndexMap<String, SetSpec>>,
    /// The part's `accessibility` (role, states, name; contract §6),
    /// evaluated like `props`.
    #[serde(default, rename = "$a11y", skip_serializing_if = "Option::is_none")]
    pub a11y: Option<Props>,
    /// eApps U3 (VAPP-232): a DATA TEMPLATE part: `item` renders once per
    /// entry of the array at `path` (evaluated like any value), keyed by
    /// `key`; the item node is lifted into the reducer's `templates`.
    #[serde(default, rename = "$template", skip_serializing_if = "Option::is_none")]
    pub template: Option<MacroDataTemplate>,
}

/// eApps U3: a macro part's `$template` (`MacroTemplate.$template` in types.ts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDataTemplate {
    pub path: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Value>,
    pub item: Box<MacroTemplate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDef {
    #[serde(rename = "recipeProps", default)]
    pub recipe_props: Vec<String>,
    pub root: MacroTemplate,
    /// #VAPP-277: props handed to nodes in an author slot BEFORE it expands
    /// (`MacroDef.$pass` in types.ts).
    #[serde(default, rename = "$pass", skip_serializing_if = "Vec::is_empty")]
    pub pass: Vec<MacroPass>,
    /// #VAPP-284: other roots, tried in order: the first whose `$if` holds
    /// AT EXPANSION (a bound condition never picks one) replaces `root`; its
    /// root part keeps the author's id. The conditions see `{childCount}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<MacroAlternative>,
    /// #EXP-1760: parts of the ROOT native the macro styles too (an
    /// IconPicker's Popover `content`), `<Macro>/<part>` over `<Native>/<part>`.
    #[serde(default, rename = "rootParts", skip_serializing_if = "Vec::is_empty")]
    pub root_parts: Vec<String>,
}

/// #VAPP-277: one `$pass` entry: when `$if` holds (literal, in the macro's
/// context) every `component` node in the author's `slot` subtree (a nested
/// node of the same macro ends the walk) takes `props` (evaluated like any
/// template value), never over a prop its author set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroPass {
    pub slot: String,
    pub component: String,
    #[serde(default, rename = "$if", skip_serializing_if = "Option::is_none")]
    pub if_: Option<Value>,
    #[serde(default)]
    pub props: Props,
}

/// #VAPP-284: one of a macro's alternative roots (`MacroDef.alternatives`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroAlternative {
    #[serde(rename = "$if")]
    pub if_: Value,
    pub root: MacroTemplate,
}

/// A `$on` entry: the author's macro event (the part's `$context` merged
/// in), or #VAPP-284 `{event, context}` = that event with its OWN context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MacroRoute {
    Event(String),
    Routed {
        event: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<Props>,
    },
}

impl MacroRoute {
    /// The author's macro event this part event routes to.
    pub fn event(&self) -> &str {
        match self {
            MacroRoute::Event(e) => e,
            MacroRoute::Routed { event, .. } => event,
        }
    }
}

/// An extension catalog: its own id, extending the core, with components and
/// the macro templates for its macro components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionDef {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// VAPP-161: `MAJOR.MINOR.PATCH`, what a package's `extensions` entry is
    /// checked against (caret); `None` = satisfies any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub extends: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enums: Option<IndexMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defs: Option<IndexMap<String, DefSchema>>,
    #[serde(default)]
    pub components: IndexMap<String, ComponentDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macros: Option<IndexMap<String, MacroDef>>,
}
