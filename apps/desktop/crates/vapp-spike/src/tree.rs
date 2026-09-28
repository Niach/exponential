//! The node tree: JSON spec → flat pre-order list.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One node as authored in the fixture.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub style: Map<String, Value>,
    #[serde(default)]
    pub props: Map<String, Value>,
    #[serde(default)]
    pub children: Vec<NodeSpec>,
}

/// Catalog kinds. Containers are laid out by taffy from their children; leaves
/// are measured by the host (`Measure`) and painted as native controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Box,
    Card,
    Text,
    Button,
    TextField,
    Textarea,
    Toggle,
    Select,
    ListRow,
    Badge,
    Pill,
    Avatar,
    Image,
    Divider,
    Progress,
    Markdown,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "box" => Kind::Box,
            "card" => Kind::Card,
            "text" => Kind::Text,
            "button" => Kind::Button,
            "textfield" => Kind::TextField,
            "textarea" => Kind::Textarea,
            "toggle" => Kind::Toggle,
            "select" => Kind::Select,
            "listrow" => Kind::ListRow,
            "badge" => Kind::Badge,
            "pill" => Kind::Pill,
            "avatar" => Kind::Avatar,
            "image" => Kind::Image,
            "divider" => Kind::Divider,
            "progress" => Kind::Progress,
            "markdown" => Kind::Markdown,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Box => "box",
            Kind::Card => "card",
            Kind::Text => "text",
            Kind::Button => "button",
            Kind::TextField => "textfield",
            Kind::Textarea => "textarea",
            Kind::Toggle => "toggle",
            Kind::Select => "select",
            Kind::ListRow => "listrow",
            Kind::Badge => "badge",
            Kind::Pill => "pill",
            Kind::Avatar => "avatar",
            Kind::Image => "image",
            Kind::Divider => "divider",
            Kind::Progress => "progress",
            Kind::Markdown => "markdown",
        }
    }

    /// Containers never reach `Measure`; everything else is a measured leaf.
    pub fn is_container(&self) -> bool {
        matches!(self, Kind::Box | Kind::Card)
    }

    /// Leaves whose intrinsic size comes from text content.
    pub fn is_textual(&self) -> bool {
        matches!(self, Kind::Text | Kind::Markdown | Kind::Button | Kind::Pill | Kind::ListRow)
    }
}

/// A node after flattening: `index` is its pre-order position, which is also
/// its paint order and its accessibility order.
#[derive(Debug, Clone)]
pub struct FlatNode {
    pub index: u32,
    pub id: String,
    pub kind: Kind,
    pub style: Map<String, Value>,
    pub props: Map<String, Value>,
    pub parent: Option<u32>,
    pub depth: u32,
    pub children: Vec<u32>,
}

/// Pre-order flatten. Errors on an unknown kind.
pub fn flatten(root: &NodeSpec) -> Result<Vec<FlatNode>, String> {
    let mut out = Vec::new();
    fn walk(
        spec: &NodeSpec,
        parent: Option<u32>,
        depth: u32,
        out: &mut Vec<FlatNode>,
    ) -> Result<u32, String> {
        let kind = Kind::parse(&spec.kind)
            .ok_or_else(|| format!("node {}: unknown kind {:?}", spec.id, spec.kind))?;
        let index = out.len() as u32;
        out.push(FlatNode {
            index,
            id: spec.id.clone(),
            kind,
            style: spec.style.clone(),
            props: spec.props.clone(),
            parent,
            depth,
            children: Vec::new(),
        });
        let mut child_indices = Vec::with_capacity(spec.children.len());
        for child in &spec.children {
            child_indices.push(walk(child, Some(index), depth + 1, out)?);
        }
        out[index as usize].children = child_indices;
        Ok(index)
    }
    walk(root, None, 0, &mut out)?;
    Ok(out)
}
