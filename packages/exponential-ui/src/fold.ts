// #VAPP-284 (docs/round-7-contract.md): the fold keys of a pressable Box with
// an `on.toggle` and a boolean `expanded` (a foldable Row's head and fold
// button): ArrowRight opens, ArrowLeft closes (swapped in rtl), and a key
// that would not change the state does nothing. The Rust core's
// `fold_key` (crate eui-core `fold`) is the same rule; every painter asks it.

/** True when `key` (a DOM key name) toggles a node that is `expanded` now. */
export function foldKey(key: string, expanded: boolean, rtl: boolean): boolean {
  const open = rtl ? `ArrowLeft` : `ArrowRight`
  const close = rtl ? `ArrowRight` : `ArrowLeft`
  return expanded ? key === close : key === open
}
