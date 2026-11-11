// #EXP-1778 (fixtures/text-truncate.json): a Text `truncate: "middle"` on the
// web. CSS ellipsises only the end, so the line is cut in JS: the element's
// own font measures each character on a canvas (`middleTruncateWidth`, the
// rule gpui's `text_ellipsis_middle` applies), again whenever its box
// resizes. Framework-agnostic: React and Solid render what `onShown` gets.

import { middleTruncateWidth } from "../text-truncate"

let context: CanvasRenderingContext2D | null | undefined

function measurer(el: HTMLElement): ((ch: string) => number) | null {
  if (context === undefined) context = typeof document === `undefined` ? null : document.createElement(`canvas`).getContext(`2d`)
  if (!context) return null
  const style = getComputedStyle(el)
  context.font = style.font || `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
  const spacing = parseFloat(style.letterSpacing) || 0
  const ctx = context
  const cache = new Map<string, number>()
  return (ch) => {
    let w = cache.get(ch)
    if (w === undefined) cache.set(ch, (w = ctx.measureText(ch).width + spacing))
    return w
  }
}

/** The line `text` shows in `el`'s content box, cut in the middle. */
export function fitMiddle(el: HTMLElement, text: string): string {
  const measure = measurer(el)
  if (!measure) return text
  const style = getComputedStyle(el)
  const width = el.getBoundingClientRect().width - (parseFloat(style.paddingLeft) || 0) - (parseFloat(style.paddingRight) || 0)
  if (!(width > 0)) return text
  return middleTruncateWidth(text, width, measure)
}

/** Keeps `onShown` told of `el`'s middle-cut line as its box resizes (and
 *  once the fonts load); returns the stop. */
export function watchMiddleTruncate(el: HTMLElement, text: string, onShown: (shown: string) => void): () => void {
  const run = () => onShown(fitMiddle(el, text))
  run()
  const observer = typeof ResizeObserver === `undefined` ? null : new ResizeObserver(run)
  observer?.observe(el)
  let live = true
  void document.fonts?.ready.then(() => live && run())
  return () => {
    live = false
    observer?.disconnect()
  }
}
