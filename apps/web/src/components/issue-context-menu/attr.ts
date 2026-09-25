// EXP-1074 — how ANY element opts into THE issue context menu.
//
// One host per team layout (`provider.tsx`) listens for the gesture on the
// document; an element joins by carrying the issue's id in this attribute —
// a board row, a sidebar row, a reviews row, an issue chip. No wrapper, no
// handler props, nothing for a memoized row to compare: `grep data-issue-menu`
// is the whole surface.

export const ISSUE_MENU_ATTR = `data-issue-menu`
/** The `?from=` origin "Open issue" carries (EXP-851: the detail keeps the
 *  list it was opened from beside it). */
export const ISSUE_MENU_FROM_ATTR = `data-issue-menu-from`

export interface IssueMenuProps {
  "data-issue-menu": string
  "data-issue-menu-from"?: string
}

export function issueMenuProps(issueId: string, from?: string): IssueMenuProps {
  return from
    ? { [ISSUE_MENU_ATTR]: issueId, [ISSUE_MENU_FROM_ATTR]: from }
    : { [ISSUE_MENU_ATTR]: issueId }
}

export interface IssueMenuHit {
  element: HTMLElement
  issueId: string
  from?: string
}

function hitOf(element: HTMLElement): IssueMenuHit {
  const from = element.getAttribute(ISSUE_MENU_FROM_ATTR)
  return {
    element,
    issueId: element.getAttribute(ISSUE_MENU_ATTR) ?? ``,
    ...(from ? { from } : {}),
  }
}

/** The opted-in element the event landed in, innermost first (a chip inside
 *  a row wins over the row). */
export function findIssueMenuElement(
  target: EventTarget | null
): IssueMenuHit | null {
  if (!(target instanceof Element)) return null
  const element = target.closest<HTMLElement>(`[${ISSUE_MENU_ATTR}]`)
  return element ? hitOf(element) : null
}

function insideRect(rect: DOMRect, x: number, y: number): boolean {
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

/** A point clipped away by a scrolling ancestor is not on the element: a
 *  row scrolled out of its list still reports a rect. */
function visibleAt(element: HTMLElement, x: number, y: number): boolean {
  for (
    let node = element.parentElement;
    node !== null;
    node = node.parentElement
  ) {
    const style = getComputedStyle(node)
    if (style.overflowX === `visible` && style.overflowY === `visible`) continue
    if (!insideRect(node.getBoundingClientRect(), x, y)) return false
  }
  return true
}

/** The opted-in element under a viewport point, by geometry. While a modal
 *  menu is up (or fading out) the body has `pointer-events: none`, so the
 *  browser hit-tests every event to `<html>` — this is how a right-click on
 *  another row still finds that row. Innermost = the smallest matching box. */
export function issueMenuElementAt(x: number, y: number): IssueMenuHit | null {
  let best: { element: HTMLElement; area: number } | null = null
  for (const element of document.querySelectorAll<HTMLElement>(
    `[${ISSUE_MENU_ATTR}]`
  )) {
    const rect = element.getBoundingClientRect()
    if (rect.width === 0 || rect.height === 0 || !insideRect(rect, x, y)) {
      continue
    }
    const area = rect.width * rect.height
    if ((best === null || area < best.area) && visibleAt(element, x, y)) {
      best = { element, area }
    }
  }
  return best ? hitOf(best.element) : null
}
