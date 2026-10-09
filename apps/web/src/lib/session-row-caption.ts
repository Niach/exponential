import type { LiveDotTone, SessionDotTone, SessionRowTone } from "@exp/ui"
import type { SessionDisplayState } from "@/lib/coding-session-display"

// EXP-1248: the BIG session row's caption, x4 — fixture
// `packages/domain-contract/fixtures/list-item.json` (desktop run_rows.rs,
// iOS SessionRowCaption.swift, Android SessionRowCaption.kt). The live
// states and their tones are `session-display.json`'s; this only words them
// for a list: "<State> · <device> · <age>".

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

/** The list's coarse age ladder: `now`, `5 min`, `21 h`, `2 d` (floored). */
export function listElapsed(ms: number): string {
  if (!(ms >= MINUTE)) return `now`
  if (ms < HOUR) return `${Math.floor(ms / MINUTE)} min`
  if (ms < DAY) return `${Math.floor(ms / HOUR)} h`
  return `${Math.floor(ms / DAY)} d`
}

type Stamp = Date | string | null | undefined

function stampMs(value: Stamp): number | null {
  if (!value) return null
  const ms = (typeof value === `string` ? new Date(value) : value).getTime()
  return Number.isNaN(ms) ? null : ms
}

const LIVE_WORD: Record<SessionDisplayState, string> = {
  working: `Building`,
  needs_input: `Needs input`,
  review: `Ready for review`,
  done: `Done`,
}

/** The live tones, `session-display.json` statusTone. */
const LIVE_TONE: Record<SessionDisplayState, SessionRowTone> = {
  working: `muted`,
  needs_input: `amber`,
  review: `emerald`,
  done: `sky`,
}

export interface SessionRowCaptionInput {
  /** `runHasEnded(session)`. */
  ended: boolean
  /** An offline host (`sessionIsPaused`). */
  paused: boolean
  /** `sessionDisplayState(session, prState)`; ignored once ended. */
  state: SessionDisplayState
  /** The resolved device label; null drops the segment. */
  device: string | null | undefined
  startedAt: Stamp
  updatedAt: Stamp
  endedAt: Stamp
  /** The usage wall's badge label (`blockedBadgeLabel`), or null. */
  blockedLabel?: string | null
  now: Date | number
}

export function sessionRowCaption(input: SessionRowCaptionInput): {
  text: string
  tone: SessionRowTone
} {
  const now = typeof input.now === `number` ? input.now : input.now.getTime()
  const since = (stamp: Stamp) => {
    const at = stampMs(stamp)
    return at === null ? null : listElapsed(now - at)
  }
  const join = (...parts: (string | null | undefined)[]) =>
    parts.filter((part): part is string => Boolean(part)).join(` · `)
  const device = input.device?.trim() || null

  if (input.ended) {
    const when = since(input.endedAt) ?? since(input.updatedAt)
    return { text: join(`Done`, device, when), tone: `muted` }
  }
  if (input.paused) return { text: join(`Paused`, device), tone: `muted` }
  if (input.blockedLabel) return { text: input.blockedLabel, tone: `amber` }
  const live =
    input.state === `working` || input.state === `needs_input`
      ? since(input.startedAt)
      : since(input.updatedAt)
  return {
    text: join(LIVE_WORD[input.state], device, live),
    tone: LIVE_TONE[input.state],
  }
}

/** EXP-887: the session tones as `LiveDot` tones (the work-tab strip's dot).
 *  The COLOURS live in the x4 `SESSION_DOT_CLASS` table; `lib/session-dot.test.ts`
 *  locks the two together. Moved here from `agent-session-row.tsx`. */
export const LIVE_DOT_TONE_BY_SESSION_TONE: Record<SessionDotTone, LiveDotTone> = {
  running: `live`,
  review: `live`,
  needs_input: `attention`,
  done: `done`,
  muted: `idle`,
}
