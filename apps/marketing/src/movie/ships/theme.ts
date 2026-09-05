import { Easing } from "remotion"
import { loadFont as loadInter } from "@remotion/google-fonts/Inter"
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono"

export const { fontFamily: UI_FONT } = loadInter("normal", {
  weights: ["400", "500", "600", "700"],
  subsets: ["latin"],
  ignoreTooManyRequestsWarning: true,
})

export const { fontFamily: MONO_FONT } = loadMono("normal", {
  weights: ["400", "700"],
  subsets: ["latin"],
  ignoreTooManyRequestsWarning: true,
})

// Display type matching the marketing page: Geist resolves when the Player
// renders inside the marketing document (which self-hosts it); Remotion
// Studio falls back to Inter.
export const PAGE_FONT = `"Geist", "${UI_FONT}", ui-sans-serif, system-ui, sans-serif`

// App-exact GLASS palette (EXP-359) — transcribed from the desktop IDE theme
// (apps/desktop/crates/theme: tokens.generated.rs `glass` module + lib.rs) and
// packages/design-tokens/tokens.json. The page is ONE zinc gradient; every
// surface above it is a white-alpha fill with a hairline white-alpha stroke.
export const C = {
  canvas: "#09090b", // video canvas only (behind the window)
  // Page gradient — the desktop ramp: BACKGROUND_TOP #050507 mixed
  // GRADIENT_TOP_MIX (0.6) toward BACKGROUND_BOTTOM #111114 (EXP-723 darkened
  // both stops two notches), so the titlebar band doesn't step hard against
  // the content (EXP-277).
  bgTop: "#0c0c0f",
  bgBottom: "#111114",
  text: "#fafafa",
  muted: "#a1a1a1",
  dim: "#737373",
  // Glass fill ladder (white alphas — mobile GlassTheme parity)
  fillSection: "rgba(255,255,255,0.04)",
  fillRow: "rgba(255,255,255,0.05)",
  fillCard: "rgba(255,255,255,0.06)",
  // EXP-723: the cutout panel's wash and the quieter active fill.
  fillPanel: "rgba(255,255,255,0.04)",
  fillActive: "rgba(255,255,255,0.09)",
  // Glass stroke ladder (1px hairlines, never fractional)
  strokeRow: "rgba(255,255,255,0.06)",
  strokeSection: "rgba(255,255,255,0.08)",
  strokeCard: "rgba(255,255,255,0.10)",
  strokeStrong: "rgba(255,255,255,0.12)",
  strokeActive: "rgba(255,255,255,0.14)",
  // cx.theme().popover — the OPAQUE floor the terminal dock and its strip sit
  // on (terminal_dock.rs, EXP-723).
  popover: "#252525",
  // Floating panels (popovers / dialogs): 95% #171717 over 16px blur, glass
  // shadow with the inset top highlight (web --glass-panel-bg / --glass-shadow).
  panelFloat: "rgba(23,23,23,0.95)",
  input: "rgba(255,255,255,0.15)",
  primary: "#e5e5e5", // primary button bg (text #171717)
  primaryFg: "#171717",
  destructive: "#ff6467",
  neutral: "#a1a1aa", // board/project dots (EXP-594: indigo retired)
  // status (contract issueStatusDefaults — done is BLUE, not green)
  statusBacklog: "#a1a1aa",
  statusInProgress: "#eab308",
  statusInReview: "#22c55e",
  statusDone: "#3b82f6",
  // priority (design tokens)
  prioUrgent: "#ef4444",
  prioHigh: "#f97316",
  prioMedium: "#facc15",
  prioLow: "#3b82f6",
  // group header tints — desktop issue_list.rs status_header_tint: the status
  // accent at HEADER_TINT_ALPHA 0.10 (between the 5% hover and 15% active
  // washes); backlog degrades to the NEUTRAL zinc accent.
  tintBacklog: "rgba(161,161,170,0.10)",
  tintInProgress: "rgba(234,179,8,0.10)",
  tintDone: "rgba(59,130,246,0.10)",
  // diff
  diffAddBg: "rgba(34,197,94,0.10)",
  diffAddBgHot: "rgba(34,197,94,0.20)",
  diffDelBg: "rgba(239,68,68,0.10)",
  diffDelBgHot: "rgba(239,68,68,0.20)",
  diffAdd: "#22c55e",
  diffDel: "#ef4444",
  hunkBg: "rgba(59,130,246,0.10)",
  hunkFg: "#60a5fa",
  // syntax tints (matched to ref diff/terminal shots)
  synKeyword: "#60a5fa",
  synString: "#4ade80",
  synNumber: "#facc15",
  synComment: "#737373",
  synType: "#5eead4",
  // terminal (real claude CLI grammar); the dock blends with the gradient's
  // bottom stop (theme/src/terminal.rs — EXP-277)
  termBg: "#111114",
  termToolDot: "#22c55e", // ● before tool names
  termProseDot: "#fafafa", // ● before Claude prose
  termSpinner: "#eab308", // ✳ Vibing…
  termWarn: "#f97316",
  termBypass: "#ef4444", // "bypass permissions on"
  green: "#22c55e",
  greenSoft: "#34d399",
} as const

// Glass region alphas + blur (desktop lib.rs glass_*_alpha; macOS look):
// the rail column is the glassiest surface (0.72), content nearly solid
// (0.96); the window sits on a blurred backdrop (NSVisualEffectView ≈ 64px).
export const GLASS = {
  railAlpha: 0.72,
  contentAlpha: 0.96,
  phoneAlpha: 0.88, // mobile GlassTheme screen — between rail and content
  panelBlur: 16, // floating panels
  backdropBlur: 64, // behind-window blur
  shadow:
    "0 12px 32px -12px rgba(0,0,0,0.45), inset 0 1px 0 rgba(255,255,255,0.06)",
} as const

// Glass radii ladder (row 10 / section 12 / card 16 / sheet 24 — the IDE scale).
export const R = {
  row: 10,
  section: 12,
  card: 16,
  sheet: 24,
} as const

// Desktop window metrics (window-local px) — the POST-EXP-253/282/723 shell:
// no top bar (tabs are glass chips in the 34px decoration band), ONE
// always-open labelled rail (208, sidebar.rs RAIL_W) sitting bare on the
// ground, and the working surface as EXP-723's CUTOUT PANEL: a rounded card
// inset 6px under the band and 10px on the other three sides (shell.rs
// PANEL_MARGIN / PANEL_MARGIN_TOP) holding the issue-list tool window (520),
// the center and the terminal dock. The detail pane has no properties
// sidebar since EXP-471 — its properties are the pill bar under the title
// (shots/issue-detail/desktop.webp).
const WIN_W = 1568
const WIN_H = 980
const TITLE_BAR = 34
const RAIL_W = 208
const PANEL_MARGIN = 10
const PANEL_MARGIN_TOP = 6
export const WIN = {
  w: WIN_W,
  h: WIN_H,
  x: 176, // comp position
  y: 50,
  radius: 10,
  titleBar: TITLE_BAR,
  rail: RAIL_W,
  sidebar: 520, // issue-list tool window (sidebar.rs DEFAULT_DOCK_WIDTH)
  row: 28, // board row height
  dockExpanded: 240, // TERMINAL_DOCK_HEIGHT
  dockHeader: 28, // DOCK_HEADER_H — the open dock's own window-controls row
  dockStrip: 29, // DOCK_STRIP_H — the tabs strip, open or collapsed
  // The cutout panel rect, window-local. `right`/`bottom` are the panel's
  // far edges, so a surface pinned inside it uses `WIN.w - WIN.panel.right`
  // as its CSS `right` inset.
  panel: {
    x: RAIL_W + PANEL_MARGIN,
    y: TITLE_BAR + PANEL_MARGIN_TOP,
    w: WIN_W - RAIL_W - 2 * PANEL_MARGIN,
    h: WIN_H - TITLE_BAR - PANEL_MARGIN_TOP - PANEL_MARGIN,
    right: WIN_W - PANEL_MARGIN,
    bottom: WIN_H - PANEL_MARGIN,
    radius: 12, // radius::LG
  },
} as const

export const EASE = Easing.bezier(0.16, 1, 0.3, 1)
// Named spring configs (use with remotion spring()):
export const POP = { damping: 12, stiffness: 200 } as const // pills, badges, tabs
export const SETTLE = { damping: 16, stiffness: 140 } as const // dock resize, dialogs
