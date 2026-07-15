// scenes/timeline.ts — scene boundaries, the full camera keyframe list, dock
// height clock, terminal/phone/orchestrator schedules, flow-graph schedule,
// release progress steps and the three cursor choreographies.
// Every frame value here is COMPOSITION-GLOBAL (the Film sequence starts at 0).

import { interpolate, spring } from "remotion"
import { EASE, SETTLE, WIN } from "../theme"
import { ORCH_SESSION, type SessionEvent } from "../fixtures"
import type { CamKey, CursorKey } from "../rig"
import {
  EFFORT_OPTIONS,
  ISSUE_DIALOG_ANCHORS,
  MODEL_OPTIONS,
  RELEASE_DIALOG_ANCHORS,
  type SelectMenuSpec,
} from "../surfaces/dialogs"
import { DETAIL_ANCHORS } from "../surfaces/detail"
import { railIconCenter } from "../surfaces/chrome"
import type { FlowGraphSchedule } from "../surfaces/flowgraph"
import type { ReleaseProgressStep } from "../surfaces/releases"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// ── Scene boundaries (storyboard §1) ─────────────────────────────────────────
export const SCENE = {
  s1: 0,
  s2: 75,
  s3: 180,
  s4: 300,
  s5: 405,
  s6: 525,
  s7: 705,
  s8: 810,
  s9: 930,
  s10: 1080,
  s11: 1260,
  filmEnd: 1380,
  outroEnd: 1500,
} as const

// ── Camera keyframes (storyboard per-scene camera specs, global frames) ──────
// camAt eases each segment with the NEXT key's ease ("linear" opts out of EASE).
export const CAMERA_KEYS: CamKey[] = [
  // S1 — terminal macro. Tighter than the storyboard's 1.8 (which left dead
  // voids above the tab strip and below the window): 2.3 makes the dock fill
  // the mid-frame band with 27px-effective mono, slight rightward drift.
  { f: 0, s: 2.3, x: 440, y: 872 },
  { f: 75, s: 2.3, x: 462, y: 872, ease: "linear" },
  // S2 — pull-back reveal, hold on the floating window
  { f: 120, s: 1.0, x: 784, y: 490 },
  { f: 179, s: 1.0, x: 784, y: 490 },
  // S3 — hard cut onto the sidebar board (y raised 430→395 so the "All Issues"
  // title row makes the crop; x 615 pins the window's left edge to the frame
  // edge instead of leaving a canvas void left of the rail)
  { f: 180, s: 1.55, x: 615, y: 395 },
  { f: 300, s: 1.55, x: 615, y: 395 },
  // S4 — settle on the detail header + properties
  { f: 330, s: 1.4, x: 800, y: 330 },
  { f: 405, s: 1.4, x: 800, y: 330 },
  // S5 — dialog centered
  { f: 420, s: 1.45, x: 784, y: 470 },
  { f: 525, s: 1.45, x: 784, y: 470 },
  // S6 — tilt down onto the dock. y 686 pins the window's bottom edge to the
  // frame bottom (no floating black band); x held constant — the storyboard's
  // rightward drift cut the "Opened PR #214" payoff line at the left edge.
  { f: 543, s: 1.5, x: 685, y: 686 },
  { f: 705, s: 1.5, x: 685, y: 686 },
  // S7 — up onto the diff, then the slow "reading" focus-y drift (scrollY stays 0)
  { f: 725, s: 1.5, x: 930, y: 400 },
  { f: 740, s: 1.5, x: 930, y: 400 },
  { f: 800, s: 1.5, x: 930, y: 460, ease: "linear" },
  // S8 — pan left to rail + sidebar (x 630 pins the window-left to the frame edge)
  { f: 810, s: 1.5, x: 930, y: 460 },
  { f: 822, s: 1.5, x: 630, y: 430 },
  { f: 930, s: 1.5, x: 630, y: 430 },
  // S9 — whip-pan with overshoot, hold, then ease out for the release dialog
  { f: 934, s: 1.5, x: 680, y: 400, ease: "linear" },
  { f: 936, s: 1.5, x: 630, y: 395 },
  { f: 984, s: 1.5, x: 630, y: 395 },
  { f: 994, s: 1.35, x: 784, y: 470 },
  { f: 1080, s: 1.35, x: 784, y: 470 },
  // S10 — wide hold on graph + sidebar + dock, micro-push on wave 2
  { f: 1110, s: 1.15, x: 830, y: 500 },
  { f: 1190, s: 1.15, x: 830, y: 500 },
  { f: 1210, s: 1.22, x: 860, y: 480 },
  { f: 1260, s: 1.22, x: 860, y: 480 },
  // S11 — final pull-back to the full window
  { f: 1320, s: 1.0, x: 784, y: 490 },
]

// Whip-pan directional blur amount (px) around the S9 transition.
export const whipBlurAt = (frame: number): number =>
  interpolate(frame, [930, 933, 936, 939], [0, 3, 3, 0], CLAMP)

// ── Dock height clock (29 ↔ 240) ─────────────────────────────────────────────
export const DOCK_COLLAPSE_END = 717 // S7 quiet collapse finishes here

export const dockHeightAt = (frame: number): number => {
  if (frame < SCENE.s3) return WIN.dockExpanded // flash-forward: already open
  if (frame < SCENE.s6) return WIN.dockStrip
  if (frame < SCENE.s7) {
    const t = spring({ frame: frame - SCENE.s6, fps: 30, config: SETTLE })
    return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t
  }
  if (frame < DOCK_COLLAPSE_END)
    return interpolate(frame, [SCENE.s7, DOCK_COLLAPSE_END], [WIN.dockExpanded, WIN.dockStrip], { ...CLAMP, easing: EASE })
  if (frame < SCENE.s10) return WIN.dockStrip
  const t = spring({ frame: frame - SCENE.s10, fps: 30, config: SETTLE })
  return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t
}

// ── Terminal schedules (indices into fixtures.HERO_SESSION) ──────────────────
// S1/S2 flash-forward: the head of the session pre-rendered (negative frames),
// the Bash-test args mid-typing AT frame 0 (motion at frame zero), the spinner
// ticking f14–40, the push typing from f40, then the S2 PR beat (f130/f156).
export const S1_FEED_SCHEDULE: number[] = [-70, -60, -50, -40, -25, -4, 14, 40, 130, 156]

// S6 full replay (storyboard S6 frame table mapped onto the fixture events).
export const S6_FEED_SCHEDULE: number[] = [546, 566, 584, 600, 615, 630, 648, 660, 681, 693]

// Phone mirror: each PHONE_FEED item lands 2f after its desktop counterpart
// (PHONE_FEED has no spinner, so index 6+ maps to desktop events 7/8/9).
export const PHONE_SCHEDULE: number[] = [548, 568, 586, 602, 617, 632, 662, 683, 695]

export const S6_EXIT_AT = 699
export const PR_DOT_AT = 696

// ── Orchestrator feed (S10) + the S11 auto-ship line ─────────────────────────
export const ORCH_EVENTS: SessionEvent[] = [
  ...ORCH_SESSION,
  { kind: "flash", text: "Merged PR #219 — release v0.12 shipped" },
]
export const ORCH_SCHEDULE: number[] = [1070, 1112, 1140, 1152, 1164, 1190, 1204, 1216, 1244, 1250, 1254, 1272]

// ── Flow graph schedule (storyboard S10 table) ────────────────────────────────
export const FLOW_SCHEDULE: FlowGraphSchedule = {
  drawMain: 1092,
  drawRel: 1092,
  wave1At: [1120, 1122, 1124],
  wave1MergeAt: [1158, 1170, 1182],
  wave2At: [1190, 1193],
  wave2MergeAt: [1222, 1234],
  prChipAt: 1250,
  prMergedAt: 1272,
}

// ── Release detail progress + status flips (S10/S11) ──────────────────────────
export const PROGRESS_STEPS: ReleaseProgressStep[] = [
  { at: 1160, from: 3, to: 4, dur: 6 },
  { at: 1172, from: 4, to: 5, dur: 6 },
  { at: 1184, from: 5, to: 6, dur: 6 },
  { at: 1226, from: 6, to: 7, dur: 6 },
  { at: 1266, from: 7, to: 8, dur: 12 },
]

export const STATUS_FLIPS: Record<string, number> = {
  "EXP-139": 1158,
  "EXP-141": 1170,
  "EXP-143": 1182,
  "EXP-144": 1222,
  "EXP-145": 1234,
}

export const SHIPPED_AT = 1290
export const CASCADE_DONE_AT = 1300

// ── Issue dialog menus (S5) ───────────────────────────────────────────────────
export const MODEL_MENU: SelectMenuSpec = {
  openAt: 420,
  closeAt: 438,
  options: MODEL_OPTIONS,
  highlight: { option: "Fable", at: 432 },
}
export const EFFORT_MENU: SelectMenuSpec = {
  openAt: 447,
  closeAt: 462,
  options: EFFORT_OPTIONS,
  highlight: { option: "Max", at: 456 },
}

// ── Cursor choreographies (window-local coords) ───────────────────────────────
// Anchor math: detail-pane anchors are pane-local — window = anchor + (304, 67).
const startCoding = {
  x: WIN.rail + WIN.sidebar + DETAIL_ANCHORS.startCoding.x,
  y: WIN.topBar + WIN.dockTabs + DETAIL_ANCHORS.startCoding.y,
}
const IA = ISSUE_DIALOG_ANCHORS
const RA = RELEASE_DIALOG_ANCHORS
const fableOpt = IA.modelOptions[0] // "Fable"
const maxOpt = IA.effortOptions[5] // "Max"
const railReviews = railIconCenter("reviews")

// Estimated window-local targets (no exported anchors for these — see notes):
const BOARD_ROW_142 = { x: 174, y: 206 } // sidebar content top 108 + Todo group row 2
const MERGE_BTN = { x: 263, y: 122 } // ReviewsTool row button (rest, w54)
const CONFIRM_BTN = { x: 238, y: 122 } // widened confirm state (w104)
const RELEASE_ROW = { x: 164, y: 101 } // ReleasesTool v0.12 row center
const RELEASE_START = { x: 188, y: 87 } // release-detail "▷ Start coding" action

// L1 — S3 board pick → S4 Start coding → S5 dialog interactions.
export const CURSOR_L1_KEYS: CursorKey[] = [
  { f: 210, x: 980, y: 280 },
  { f: 240, x: BOARD_ROW_142.x, y: BOARD_ROW_142.y },
  { f: 346, x: BOARD_ROW_142.x, y: BOARD_ROW_142.y },
  { f: 372, x: startCoding.x, y: startCoding.y },
  { f: 406, x: startCoding.x, y: startCoding.y },
  { f: 418, x: IA.modelSelect.x, y: IA.modelSelect.y },
  { f: 422, x: IA.modelSelect.x, y: IA.modelSelect.y },
  { f: 430, x: fableOpt.x, y: fableOpt.y },
  { f: 436, x: fableOpt.x, y: fableOpt.y },
  { f: 445, x: IA.effortSelect.x, y: IA.effortSelect.y },
  { f: 449, x: IA.effortSelect.x, y: IA.effortSelect.y },
  { f: 454, x: maxOpt.x, y: maxOpt.y },
  { f: 460, x: maxOpt.x, y: maxOpt.y },
  { f: 470, x: IA.planCheckbox.x, y: IA.planCheckbox.y },
  { f: 476, x: IA.planCheckbox.x, y: IA.planCheckbox.y },
  { f: 500, x: IA.startCoding.x, y: IA.startCoding.y },
  { f: 518, x: IA.startCoding.x, y: IA.startCoding.y },
  { f: 536, x: 1620, y: 940 },
]
export const CURSOR_L1 = { from: 210, to: 536, clicks: [276, 390, 420, 432, 447, 456, 472, 516] } as const

// L2 — S8 rail click → merge two-click.
export const CURSOR_L2_KEYS: CursorKey[] = [
  { f: 808, x: 620, y: 320 },
  { f: 818, x: railReviews.x, y: railReviews.y },
  { f: 826, x: railReviews.x, y: railReviews.y },
  { f: 848, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 858, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 864, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 886, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 898, x: 700, y: 830 },
]
export const CURSOR_L2 = { from: 808, to: 898, clicks: [820, 855, 882] } as const

// L3 — S9 releases drill-in → Start coding → dialog primary.
export const CURSOR_L3_KEYS: CursorKey[] = [
  { f: 938, x: 620, y: 520 },
  { f: 952, x: RELEASE_ROW.x, y: RELEASE_ROW.y },
  { f: 962, x: RELEASE_ROW.x, y: RELEASE_ROW.y },
  { f: 978, x: RELEASE_START.x, y: RELEASE_START.y },
  { f: 992, x: RELEASE_START.x, y: RELEASE_START.y },
  { f: 1008, x: 1085, y: 640 },
  { f: 1054, x: 1085, y: 640 },
  { f: 1064, x: RA.startCoding.x, y: RA.startCoding.y },
  { f: 1072, x: RA.startCoding.x, y: RA.startCoding.y },
  { f: 1078, x: 1260, y: 900 },
]
export const CURSOR_L3 = { from: 938, to: 1078, clicks: [956, 984, 1068] } as const

// ── Caption schedule (fixtures.COPY, storyboard in/out frames) ────────────────
export const CAPTIONS = {
  hook: { in: 24, out: 92 },
  s2: { in: 96, out: 168 },
  s3: { in: 192, out: 288 },
  s5: { in: 417, out: 510 },
  s6a: { in: 537, out: 612 },
  s6b: { in: 622, out: 698 },
  s7: { in: 717, out: 798 },
  s8: { in: 822, out: 918 },
  s9: { in: 942, out: 1074 },
  s10: { in: 1092, out: 1250 },
  s11: { in: 1300, out: 1374 },
  wordmark: { in: 130, out: 1330 },
} as const
