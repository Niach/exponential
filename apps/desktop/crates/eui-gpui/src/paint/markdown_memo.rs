//! #VAPP-258: the laid-out Markdown memo. A leaf painted again with the same
//! text, available width, theme + mode, text style, props, host options and
//! host-hook answers (the host's `markdown_hooks_version`) reuses the hosted,
//! chip-sized blocks and their layout instead of laying the document out
//! again every frame. A host without a version is never memoized.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use super::markdown::{Block, MdLayout, MdStyles, RunColors, TaskBox};

/// How many laid-out documents the memo keeps (over it, it starts over).
const CAP: usize = 256;

thread_local! {
    static MEMO: RefCell<HashMap<u64, (Rc<Vec<Block>>, Rc<MdLayout>)>> = RefCell::new(HashMap::new());
}

/// Everything a layout depends on. `theme` = the resolved theme's identity.
pub struct LayoutKey<'a> {
    pub text: &'a str,
    pub width: f32,
    pub theme: usize,
    pub mode: &'a str,
    pub text_style: (f32, f32, u16, Option<&'a str>),
    pub props: &'a serde_json::Map<String, serde_json::Value>,
    pub options: &'a crate::host::MarkdownOptions,
    pub viewport: f32,
    pub hooks: u64,
}

fn hash_options(o: &crate::host::MarkdownOptions, h: &mut impl Hasher) {
    (o.hard_breaks, o.chat, o.selectable, o.streaming).hash(h);
    match o.image_max_height {
        crate::host::ImageCap::Catalog => 0u32.hash(h),
        crate::host::ImageCap::None => 1u32.hash(h),
        crate::host::ImageCap::Height(px) => (2u32, px.to_bits()).hash(h),
    }
}

/// The key's hash (the `text` prop is the text, hashed once).
pub fn key(k: &LayoutKey) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    k.text.hash(&mut h);
    k.width.to_bits().hash(&mut h);
    k.theme.hash(&mut h);
    k.mode.hash(&mut h);
    k.text_style.0.to_bits().hash(&mut h);
    k.text_style.1.to_bits().hash(&mut h);
    k.text_style.2.hash(&mut h);
    k.text_style.3.hash(&mut h);
    for (name, value) in k.props.iter().filter(|(name, _)| name.as_str() != "text") {
        name.hash(&mut h);
        value.to_string().hash(&mut h);
    }
    hash_options(k.options, &mut h);
    k.viewport.to_bits().hash(&mut h);
    k.hooks.hash(&mut h);
    h.finish()
}

/// The memoized blocks + layout under `key`, else `build`'s (remembered).
pub fn layout(key: Option<u64>, build: impl FnOnce() -> (Rc<Vec<Block>>, MdLayout)) -> (Rc<Vec<Block>>, Rc<MdLayout>) {
    let Some(key) = key else {
        let (blocks, lay) = build();
        return (blocks, Rc::new(lay));
    };
    if let Some(hit) = MEMO.with(|m| m.borrow().get(&key).cloned()) {
        return hit;
    }
    let (blocks, lay) = build();
    let entry = (blocks, Rc::new(lay));
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CAP {
            m.clear();
        }
        m.insert(key, entry.clone());
    });
    entry
}

/// What a leaf's look resolves from the theme every paint (the part
/// recipes): the same theme, mode, text style, props, options, viewport,
/// ink and host = the same look.
pub struct MdLook {
    pub styles: MdStyles,
    pub colors: (RunColors, gpui::Hsla, gpui::Hsla, Option<gpui::Hsla>),
    pub selection_bg: gpui::Hsla,
    pub task_box: TaskBox,
    pub chip_box: super::PaintStyle,
    pub chip_label: Option<gpui::Hsla>,
    /// Each chip kind's `Markdown/chip*` colours.
    pub chip_looks: HashMap<String, super::markdown::ChipLook>,
    /// `Markdown/codeBlock` border and `Markdown/headerCell` fill.
    pub code_block_border: Option<gpui::Hsla>,
    pub header_bg: Option<gpui::Hsla>,
}

thread_local! {
    static LOOKS: RefCell<HashMap<u64, Rc<MdLook>>> = RefCell::new(HashMap::new());
}

/// The look's key: [`key`]'s inputs but the text, width and hooks, plus
/// the ink and the host's identity (the task box paints its icons).
pub fn look_key(k: &LayoutKey, ink: gpui::Hsla, host: usize) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    k.theme.hash(&mut h);
    k.mode.hash(&mut h);
    k.text_style.0.to_bits().hash(&mut h);
    k.text_style.1.to_bits().hash(&mut h);
    k.text_style.2.hash(&mut h);
    k.text_style.3.hash(&mut h);
    for (name, value) in k.props.iter().filter(|(name, _)| name.as_str() != "text") {
        name.hash(&mut h);
        value.to_string().hash(&mut h);
    }
    hash_options(k.options, &mut h);
    k.viewport.to_bits().hash(&mut h);
    [ink.h, ink.s, ink.l, ink.a].map(f32::to_bits).hash(&mut h);
    host.hash(&mut h);
    h.finish()
}

/// The memoized look under `key`, else `build`'s (remembered).
pub fn look(key: u64, build: impl FnOnce() -> MdLook) -> Rc<MdLook> {
    if let Some(hit) = LOOKS.with(|m| m.borrow().get(&key).cloned()) {
        return hit;
    }
    let look = Rc::new(build());
    LOOKS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CAP {
            m.clear();
        }
        m.insert(key, look.clone());
    });
    look
}

/// Drop `key` (a picture just decoded at a size the memo did not know).
pub fn forget(key: Option<u64>) {
    if let Some(key) = key {
        MEMO.with(|m| m.borrow_mut().remove(&key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_reuses_the_layout_and_a_miss_builds() {
        let built = std::cell::Cell::new(0);
        let build = || {
            built.set(built.get() + 1);
            (Rc::new(Vec::new()), MdLayout { blocks: Vec::new(), height: 12.0 })
        };
        let props = serde_json::Map::new();
        let options = crate::host::MarkdownOptions::default();
        let k = |text: &str, width: f32, hooks: u64| key(&LayoutKey { text, width, theme: 1, mode: "dark", text_style: (14.0, 20.0, 400, None), props: &props, options: &options, viewport: 900.0, hooks });
        let a = k("memo test a", 300.0, 1);
        assert_eq!(layout(Some(a), build).1.height, 12.0);
        layout(Some(a), build);
        assert_eq!(built.get(), 1, "the same key reuses");
        layout(Some(k("memo test a", 301.0, 1)), build);
        layout(Some(k("memo test a", 300.0, 2)), build);
        assert_eq!(built.get(), 3, "a width or a hooks change builds again");
        layout(None, build);
        layout(None, build);
        assert_eq!(built.get(), 5, "no version, no memo");
        forget(Some(a));
        layout(Some(a), build);
        assert_eq!(built.get(), 6);
    }
}
