//! #VAPP-284 (packages/exponential-ui/docs/round-7-contract.md): the fold
//! keys of a pressable Box with an `on.toggle` and a boolean `expanded` (a
//! foldable Row's head and fold button). ArrowRight opens, ArrowLeft closes
//! (swapped in rtl); a key that would not change the state does nothing.
//! The TS reference is `src/fold.ts` `foldKey`; `Surface::event(index,
//! "key", {key})` applies it, so every painter only forwards the key.

/// True when `key` (a DOM key name) toggles a node that is `expanded` now.
pub fn fold_key(key: &str, expanded: bool, rtl: bool) -> bool {
    let (open, close) = if rtl { ("ArrowLeft", "ArrowRight") } else { ("ArrowRight", "ArrowLeft") };
    if expanded {
        key == close
    } else {
        key == open
    }
}

#[cfg(test)]
mod tests {
    use super::fold_key;

    #[test]
    fn arrows_open_and_close_and_swap_in_rtl() {
        assert!(fold_key("ArrowRight", false, false));
        assert!(!fold_key("ArrowRight", true, false));
        assert!(fold_key("ArrowLeft", true, false));
        assert!(!fold_key("ArrowLeft", false, false));
        assert!(fold_key("ArrowLeft", false, true));
        assert!(fold_key("ArrowRight", true, true));
        assert!(!fold_key("Enter", false, false));
    }
}
