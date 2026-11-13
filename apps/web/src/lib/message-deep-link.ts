import { deepLinkStep, type DeepLinkStep } from "@exp/domain-contract/waiting-cards"

// waiting-cards.json DEEP LINKS ×web: ONE pending deep link into a message
// (a Needs you row, a waiting chip, an inbox row, `?card=`, a plan link),
// held by the session view until it settles. This is the pure half: what
// the view measured this frame goes in (`DeepLinkProbe`), `deepLinkStep`
// picks the step and the host's effect for it runs. The React half
// (`hooks/use-deep-link.ts`) probes every frame while a link is pending.

/** After a `page` the older page is "in flight" until the loaded rows
 *  change (`mark`), or this long passes with nothing new. */
export const DEEP_LINK_PAGE_GRACE_MS = 2_000

/** Within this many px of the viewport's centre counts as centred. */
export const DEEP_LINK_CENTRE_SLACK = 2

/** What the view knows about the link's target this frame. */
export interface DeepLinkProbe {
  /** The target's row is in the view's loaded rows (mounted). */
  found: boolean
  /** That row has its real height (mounted, nonzero, not a placeholder). */
  laidOut: boolean
  /** Its centre sits at the viewport's centre, or the list can go no further. */
  centred: boolean
  /** The host's own "an older page is in flight". */
  paging: boolean
  /** Older rows can still page in. */
  more: boolean
  /** The loaded rows' generation (a count, a key): a change = a page landed. */
  mark: string | number
}

/** The pending link's own memory between frames. */
export interface DeepLinkCursor {
  takenAt: number
  /** The last `page`: the generation it was asked at and when. */
  paged: { mark: string | number; at: number } | null
}

export interface DeepLinkEffects {
  /** Page ONE older page in. */
  page: () => void
  /** Scroll the target to the viewport's centre. */
  scroll: () => void
  /** Ring it (THE highlight); the link settles. */
  ring: () => void
  /** Not there: the link settles, the view stays where it is. */
  missing: () => void
}

export function takeDeepLink(now: number): DeepLinkCursor {
  return { takenAt: now, paged: null }
}

/** One frame of a pending deep link: the step `deepLinkStep` picks, its
 *  effect run, the cursor for the next frame; `settled` = ring / missing. */
export function deepLinkTick(
  cursor: DeepLinkCursor,
  probe: DeepLinkProbe,
  now: number,
  effects: DeepLinkEffects
): { step: DeepLinkStep; cursor: DeepLinkCursor; settled: boolean } {
  const inFlight =
    cursor.paged !== null && cursor.paged.mark === probe.mark && now - cursor.paged.at < DEEP_LINK_PAGE_GRACE_MS
  const step = deepLinkStep({
    found: probe.found,
    laidOut: probe.laidOut,
    centred: probe.centred,
    paging: probe.paging || inFlight,
    more: probe.more,
    elapsedMs: now - cursor.takenAt,
  })
  let next = cursor
  if (step === `page`) {
    effects.page()
    next = { ...cursor, paged: { mark: probe.mark, at: now } }
  } else if (step === `scroll`) effects.scroll()
  else if (step === `ring`) effects.ring()
  else if (step === `missing`) effects.missing()
  return { step, cursor: next, settled: step === `ring` || step === `missing` }
}

/** Where the target sits against the scroll viewport: `delta` = how far to
 *  scroll (positive = down) to centre it; `centred` = within
 *  `DEEP_LINK_CENTRE_SLACK`, or the viewport cannot scroll further that way. */
export function centreOffset(
  target: { top: number; height: number },
  viewport: { top: number; height: number; scrollTop: number; scrollHeight: number }
): { delta: number; centred: boolean } {
  const delta = target.top + target.height / 2 - (viewport.top + viewport.height / 2)
  const maxTop = Math.max(0, viewport.scrollHeight - viewport.height)
  const centred =
    Math.abs(delta) <= DEEP_LINK_CENTRE_SLACK ||
    (delta < 0 && viewport.scrollTop <= 0) ||
    (delta > 0 && viewport.scrollTop >= maxTop - 1)
  return { delta, centred }
}
