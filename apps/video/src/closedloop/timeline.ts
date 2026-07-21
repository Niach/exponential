// closedloop/timeline.ts — scene boundaries, chapter markers (consumed by the
// marketing player UI), camera keyframes, whip-pan blur, dock-height clock,
// terminal feed schedule, cursor choreographies and the caption windows.
// Every frame value is COMPOSITION-GLOBAL. The story runs 1030 frames @ 30fps
// (~34s): after the merge it closes on the full-frame Shipped card + platform
// lineup (EXP-200 — replaced the reply-email beat; EXP-217 added the brand
// header + web mock and a longer hold), whose content fades to the bare canvas
// before STORY_FRAMES; the END_HOLD tail rests on that canvas so the Player
// loop breathes before wrapping back to the f0 storefront.

import { interpolate, spring } from "remotion";
import { CHAPTER_INFO } from "./chapters";
import { SETTLE, WIN } from "../ships/theme";
import type { CamKey, CursorKey } from "../ships/rig";
import { DETAIL_ANCHORS } from "../ships/surfaces/detail";
import { railIconCenter } from "../ships/surfaces/chrome";
import { SITE_ANCHORS } from "./surfaces/sitemock";
import { WIDGET_ANCHORS } from "./surfaces/widgetmock";
import { START_DIALOG_ANCHORS } from "./surfaces/startdialog";

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

export const FPS = 30;
// The authored story — the ending overlay's content has fully faded to the
// bare canvas by here (EXP-200 retired the old f779==f0 seamless-loop anchor;
// the wrap is now canvas-rest → hard restart on the f0 storefront).
export const STORY_FRAMES = 1030;
// Rest on the bare canvas before the loop wraps (EXP-176: "loops too fast").
export const END_HOLD = 20;
export const DURATION_IN_FRAMES = STORY_FRAMES + END_HOLD;

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
  shipped: 735, // full-frame Shipped card — logo draw + headline (EXP-200)
  platforms: 800, // the every-platform lineup: MacBook IDE + phone + store icons
  end: STORY_FRAMES, // the END_HOLD tail after this is pure rest
} as const;

// Chapter markers for the marketing player scrubber — the id/label/phrase
// metadata lives in chapters.ts (remotion-free, shared with the marketing
// rail); only the frame mapping is authored here.
export type Chapter = { id: string; label: string; frame: number };
const CHAPTER_FRAMES: Record<string, number> = {
  feedback: SCENE.site,
  issue: SCENE.board,
  code: SCENE.dialog,
  merge: SCENE.diff,
  shipped: SCENE.shipped,
};
export const CHAPTERS: Chapter[] = CHAPTER_INFO.map(({ id, label }) => {
  const frame = CHAPTER_FRAMES[id];
  if (frame === undefined) throw new Error(`chapter ${id} has no frame`);
  return { id, label, frame };
});

// ── Camera (single global key list; 1-frame gaps = hard cuts under whip blur) ─
// EXP-155: every shot reads ~15–20% tighter so the embedded film stays legible
// at page width. EXP-176: the app shots (S2–S6) gained another ~6% for phone
// widths — S1/S9 stay untouched so the loop anchor and the checked-in poster
// (frame 0) remain valid. Viewport-in-window bounds for the 1568×980 chassis:
// 960/s ≤ x ≤ 1568−960/s and 540/s ≤ y ≤ 980−540/s (deliberately violated
// only where an edge is meant to pin past the window, e.g. the S6 dock tilt).
export const CAMERA_KEYS: CamKey[] = [
  // S1 — browser window fills the viewport height, then a push to the pay button
  { f: 0, s: 1.1, x: 784, y: 490 },
  { f: 22, s: 1.1, x: 784, y: 490 },
  { f: 46, s: 1.55, x: 920, y: 475 },
  { f: 78, s: 1.55, x: 920, y: 475 },
  // S2 — settle onto the FAB corner / widget panel zone (bottom-right pinned)
  { f: 98, s: 1.85, x: 1049, y: 688 },
  { f: 254, s: 1.85, x: 1049, y: 688 },
  // S3 — hard cut onto the app board (left/top pinned like the ships framing)
  { f: 255, s: 1.9, x: 507, y: 331 },
  { f: 332, s: 1.9, x: 507, y: 331 },
  // S4 — fly to the detail header + properties
  { f: 356, s: 1.75, x: 915, y: 325 },
  { f: 408, s: 1.75, x: 915, y: 325 },
  // S5 — dialog centered (EXP-217: the rebuilt 500px-tall dialog needs the
  // slightly wider 1.7 shot so its bottom still clears the caption band)
  { f: 420, s: 1.7, x: 784, y: 500 },
  { f: 500, s: 1.7, x: 784, y: 500 },
  // S6 — tilt down onto the dock (window bottom pinned; y keeps the terminal
  // status line above the screen-space caption band — this shot has no
  // headroom for the EXP-176 tighten, so it keeps the EXP-155 framing)
  { f: 518, s: 1.7, x: 610, y: 718 },
  { f: 600, s: 1.7, x: 610, y: 718 },
  // S7 — up onto the diff, slow reading drift (full 1264px pane caps the zoom)
  { f: 620, s: 1.55, x: 928, y: 400 },
  { f: 632, s: 1.55, x: 928, y: 400 },
  { f: 672, s: 1.55, x: 928, y: 455, ease: "linear" },
  // S8 — pan left to the rail + reviews sidebar; the camera parks here — the
  // opaque Shipped/platforms overlay (S9, EXP-200) covers everything after 743
  { f: 676, s: 1.55, x: 928, y: 455 },
  { f: 688, s: 1.7, x: 575, y: 385 },
  { f: 732, s: 1.7, x: 575, y: 385 },
];

// Whip-pan blur at the one remaining hard cut (site → app; the old second cut
// back to the site is gone — the Shipped card fades in over the merge shot).
export const whipBlurAt = (frame: number): number =>
  interpolate(frame, [251, 254, 256, 259], [0, 3, 3, 0], CLAMP);

// ── Dock height clock (29 ↔ 240) ─────────────────────────────────────────────
export const DOCK_COLLAPSE_END = 614;

export const dockHeightAt = (frame: number): number => {
  if (frame < SCENE.dock) return WIN.dockStrip;
  if (frame < SCENE.diff) {
    const t = spring({ frame: frame - SCENE.dock, fps: 30, config: SETTLE });
    return WIN.dockStrip + (WIN.dockExpanded - WIN.dockStrip) * t;
  }
  if (frame < DOCK_COLLAPSE_END)
    return interpolate(
      frame,
      [SCENE.diff, DOCK_COLLAPSE_END],
      [WIN.dockExpanded, WIN.dockStrip],
      CLAMP,
    );
  return WIN.dockStrip;
};

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
} as const;

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
} as const;

export const DIALOG_BEATS = {
  appear: 420,
  checkPulse: 430,
  rowHover: { index: 1, at: 436, out: 452 },
  buttonHover: 466,
  starting: 472,
  collapse: 482,
  scrimOut: 482,
} as const;

// ── Terminal feed (indices into fixtures.CL_SESSION) ──────────────────────────
export const FEED_SCHEDULE: number[] = [
  500, 511, 524, 537, 549, 558, 566, 574, 584, 590,
];
export const SESSION_TAB_POP = 497;
export const SESSION_EXIT = 594;
export const PR_AT = 592; // PR chip + board dot + rail badge
export const SPINNER_BASE = { sec: 41, tokensK: 7.4 } as const;

// ── Coding / merge state frames ───────────────────────────────────────────────
export const CODING_START = 486; // EXP-151 → In Progress (behind the dialog collapse)
export const CODING_PILL = { at: 497, out: SESSION_EXIT } as const;

export const DIFF_BEATS = {
  tabSwitch: SCENE.diff,
  statsRoll: 605,
  paint: 608,
  fileSelect: 646,
} as const;

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
} as const;

// ── Shipped card + platform lineup (S9, EXP-200) ─────────────────────────────
// The overlay's backdrop goes opaque over the merge shot, the logo strokes
// draw, the headline lands, then the card crossfades into the platform lineup
// (brand header + web app in a browser + MacBook IDE + phone with their icon
// rows — all three clients, EXP-217), which holds long and fades to the bare
// canvas before STORY_FRAMES.
export const ENDING = {
  backdropIn: SCENE.shipped, // 735 — opaque by +8
  logoDrawFrom: 739,
  logoDrawTo: 771,
  titleAt: 746,
  subAt: 754,
  cardOutFrom: SCENE.platforms - 6, // 794
  cardOutTo: SCENE.platforms + 6, // 806
  brandAt: SCENE.platforms + 2, // 802 — Exponential logo + wordmark header
  webAt: SCENE.platforms + 6, // the web app in a browser window
  macAt: SCENE.platforms + 12,
  phoneAt: SCENE.platforms + 18,
  iconsAt: SCENE.platforms + 26,
  fadeOutFrom: 1006,
  fadeOutTo: 1026,
} as const;

// ── Cursor choreographies (window-local) ──────────────────────────────────────
const PAY = SITE_ANCHORS.payButton;
const FAB = SITE_ANCHORS.fab;
const WT = WIDGET_ANCHORS.titleInput;
const WD = WIDGET_ANCHORS.detailsInput;
const WS = WIDGET_ANCHORS.send;
const startCoding = {
  x: WIN.rail + WIN.sidebar + DETAIL_ANCHORS.startCoding.x,
  y: WIN.topBar + WIN.dockTabs + DETAIL_ANCHORS.startCoding.y,
};
const SD = START_DIALOG_ANCHORS;
const railReviews = railIconCenter("reviews");
const BOARD_ROW_151 = { x: 174, y: 206 }; // sidebar: In Progress header+row, Todo header, EXP-151 first
const MERGE_BTN = { x: 263, y: 122 };
const CONFIRM_BTN = { x: 238, y: 122 };
const LOOP_REST = { x: 900, y: 560 }; // cursor rest position at f0

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
];
export const CURSOR_SITE = {
  from: 0,
  to: 250,
  clicks: [
    SITE_BEATS.payClick1,
    SITE_BEATS.payClick2,
    SITE_BEATS.fabClick,
    SITE_BEATS.titleClick,
    SITE_BEATS.detailsClick,
    SITE_BEATS.sendClick,
  ],
} as const;

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
];
export const CURSOR_APP1 = {
  from: 262,
  to: 486,
  clicks: [BOARD_BEATS.rowClick, BOARD_BEATS.startClick, DIALOG_BEATS.starting],
} as const;

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
];
export const CURSOR_APP2 = {
  from: 668,
  to: 730,
  clicks: [MERGE_BEATS.railClick, MERGE_BEATS.confirmAt, MERGE_BEATS.mergingAt],
} as const;

// ── Caption windows ───────────────────────────────────────────────────────────
// (s9 lives on the Shipped card itself now — see ENDING.)
export const CAPTIONS = {
  s1: { in: 16, out: 96 },
  s2: { in: 116, out: 242 },
  s3: { in: 264, out: 322 },
  s5: { in: 424, out: 478 },
  s6: { in: 500, out: 590 },
  s7: { in: 606, out: 668 },
  s8: { in: 684, out: 728 },
} as const;
