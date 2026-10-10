//! The shapes shared by every module: the catalog source (`core.catalog.json`),
//! a normalized surface node, the nested authoring form, the A2UI wire form
//! and an extension catalog. Mirrors `packages/exponential-ui/src/types.ts`.
//!
//! Objects are `serde_json::Map` (sorted keys); where ORDER is part of the
//! contract (slots, catalog definitions, macro templates) an `IndexMap` keeps
//! the source order.

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
    #[serde(flatten)]
    pub rest: Props,
}

/// One reducer complaint, by node id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReduceIssue {
    pub id: String,
    pub message: String,
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
    Node(Box<MacroTemplate>),
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
    pub each: Option<String>,
    #[serde(default, rename = "$as", skip_serializing_if = "Option::is_none")]
    pub as_: Option<String>,
    #[serde(default, rename = "$on", skip_serializing_if = "Option::is_none")]
    pub on: Option<IndexMap<String, String>>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroDef {
    #[serde(rename = "recipeProps", default)]
    pub recipe_props: Vec<String>,
    pub root: MacroTemplate,
}

/// An extension catalog: its own id, extending the core, with components and
/// the macro templates for its macro components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionDef {
    pub id: String,
    #[serde(default)]
    pub name: String,
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
