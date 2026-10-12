import type { TypeaheadAnchor } from "@exp/ui"

// SLOP-28: the `#` search list hangs off the CARET, and a plain <textarea>
// has no `coordsAtPos`. The classic mirror: a hidden div with the field's
// box and text metrics holds the text up to the caret plus a marker span,
// and the marker's offset IS the caret, in the field's own coordinates. The
// field's rect and scroll turn that into the viewport rect the popover takes.

const MIRRORED = [
  `boxSizing`,
  `width`,
  `borderTopWidth`,
  `borderRightWidth`,
  `borderBottomWidth`,
  `borderLeftWidth`,
  `paddingTop`,
  `paddingRight`,
  `paddingBottom`,
  `paddingLeft`,
  `fontFamily`,
  `fontSize`,
  `fontWeight`,
  `fontStyle`,
  `letterSpacing`,
  `lineHeight`,
  `textTransform`,
  `wordSpacing`,
  `tabSize`,
] as const

/** The caret rect at `position` of `text` (the field's value, or the one
 *  about to be committed) in viewport coordinates. */
export function textareaCaretAnchor(
  field: HTMLTextAreaElement,
  text: string,
  position: number
): TypeaheadAnchor {
  const style = window.getComputedStyle(field)
  const mirror = document.createElement(`div`)
  for (const key of MIRRORED) mirror.style[key] = style[key]
  mirror.style.position = `absolute`
  mirror.style.visibility = `hidden`
  mirror.style.top = `0`
  mirror.style.left = `-9999px`
  mirror.style.whiteSpace = `pre-wrap`
  mirror.style.overflowWrap = `break-word`
  mirror.textContent = text.slice(0, position)
  const marker = document.createElement(`span`)
  marker.textContent = `\u200b`
  mirror.appendChild(marker)
  document.body.appendChild(mirror)
  const top = marker.offsetTop
  const left = marker.offsetLeft
  const height = marker.offsetHeight || parseFloat(style.lineHeight) || 16
  mirror.remove()

  const rect = field.getBoundingClientRect()
  const caretTop = rect.top + top - field.scrollTop
  return {
    top: caretTop,
    bottom: caretTop + height,
    left: rect.left + left - field.scrollLeft,
  }
}
