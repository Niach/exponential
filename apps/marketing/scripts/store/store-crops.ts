/* Pop-out crop rects (EXP-566): the slice of a raw capture that gets lifted out
   of the bezel and floated over it as its own card.

   Rects are NORMALIZED 0..1 against the raw's own pixel size, so they survive a
   device/canvas change. Resolution ladder, most specific first:

     1. a sidecar `pop-<shot>.json` next to the raw   (hand-tuned, wins)
     2. the HAND_RECTS entry for `<shot>:<form>`      (measured, below)
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

/* ── MEASURED on 2026-08-24 against the committed raws — iPhone 17 Pro Max
   (1320×2868), iPad Pro 13-inch M5 (2064×2752) and the Android phone
   (1080×2400) — by reading each capture, locating the element the slide's
   headline is about and converting its bounding box, with a ~1–2% margin.
   Re-measure whenever `seed:screenshots` or a client's layout changes.

   Two things the raws taught us: the iPad shots are NOT split views (every
   surface is one full-width column, so the tablet rects are full-bleed), and
   a lone list row on a phone is ~9:1, so several rects deliberately take in
   the neighbouring group header or sibling row to land in the 2:1–6:1 band the
   compositor's cards want. Rows are always framed whole — never half a row. ── */
export const HAND_RECTS: Record<string, Rect> = {
  // The richest issue row (APP-6: priority, id, label dot, due date, avatar),
  // under its "Todo" group header. Tablet takes the whole Todo group.
  "board:ios-phone": { x: 0.024, y: 0.359, w: 0.953, h: 0.081 },
  "board:android-phone": { x: 0.024, y: 0.33, w: 0.951, h: 0.0735 },
  "board:ios-tablet": { x: 0.007, y: 0.226, w: 0.986, h: 0.157 },

  // The Claude Code / Codex / pi agent picker, plus the Model + Effort rows
  // right under it — the picker alone is a ~12:1 sliver.
  "start-coding:ios-phone": { x: 0.0315, y: 0.578, w: 0.937, h: 0.177 },
  "start-coding:android-phone": { x: 0.0289, y: 0.5165, w: 0.942, h: 0.156 },
  "start-coding:ios-tablet": { x: 0.228, y: 0.606, w: 0.543, h: 0.133 },

  // The agent's unanswered question card with its two numbered options.
  // The tablet rect also takes the prose question that sets it up.
  "steering:ios-phone": { x: 0.0174, y: 0.609, w: 0.965, h: 0.209 },
  "steering:android-phone": { x: 0.0289, y: 0.559, w: 0.951, h: 0.212 },
  "steering:ios-tablet": { x: 0.007, y: 0.7205, w: 0.986, h: 0.161 },

  // The floating Merge action bar (dismiss / Merge / open on GitHub), which
  // sits over the diff at the bottom of the review screen.
  "review:ios-phone": { x: 0.195, y: 0.9015, w: 0.603, h: 0.0585 },
  "review:android-phone": { x: 0.178, y: 0.9025, w: 0.643, h: 0.0675 },
  "review:ios-tablet": { x: 0.2725, y: 0.906, w: 0.46, h: 0.088 },

  // The "Update dependencies" action row: title, description, its
  // "1 automation" sub-line and the run button. Tablet takes two such rows.
  "actions:ios-phone": { x: 0.0206, y: 0.314, w: 0.958, h: 0.107 },
  "actions:android-phone": { x: 0.0267, y: 0.258, w: 0.9455, h: 0.095 },
  "actions:ios-tablet": { x: 0.008, y: 0.1875, w: 0.985, h: 0.1235 },

  // The top unread notification (APP-6 assigned, bold + unread dot).
  // Tablet needs three rows to stay out of sliver territory.
  "inbox:ios-phone": { x: 0.0217, y: 0.189, w: 0.955, h: 0.0735 },
  "inbox:android-phone": { x: 0.0222, y: 0.1465, w: 0.955, h: 0.0705 },
  "inbox:ios-tablet": { x: 0.007, y: 0.1095, w: 0.986, h: 0.151 },

  // The top open ticket — reporter name and their message. The mobile support
  // shots are the thread LIST, so the preview line is the reporter's message.
  "support:ios-phone": { x: 0.0217, y: 0.188, w: 0.955, h: 0.0935 },
  "support:android-phone": { x: 0.02, y: 0.1345, w: 0.955, h: 0.072 },
  "support:ios-tablet": { x: 0.007, y: 0.1095, w: 0.986, h: 0.187 },

  // The markdown checklist and the live green "Coding now" row below it.
  "issue-detail:ios-phone": { x: 0.042, y: 0.36, w: 0.923, h: 0.198 },
  "issue-detail:android-phone": { x: 0.032, y: 0.324, w: 0.928, h: 0.2145 },
  "issue-detail:ios-tablet": { x: 0.007, y: 0.165, w: 0.986, h: 0.148 },
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
