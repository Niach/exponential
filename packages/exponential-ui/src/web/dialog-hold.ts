// EXP-1676: a Dialog HOLDS its bottom edge once it has opened, the rule the
// core lays out for the natives (eui-core `LayerPlacement::Centered`): it
// opens centred, then its first height `h0` pins the bottom edge at
// `50% + h0/2` of the overlay's box and content that grows (a refused
// save's inline errors) or shrinks moves the TOP edge only, so the action
// row never leaves the pointer. Capped 16px below the top (base-css.ts
// `.xui-Dialog-content[data-xui-hold]`).

export const DIALOG_HOLD_ATTR = `data-xui-hold`
export const DIALOG_HOLD_VAR = `--xui-dialog-h0`

/** Pin `el` (an open Dialog's content) at its first laid-out height; the
 *  returned cleanup stops waiting for one. */
export function holdDialogBottom(el: HTMLElement): () => void {
  const pin = (): boolean => {
    const h = el.offsetHeight
    if (h <= 0) return false
    el.style.setProperty(DIALOG_HOLD_VAR, `${h}px`)
    el.setAttribute(DIALOG_HOLD_ATTR, ``)
    return true
  }
  if (el.hasAttribute(DIALOG_HOLD_ATTR) || pin() || typeof ResizeObserver === `undefined`) return () => {}
  const observer = new ResizeObserver(() => {
    if (pin()) observer.disconnect()
  })
  observer.observe(el)
  return () => observer.disconnect()
}
