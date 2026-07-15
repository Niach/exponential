// spot-vertical/timeline.ts — the 15s 9:16 Instagram cut (1080×1920 @30fps,
// 450 frames). Fully independent of src/spot/ — its own clock, camera, and
// copy, sharing only the ships/ surface library. Story: chaos hook → brand
// card → ONE UI beat (pick issue → Start coding → status flips) → "Shipped."
// → coffee payoff → brand card. The phone/release beats are 16:9-only.

import type { CamKey, CursorKey } from "../ships/rig"
import { WIN } from "../ships/theme"
import { DETAIL_ANCHORS } from "../ships/surfaces/detail"

// ── Segment boundaries ────────────────────────────────────────────────────────
export const SEGV = {
  liveA: 0, // café chaos (square take crops well to 9:16)
  card1: 70, // "Your issue tracker. Writing code." + brand lockup
  u1: 130, // UI beat — board pick → detail → Start coding click
  card3: 258, // "Shipped."
  liveC: 302, // coffee payoff (card3 lifts over it, like the 16:9 cut)
  outro: 378,
  end: 450,
} as const

export const UI_MOUNT_AT = 124 // under card1's opaque plateau
export const UI_UNMOUNT_AT = 268 // under card3's opaque plateau

// ── U1 anchors (window-local; same derivation as the 16:9 spot) ───────────────
export const START_CODING_V = {
  x: WIN.rail + WIN.sidebar + DETAIL_ANCHORS.startCoding.x,
  y: WIN.topBar + WIN.dockTabs + DETAIL_ANCHORS.startCoding.y,
}
const BOARD_ROW = { x: 174, y: 206 } // sidebar board, Todo group row 2

// ── Camera (PORTRAIT rig: focus lands at comp center 540,960) ─────────────────
// Visible window slice at scale s: 1080/s wide × 1920/s tall. Pinning rules:
// left edge x = 540/s, right edge x = WIN.w − 540/s, top edge y = 960/s.
export const CAMERA_V_KEYS: CamKey[] = [
  // board: rail+sidebar fill the frame, top-pinned (s=2.6 → 415×738 slice)
  { f: UI_MOUNT_AT, s: 2.6, x: 208, y: 370 },
  { f: 156, s: 2.6, x: 208, y: 370 },
  // detail opens: title + description column, top-pinned
  { f: 174, s: 2.2, x: 549, y: 436 },
  { f: 198, s: 2.2, x: 549, y: 436 },
  // push onto Start coding: right+top pinned (props column + the button)
  { f: 220, s: 2.6, x: WIN.w - 540 / 2.6, y: 960 / 2.6 },
]

// ── U1 cursor + beats ─────────────────────────────────────────────────────────
export const CURSOR_V_KEYS: CursorKey[] = [
  { f: 132, x: 420, y: 320 },
  { f: 148, x: BOARD_ROW.x, y: BOARD_ROW.y },
  { f: 160, x: BOARD_ROW.x, y: BOARD_ROW.y },
  { f: 216, x: START_CODING_V.x, y: START_CODING_V.y },
  { f: 232, x: START_CODING_V.x, y: START_CODING_V.y },
  { f: 248, x: START_CODING_V.x + 90, y: START_CODING_V.y + 60 },
]
export const CURSOR_V = { from: 132, to: 250, clicks: [154, 226] } as const

export const ROW_HOVER_V = { from: 140, to: 154 } as const
export const ROW_CLICK_V = 154
export const DETAIL_IN_V = 156
export const START_HOVER_V = { at: 216, out: 228 } as const
export const START_CLICK_V = 226
export const STATUS_FLIP_V = 229 // todo → in_progress in the props column
export const CODING_NOW_V = 236 // the pill pops as card3 approaches

// ── Copy + overlay schedule ───────────────────────────────────────────────────
export const COPY_V = {
  liveA: "9:04 AM. One fix to ship.",
  hook1: "Your issue tracker.",
  hook2: "Writing code.",
  card3: "Shipped.",
  liveC: "Before the coffee.",
  tagline: "Shipping faster than your barista.",
} as const

export const OVERLAYS_V = {
  liveA: { in: 14, out: 62 },
  card1: { in: SEGV.card1, out: SEGV.u1 + 2 },
  card3: { in: SEGV.card3, out: SEGV.liveC + 8 }, // lifts OVER the cup footage
  liveC: { in: 318, out: 368 },
} as const

// ── Audio anchors ─────────────────────────────────────────────────────────────
// music.mp3 (29.7s): ARRIVAL at ~22s in-track. The 15s cut starts the music at
// f0 trimmed 13.4s in, so the arrival lands at f≈258 — under the "Shipped."
// card — and the natural fade dies inside the outro.
export const MUSIC_TRIM_SEC = 13.4
export const KEYS_BED_V = { from: SEGV.u1, to: SEGV.card3 } as const
