// closedloop/timeline.ts — scene boundaries, chapter markers (consumed by the
// marketing player UI), camera keyframes, whip-pan blur, dock-height clock,
// terminal feed schedule, cursor choreographies and the caption windows.
// Every frame value is COMPOSITION-GLOBAL. 780 frames @ 30fps (~26s), and the
// last frame matches the first (camera, FAB, cursor) for a seamless loop.

import { interpolate, spring } from "remotion"
import { CHAPTER_INFO } from "./chapters"
import { SETTLE, WIN } from "../ships/theme"
import type { CamKey, CursorKey } from "../ships/rig"
import { DETAIL_ANCHORS } from "../ships/surfaces/detail"
import { railIconCenter } from "../ships/surfaces/chrome"
import { SITE_ANCHORS } from "./surfaces/sitemock"
import { WIDGET_ANCHORS } from "./surfaces/widgetmock"
import { START_DIALOG_ANCHORS } from "./surfaces/startdialog"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const FPS = 30
export const DURATION_IN_FRAMES = 780

// ── Scenes ────────────────────────────────────────────────────────────────────
export const SCENE = {
  site: 0, // the acme.shop visitor + dead Pay-now button
  widget: 110, // the feedback panel: screenshot, annotation, report, send
  board: 255, // whip into the app — EXP-151 pops onto the board
  detail: 330, // issue detail + Start coding
  dialog: 420, // the unified Start-coding dialog
  dock: 495, // Claude session in the terminal dock
  diff: 600, // Changes tab — side-by-side diff
  merge: 675, // Reviews rail → two-stage merge → board regroups to Done
  email: 735, // back on acme.shop — the reporter hears back
  end: DURATION_IN_FRAMES,
} as const

// Chapter markers for the marketing player scrubber — the id/label/phrase
// metadata lives in chapters.ts (remotion-free, shared with the marketing
// rail); only the frame mapping is authored here.
export type Chapter = { id: string; label: string; frame: number }
const CHAPTER_FRAMES: Record<string, number> = {
  feedback: SCENE.site,
  issue: SCENE.board,
  code: SCENE.dialog,
  merge: SCENE.diff,
  shipped: SCENE.email,
}
export const CHAPTERS: Chapter[] = CHAPTER_INFO.map(({ id, label }) => {
  const frame = CHAPTER_FRAMES[id]
  if (frame === undefined) throw new Error(`chapter ${id} has no frame`)
  return { id, label, frame }
})

// ── Camera (single global key list; 1-frame gaps = hard cuts under whip blur) ─
export const CAMERA_KEYS: CamKey[] = [
  // S1 — full browser window, then a push toward the pay button
  { f: 0, s: 1.0, x: 784, y: 490 },
  { f: 22, s: 1.0, x: 784, y: 490 },
  { f: 46, s: 1.3, x: 829, y: 520 },
  { f: 78, s: 1.3, x: 829, y: 520 },
  // S2 — settle onto the FAB corner / widget panel zone (window edges pinned)
  { f: 98, s: 1.5, x: 928, y: 620 },
  { f: 254, s: 1.5, x: 928, y: 620 },
  // S3 — hard cut onto the app board (ships S3 framing)
  { f: 255, s: 1.55, x: 615, y: 395 },
  { f: 332, s: 1.55, x: 615, y: 395 },
  // S4 — fly to the detail header + properties
  { f: 356, s: 1.4, x: 800, y: 330 },
  { f: 408, s: 1.4, x: 800, y: 330 },
  // S5 — dialog centered
  { f: 420, s: 1.45, x: 784, y: 470 },
  { f: 500, s: 1.45, x: 784, y: 470 },
  // S6 — tilt down onto the dock (window bottom pinned)
  { f: 518, s: 1.5, x: 685, y: 686 },
  { f: 600, s: 1.5, x: 685, y: 686 },
  // S7 — up onto the diff, slow reading drift
  { f: 620, s: 1.5, x: 930, y: 400 },
  { f: 632, s: 1.5, x: 930, y: 400 },
  { f: 672, s: 1.5, x: 930, y: 455, ease: "linear" },
  // S8 — pan left to the rail + reviews sidebar
  { f: 676, s: 1.5, x: 930, y: 455 },
  { f: 688, s: 1.5, x: 630, y: 430 },
  { f: 732, s: 1.5, x: 630, y: 430 },
  // S9 — hard cut back to the site; settle exactly onto the f0 framing (loop)
  { f: 735, s: 1.06, x: 800, y: 500 },
  { f: 779, s: 1.0, x: 784, y: 490 },
]

// Whip-pan blur at both hard cuts.
export const whipBlurAt = (frame: number): number =>
  interpolate(frame, [251, 254, 256, 259], [0, 3, 3, 0], CLAMP) +
  interpolate(frame, [731, 734, 736, 739], [0, 3, 3, 0], CLAMP)

// ── Dock height clock (29 ↔ 240) ─────────────────────────────────────────────
export const DOCK_COLLAPSE_END = 614

export const dockHeightAt = (frame: number): number => {
  if (frame < SCENE.dock) return WIN.dockStrip
  if (frame < SCENE.diff) {
    const t = spring({ frame: frame - SCENE.dock, fps: 30, config: SETTLE })
    return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t
  }
  if (frame < DOCK_COLLAPSE_END)
    return interpolate(frame, [SCENE.diff, DOCK_COLLAPSE_END], [WIN.dockExpanded, WIN.dockStrip], CLAMP)
  return WIN.dockStrip
}

// ── Widget beat frames (S1/S2) ────────────────────────────────────────────────
export const SITE_BEATS = {
  payClick1: 30,
  payClick2: 54,
  fabHover: 86,
  fabClick: 98,
  panelAppear: 104,
  fabRest: 112,
  annotate: 122,
  titleClick: 148,
  titleType: 152,
  detailsClick: 176,
  detailsType: 180,
  sendHover: 210,
  sendClick: 216,
  sending: 216,
  success: 232,
} as const

// ── Board / detail / dialog beats (S3–S5) ─────────────────────────────────────
export const BOARD_BEATS = {
  cascade: 257,
  insert: 300, // EXP-151 pops into Todo
  rowHoverFrom: 312,
  rowClick: 326,
  tabPop: 330,
  detailStagger: 336,
  startHover: 388,
  startHoverOut: 413,
  startClick: 412,
} as const

export const DIALOG_BEATS = {
  appear: 420,
  checkPulse: 430,
  rowHover: { index: 1, at: 436, out: 452 },
  buttonHover: 466,
  starting: 472,
  collapse: 482,
  scrimOut: 482,
} as const

// ── Terminal feed (indices into fixtures.CL_SESSION) ──────────────────────────
export const FEED_SCHEDULE: number[] = [500, 511, 524, 537, 549, 558, 566, 574, 584, 590]
export const SESSION_TAB_POP = 497
export const SESSION_EXIT = 594
export const PR_AT = 592 // PR chip + board dot + rail badge
export const SPINNER_BASE = { sec: 41, tokensK: 7.4 } as const

// ── Coding / merge state frames ───────────────────────────────────────────────
export const CODING_START = 486 // EXP-151 → In Progress (behind the dialog collapse)
export const CODING_PILL = { at: 497, out: SESSION_EXIT } as const

export const DIFF_BEATS = {
  tabSwitch: SCENE.diff,
  statsRoll: 605,
  paint: 608,
  fileSelect: 646,
} as const

export const MERGE_BEATS = {
  railClick: 680,
  railTransition: 676,
  sidebarSwapOut: 678, // board → reviews crossfade
  sidebarSwapIn: 722, // reviews → board crossfade
  mergeHover: 692,
  confirmAt: 698, // click 1 → Confirm merge
  mergingAt: 708, // click 2 → Merging…
  rowFadeFrom: 716,
  rowFadeTo: 726,
  doneAt: 724, // EXP-151 regroups to Done
  regroupEnd: 742,
} as const

// ── Email beat (S9) ───────────────────────────────────────────────────────────
export const EMAIL_BEATS = { appear: 742, fade: 766 } as const

// ── Cursor choreographies (window-local) ──────────────────────────────────────
const PAY = SITE_ANCHORS.payButton
const FAB = SITE_ANCHORS.fab
const WT = WIDGET_ANCHORS.titleInput
const WD = WIDGET_ANCHORS.detailsInput
const WS = WIDGET_ANCHORS.send
const startCoding = {
  x: WIN.rail + WIN.sidebar + DETAIL_ANCHORS.startCoding.x,
  y: WIN.topBar + WIN.dockTabs + DETAIL_ANCHORS.startCoding.y,
}
const SD = START_DIALOG_ANCHORS
const railReviews = railIconCenter("reviews")
const BOARD_ROW_151 = { x: 174, y: 206 } // sidebar: In Progress header+row, Todo header, EXP-151 first
const MERGE_BTN = { x: 263, y: 122 }
const CONFIRM_BTN = { x: 238, y: 122 }
const LOOP_REST = { x: 900, y: 560 } // cursor position at f0 AND f779 (seamless loop)

// L1 — the visitor: dead clicks → FAB → widget form → send.
export const CURSOR_SITE_KEYS: CursorKey[] = [
  { f: 0, x: LOOP_REST.x, y: LOOP_REST.y },
  { f: 26, x: PAY.x, y: PAY.y },
  { f: 70, x: PAY.x, y: PAY.y },
  { f: 86, x: FAB.x, y: FAB.y },
  { f: 104, x: FAB.x, y: FAB.y },
  { f: 118, x: 1440, y: 820 },
  { f: 144, x: WT.x, y: WT.y },
  { f: 170, x: WT.x, y: WT.y },
  { f: 178, x: WD.x, y: WD.y },
  { f: 206, x: WD.x, y: WD.y },
  { f: 214, x: WS.x, y: WS.y },
  { f: 226, x: WS.x, y: WS.y },
  { f: 240, x: 1430, y: 900 },
]
export const CURSOR_SITE = {
  from: 0,
  to: 250,
  clicks: [SITE_BEATS.payClick1, SITE_BEATS.payClick2, SITE_BEATS.fabClick, SITE_BEATS.titleClick, SITE_BEATS.detailsClick, SITE_BEATS.sendClick],
} as const

// L2 — board pick → Start coding → dialog.
export const CURSOR_APP1_KEYS: CursorKey[] = [
  { f: 262, x: 940, y: 320 },
  { f: 276, x: 940, y: 320 },
  { f: 306, x: BOARD_ROW_151.x, y: BOARD_ROW_151.y },
  { f: 336, x: BOARD_ROW_151.x, y: BOARD_ROW_151.y },
  { f: 364, x: startCoding.x, y: startCoding.y },
  { f: 414, x: startCoding.x, y: startCoding.y },
  { f: 428, x: SD.rows[1].row.x, y: SD.rows[1].row.y },
  { f: 438, x: SD.rows[1].row.x, y: SD.rows[1].row.y },
  { f: 446, x: SD.rows[3].row.x, y: SD.rows[3].row.y },
  { f: 452, x: SD.rows[3].row.x, y: SD.rows[3].row.y },
  { f: 462, x: SD.startCoding.x, y: SD.startCoding.y },
  { f: 474, x: SD.startCoding.x, y: SD.startCoding.y },
  { f: 486, x: 1240, y: 850 },
]
export const CURSOR_APP1 = {
  from: 262,
  to: 486,
  clicks: [BOARD_BEATS.rowClick, BOARD_BEATS.startClick, DIALOG_BEATS.starting],
} as const

// L3 — rail → two-stage merge.
export const CURSOR_APP2_KEYS: CursorKey[] = [
  { f: 668, x: 900, y: 400 },
  { f: 678, x: railReviews.x, y: railReviews.y },
  { f: 686, x: railReviews.x, y: railReviews.y },
  { f: 694, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 700, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 704, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 712, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 724, x: 500, y: 560 },
]
export const CURSOR_APP2 = {
  from: 668,
  to: 730,
  clicks: [MERGE_BEATS.railClick, MERGE_BEATS.confirmAt, MERGE_BEATS.mergingAt],
} as const

// L4 — drift back to the loop rest position (matches f0).
export const CURSOR_END_KEYS: CursorKey[] = [
  { f: 740, x: 1240, y: 720 },
  { f: 776, x: LOOP_REST.x, y: LOOP_REST.y },
]
export const CURSOR_END = { from: 740, to: 780, clicks: [] as number[] } as const

// ── Caption windows ───────────────────────────────────────────────────────────
export const CAPTIONS = {
  s1: { in: 16, out: 96 },
  s2: { in: 116, out: 242 },
  s3: { in: 264, out: 322 },
  s5: { in: 424, out: 478 },
  s6: { in: 500, out: 590 },
  s7: { in: 606, out: 668 },
  s8: { in: 684, out: 728 },
  s9: { in: 744, out: 764 },
} as const
