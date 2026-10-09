//! VAPP-103: the input LIMITS (`catalog/limits.json`, generated as
//! `LIMIT_NAMES`/`LIMIT_VALUES`): an agent's input past one is refused with
//! an issue, never worked on. Mirrors `src/limits.ts`; the issue messages
//! are byte-identical in every core.

use crate::generated::catalog as g;

/// A limit by name (`maxDepth`, `maxMessageBytes`, …).
pub fn limit(name: &str) -> Option<usize> {
    let i = g::LIMIT_NAMES.iter().position(|n| *n == name)?;
    g::LIMIT_VALUES.get(i)?.parse().ok()
}

/// Nodes the reducer places in one surface's tree (template nodes included).
pub const MAX_COMPONENTS: usize = 20_000;
/// Component nesting levels (the root = 1): every reduced tree stays
/// inside serde_json's 128-level recursion limit (2 JSON levels per node).
/// The stack is never the bound: the recursive passes grow it on the heap
/// (`crate::roomy`/`crate::deep`; `tests/robustness.rs` lays a 48-deep
/// Card surface out on a 128 KB thread).
pub const MAX_DEPTH: usize = 48;
/// One server message as UTF-8 JSON.
pub const MAX_MESSAGE_BYTES: usize = 4_194_304;
/// Template items one surface instantiates.
pub const MAX_TEMPLATE_ITEMS: usize = 10_000;
/// A data pointer's length in UTF-8 bytes.
pub const MAX_POINTER_BYTES: usize = 1024;
/// A data pointer's token count.
pub const MAX_POINTER_SEGMENTS: usize = 64;

/// An id placed a second time (two parents, or one parent twice).
pub const USED_TWICE_ISSUE: &str = "id used twice; only its first place renders";

pub fn depth_issue() -> String {
    format!("nesting deeper than {MAX_DEPTH} levels")
}

pub fn components_issue() -> String {
    format!("surface: more than {MAX_COMPONENTS} components; the rest is dropped")
}

pub fn template_items_issue() -> String {
    format!("template: more than {MAX_TEMPLATE_ITEMS} items; the rest is not rendered")
}

pub fn message_bytes_issue() -> String {
    format!("message larger than {MAX_MESSAGE_BYTES} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_are_limits_json() {
        for (name, v) in [
            ("maxComponents", MAX_COMPONENTS),
            ("maxDepth", MAX_DEPTH),
            ("maxMessageBytes", MAX_MESSAGE_BYTES),
            ("maxTemplateItems", MAX_TEMPLATE_ITEMS),
            ("maxPointerBytes", MAX_POINTER_BYTES),
            ("maxPointerSegments", MAX_POINTER_SEGMENTS),
        ] {
            assert_eq!(limit(name), Some(v), "{name}");
        }
        assert_eq!(g::LIMIT_NAMES.len(), 6);
    }
}
