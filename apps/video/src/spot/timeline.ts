// spot/timeline.ts — the 36s LaunchSpot cut (storyboard-launch-spot.md).
// Every frame value is COMPOSITION-GLOBAL. The spot never imports the ships
// film's timeline — same surfaces/fixtures, its own clock.
//
// Text plan (2026-07-13 review, r2): TWO standalone cards (C1 hook · C3
// "Shipped."), TWO punches on the live bookends, and TWO in-beat captions
// (steer during the phone PiP, release while the wave lanes pop).

import type { CamKey, CursorKey } from "../ships/rig"
import { WIN } from "../ships/theme"
import { ORCH_SESSION, type SessionEvent } from "../ships/fixtures"
import { DETAIL_ANCHORS } from "../ships/surfaces/detail"
import type { FlowGraphSchedule } from "../ships/surfaces/flowgraph"
import type { ReleaseProgressStep } from "../ships/surfaces/releases"

// ── Segment boundaries (storyboard §1) ────────────────────────────────────────
export const SEG = {
  liveA: 0, // café chaos
  liveB: 120, // typing, world goes quiet (no text — the audio duck carries it)
  uiFadeIn: 219, // footage → UI crossfade window (9f; footage ends f228)
  u1: 228, // ISSUE PICK — board list → click → detail → Start coding click
  card1: 345, // "Your issue tracker. Writing code."
  u2: 393, // dock run + phone PiP (the terminal's first appearance)
  u3: 558, // HARD CUT → releases list → drill → waves (no card in between)
  card3: 750, // "Shipped." — standalone
  liveC: 798, // coffee payoff
  filmEnd: 990,
  outroEnd: 1080,
} as const

export const SHELL_SWITCH_AT = SEG.u3 // hero → release shell, on the hard cut
export const UI_UNMOUNT_AT = 762 // card3 is opaque from f758 — never flash UI behind its lift
export const HERO_DOCK_OPEN_AT = 380 // collapsed strip → expanded dock, under card1

// ── U1 anchors: the Start-coding button (pane-local anchor → window-local) ────
export const START_CODING = {
  x: WIN.rail + WIN.sidebar + DETAIL_ANCHORS.startCoding.x,
  y: WIN.topBar + WIN.dockTabs + DETAIL_ANCHORS.startCoding.y,
}
const BOARD_ROW_142 = { x: 174, y: 206 } // sidebar board, Todo group row 2 (ships estimate)

// ── Camera ────────────────────────────────────────────────────────────────────
export const SPOT_CAMERA_KEYS: CamKey[] = [
  // U1a — the board: window-left pinned (x = 960/s), sidebar list fills the left
  { f: SEG.uiFadeIn, s: 1.6, x: 600, y: 375 },
  { f: 254, s: 1.6, x: 600, y: 375 },
  // U1b — the detail opens: title + description + properties
  { f: 276, s: 1.6, x: 910, y: 398 },
  { f: 292, s: 1.6, x: 910, y: 398 },
  // U1c — push onto the Start-coding button (right+top window edges pinned at 2.0)
  { f: 316, s: 2.0, x: 1088, y: 270 },
  // cut under card1 → U2: bottom pinned (y = 980 − 540/s); x = 24 + 952/s keeps
  // the terminal line-starts exactly at the left frame edge (the ships S6 rule —
  // never crop the payoff line). Static: the phone is the motion.
  { f: 368, s: 2.0, x: 1088, y: 270 },
  { f: 369, s: 1.8, x: 553, y: 680 },
  { f: 557, s: 1.8, x: 553, y: 680 },
  // HARD CUT → U3: 1.26 with BOTH edges pinned (x = 960/s) — release sidebar
  // (header + progress bar) left, full graph incl. the "PR #219" chip (fixed at
  // wl ≈1338–1509) right, multiplying dock tabs below. Drift vertical-only.
  { f: SEG.u3, s: 1.26, x: 762, y: 480 },
  { f: SEG.card3, s: 1.26, x: 762, y: 472, ease: "linear" },
] // holds the last key under card3; the UI unmounts at UI_UNMOUNT_AT

// ── U1 cursor choreography (window-local; the spot's only cursor) ─────────────
export const CURSOR_U1_KEYS: CursorKey[] = [
  { f: 230, x: 560, y: 330 },
  { f: 248, x: BOARD_ROW_142.x, y: BOARD_ROW_142.y },
  { f: 262, x: BOARD_ROW_142.x, y: BOARD_ROW_142.y },
  { f: 298, x: START_CODING.x, y: START_CODING.y },
  { f: 324, x: START_CODING.x, y: START_CODING.y },
  { f: 344, x: START_CODING.x + 110, y: START_CODING.y + 70 },
]
export const CURSOR_U1 = { from: 230, to: 344, clicks: [252, 318] } as const

// U1 beat frames
export const ROW_HOVER = { from: 238, to: 252 } as const
export const ROW_CLICK_AT = 252 // detail slides in right after
export const DETAIL_IN_AT = 254
export const START_HOVER = { at: 306, out: 320 } as const
export const START_CLICK_AT = 318
export const STATUS_FLIP_AT = 321 // todo → in_progress (board row + detail)
export const CODING_NOW_AT = 330 // the pill pops as card1 approaches

// ── U2 — the hero session (dock opens under card1; events 0–3 pre-rendered) ──
export const HERO_FEED_SCHEDULE: number[] = [300, 310, 320, 330, 408, 438, 462, 480, 510, 528]
export const DOCK_INPUT_GLOW_AT = 455
export const SESSION_EXIT_AT = 545
export const PR_CHIP_AT = 530

// Phone PiP (screen-space, 1.24× the film's size): slide-in f420, catch-up
// cascade for the six items already done, then live mirror of push/pr_open/done.
export const PHONE_IN_AT = 420
export const PHONE_SCALE = 1.24
export const PHONE_POS = { x: 1425, y: 130 }
export const PHONE_FEED_SCHEDULE: number[] = [424, 429, 434, 439, 444, 449, 482, 512, 530]
export const PHONE_PULSE_AT = 470
export const PHONE_OUT = { from: 540, to: 552 }

// ── U3 — hard cut at f558: releases LIST first, drill f592, waves f602–732 ────
export const ORCH_EVENTS: SessionEvent[] = [
  ...ORCH_SESSION,
  { kind: "flash", text: "Merged PR #219 — release v0.12 shipped" },
]
// e0/e1 land during the list phase (f558–592) so the dock is alive right after the cut
export const ORCH_SCHEDULE: number[] = [566, 580, 624, 633, 642, 648, 668, 677, 686, 690, 694, 722]

export const RELEASE_DRILL_AT = 592

export const FLOW_SCHEDULE: FlowGraphSchedule = {
  drawMain: 602,
  drawRel: 602,
  wave1At: [610, 613, 616],
  wave1MergeAt: [626, 635, 644],
  wave2At: [650, 653],
  wave2MergeAt: [670, 679],
  prChipAt: 692,
  prMergedAt: 722,
}

export const PROGRESS_STEPS: ReleaseProgressStep[] = [
  { at: 628, from: 3, to: 4, dur: 6 },
  { at: 637, from: 4, to: 5, dur: 6 },
  { at: 646, from: 5, to: 6, dur: 6 },
  { at: 672, from: 6, to: 7, dur: 6 },
  { at: 712, from: 7, to: 8, dur: 12 },
]

export const STATUS_FLIPS: Record<string, number> = {
  "EXP-139": 626,
  "EXP-141": 635,
  "EXP-143": 644,
  "EXP-144": 670,
  "EXP-145": 679,
}

export const RELEASE_PR_CHIP = { at: 694, mergedAt: 722 } as const
export const SHIPPED_AT = 728
export const CASCADE_DONE_AT = 732
// f732–750: the shipped pill holds the frame, then card3 takes over

// ── Copy + overlay schedule ───────────────────────────────────────────────────
export const SPOT_COPY = {
  liveA: "9:04 AM. One fix to ship.",
  hook1: "Your issue tracker.",
  hook2: "Writing code.",
  steer: "Steer from your phone.",
  release: "Or ship a whole release.",
  card3: "Shipped.",
  liveC: "Before the coffee.",
  tagline: "Shipping faster than your barista.",
} as const

export const OVERLAYS = {
  liveA: { in: 24, out: 96 },
  card1: { in: SEG.card1, out: SEG.u2 },
  steer: { in: 444, out: 538 }, // while the phone PiP is on screen
  release: { in: 606, out: 700 }, // while the wave lanes pop and merge
  card3: { in: SEG.card3, out: SEG.liveC + 8 }, // bg lifts OVER the cup footage
  liveC: { in: 828, out: 950 },
  wordmark: { in: 240, out: 745 },
} as const

// ── Audio anchors (see audio.tsx for the measured track map) ──────────────────
export const MUSIC_IN = SEG.liveB // music enters as the café goes quiet
export const KEYS_BED = { from: SEG.u1, to: SEG.card3 } as const // UI act → the Shipped card
