/* Pop-out crop rects (EXP-566): the slice of a raw capture that gets lifted out
   of the bezel and floated over it as its own card.

   Rects are NORMALIZED 0..1 against the raw's own pixel size, so they survive a
   device/canvas change. Resolution ladder, most specific first:

     1. a sidecar `pop-<shot>.json` next to the raw   (hand-tuned, wins)
     2. the HAND_RECTS entry for `<shot>:<form>`      (first guesses below)
     3. nothing — the slide still renders, just without a pop-out

   Sidecars are either a bare rect (`{"x":…,"y":…,"w":…,"h":…}`) or a map keyed
   by form (`{"ios-phone":{…}}`). They are not committed; tune them by eye with
   `--debug-crops`, which writes the raw with every candidate rect stroked and
   labelled. */

import { existsSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import type { Form } from "./raw-store"

export type Rect = { x: number; y: number; w: number; h: number }

function valid(r: unknown): r is Rect {
  const c = r as Rect | null
  return (
    !!c &&
    [c.x, c.y, c.w, c.h].every((n) => typeof n === `number` && Number.isFinite(n)) &&
    c.w > 0 &&
    c.h > 0 &&
    c.x >= 0 &&
    c.y >= 0 &&
    c.x + c.w <= 1.0001 &&
    c.y + c.h <= 1.0001
  )
}

/* ── FIRST GUESSES — hand-tuned by eye against the seeded demo data, NOT
   measured. Expect to move all of them once real captures exist; that is what
   `--debug-crops` and the sidecars are for. Phone rects assume a single-column
   list; the iPad rects target the right-hand detail pane of the split view. ── */
export const HAND_RECTS: Record<string, Rect> = {
  // An issue row mid-list.
  "board:ios-phone": { x: 0.04, y: 0.42, w: 0.92, h: 0.095 },
  "board:android-phone": { x: 0.04, y: 0.40, w: 0.92, h: 0.085 },
  "board:ios-tablet": { x: 0.36, y: 0.36, w: 0.56, h: 0.08 },

  // The agent picker chips.
  "start-coding:ios-phone": { x: 0.05, y: 0.54, w: 0.9, h: 0.13 },
  "start-coding:android-phone": { x: 0.05, y: 0.5, w: 0.9, h: 0.12 },
  "start-coding:ios-tablet": { x: 0.32, y: 0.44, w: 0.42, h: 0.12 },

  // The unanswered question bubble in the lower third.
  "steering:ios-phone": { x: 0.06, y: 0.62, w: 0.88, h: 0.155 },
  "steering:android-phone": { x: 0.06, y: 0.6, w: 0.88, h: 0.14 },
  "steering:ios-tablet": { x: 0.38, y: 0.56, w: 0.54, h: 0.14 },

  // A diff hunk.
  "review:ios-phone": { x: 0.04, y: 0.46, w: 0.92, h: 0.175 },
  "review:android-phone": { x: 0.04, y: 0.44, w: 0.92, h: 0.16 },
  "review:ios-tablet": { x: 0.36, y: 0.4, w: 0.58, h: 0.16 },

  // An action row.
  "actions:ios-phone": { x: 0.04, y: 0.3, w: 0.92, h: 0.105 },
  "actions:android-phone": { x: 0.04, y: 0.29, w: 0.92, h: 0.095 },
  "actions:ios-tablet": { x: 0.36, y: 0.26, w: 0.56, h: 0.09 },

  // The top unread notification.
  "inbox:ios-phone": { x: 0.04, y: 0.2, w: 0.92, h: 0.105 },
  "inbox:android-phone": { x: 0.04, y: 0.2, w: 0.92, h: 0.095 },
  "inbox:ios-tablet": { x: 0.06, y: 0.2, w: 0.36, h: 0.09 },

  // A message bubble in the thread.
  "support:ios-phone": { x: 0.06, y: 0.45, w: 0.88, h: 0.135 },
  "support:android-phone": { x: 0.06, y: 0.43, w: 0.88, h: 0.125 },
  "support:ios-tablet": { x: 0.38, y: 0.4, w: 0.54, h: 0.12 },

  // The "Coding now" row.
  "issue-detail:ios-phone": { x: 0.04, y: 0.34, w: 0.92, h: 0.115 },
  "issue-detail:android-phone": { x: 0.04, y: 0.33, w: 0.92, h: 0.105 },
  "issue-detail:ios-tablet": { x: 0.36, y: 0.3, w: 0.58, h: 0.1 },
}

function sidecarRect(rawPath: string, shot: string, form: Form): Rect | null {
  const file = join(dirname(rawPath), `pop-${shot}.json`)
  if (!existsSync(file)) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(readFileSync(file, `utf8`))
  } catch {
    console.warn(`pop-${shot}.json is not valid JSON — ignoring`)
    return null
  }
  const byForm = (parsed as Record<string, unknown> | null)?.[form]
  const candidate = valid(byForm) ? byForm : parsed
  if (!valid(candidate)) {
    console.warn(`pop-${shot}.json has no usable rect for ${form} — ignoring`)
    return null
  }
  return candidate
}

/** Sidecar → hand rect → none. Never throws: a missing rect just drops the pop-out. */
export function popRect(shot: string | null, form: Form, rawPath: string | null): Rect | null {
  if (!shot) return null
  if (rawPath) {
    const side = sidecarRect(rawPath, shot, form)
    if (side) return side
  }
  return HAND_RECTS[`${shot}:${form}`] ?? null
}

/** Every candidate for a shot, for the --debug-crops overlay. */
export function debugRects(shot: string, form: Form, rawPath: string): { label: string; rect: Rect }[] {
  const out: { label: string; rect: Rect }[] = []
  const side = sidecarRect(rawPath, shot, form)
  if (side) out.push({ label: `sidecar pop-${shot}.json`, rect: side })
  const hand = HAND_RECTS[`${shot}:${form}`]
  if (hand) out.push({ label: `hand ${shot}:${form}`, rect: hand })
  return out
}
