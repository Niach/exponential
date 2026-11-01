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
