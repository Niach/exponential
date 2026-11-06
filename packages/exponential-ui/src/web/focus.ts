// EXP-1556: an overlay's open focus without a ring unless the keyboard
// opened it. The focus lands where the contract puts it (a part marked
// `accessibility.autoFocus`, an AlertDialog's cancel, else the first
// tabbable, as Radix picks it); after a pointer press (or no interaction
// yet) the target carries `data-xui-quiet-focus`, which the focus
// selectors skip, until the next key anywhere (a Tab back to it rings). Desktop, iOS and Android
// draw no ring on an overlay opened by a press either.

export const QUIET_FOCUS_ATTR = `data-xui-quiet-focus`

const FOCUSABLE = `button,input,textarea,select,a[href],[tabindex]`
const watched = new WeakSet<Document>()
const quiet = new Set<HTMLElement>()
let keyboard = false

function watchModality(doc: Document): void {
  if (watched.has(doc)) return
  watched.add(doc)
  doc.addEventListener(
    `keydown`,
    () => {
      keyboard = true
      for (const el of quiet) el.removeAttribute(QUIET_FOCUS_ATTR)
      quiet.clear()
    },
    true
  )
  doc.addEventListener(`pointerdown`, () => (keyboard = false), true)
}

/** Where an opening overlay's focus goes: the marked part, else the
 *  AlertDialog's cancel, else the first tabbable (links excluded, as Radix). */
export function openFocusTarget(content: HTMLElement): HTMLElement | null {
  const marked = content.querySelector<HTMLElement>(`[data-xui-autofocus]`) ?? content.querySelector<HTMLElement>(`[data-xui-part="AlertDialog/cancel"]`)
  if (marked) return marked.matches(FOCUSABLE) ? marked : marked.querySelector<HTMLElement>(FOCUSABLE)
  for (const el of content.querySelectorAll<HTMLElement>(`*`)) {
    if (el.tabIndex < 0 || el.tagName === `A` || (el as HTMLButtonElement).disabled) continue
    if (el.tagName === `INPUT` && (el as HTMLInputElement).type === `hidden`) continue
    return el
  }
  return null
}

/** An overlay's open-autofocus handler, shared by every web renderer: it
 *  takes the focus over (`preventDefault`) and moves it to
 *  `openFocusTarget`, quietly unless the keyboard opened the overlay. */
export function overlayOpenFocus(e: Event): void {
  const content = e.currentTarget as HTMLElement | null
  if (!content) return
  watchModality(content.ownerDocument)
  const target = openFocusTarget(content)
  if (!target) return
  e.preventDefault()
  if (!keyboard) {
    target.setAttribute(QUIET_FOCUS_ATTR, ``)
    quiet.add(target)
  }
  target.focus()
}

/** #VAPP-277: the focusable element AROUND a tooltip's anchor (a pressable
 *  Row, a button), inside the same surface, when nothing inside the anchor
 *  takes focus itself. Such a tooltip adds no tab stop
 *  of its own: its anchor is not focusable and the host's keyboard focus
 *  opens it (one tab stop per rail row). `null` = the anchor is its own stop. */
export function tooltipFocusHost(anchor: HTMLElement): HTMLElement | null {
  // A Tooltip around a Button keeps the Button's own focus.
  if (anchor.querySelector(`button,input,textarea,select,a[href],[tabindex]:not([tabindex="-1"])`)) return null
  const host = anchor.parentElement?.closest<HTMLElement>(`button,a[href],[tabindex]:not([tabindex="-1"])`) ?? null
  return host && host.closest(`[data-xui-surface]`) === anchor.closest(`[data-xui-surface]`) ? host : null
}

/** #VAPP-277: a tooltip `side` as a physical side: `start` / `end` are the
 *  inline start / end (left / right in LTR, mirrored in RTL). */
export function tooltipPhysicalSide(side: string, direction: string | undefined): `top` | `right` | `bottom` | `left` {
  if (side === `end`) return direction === `rtl` ? `left` : `right`
  if (side === `start`) return direction === `rtl` ? `right` : `left`
  return side === `right` || side === `bottom` || side === `left` ? side : `top`
}
