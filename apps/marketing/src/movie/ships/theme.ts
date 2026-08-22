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
  // Page gradient — the desktop ramp: BACKGROUND_TOP #09090b mixed
  // GRADIENT_TOP_MIX (0.6) toward BACKGROUND_BOTTOM #18181b, so the titlebar
  // band doesn't step hard against the content (EXP-277).
  bgTop: "#121215",
  bgBottom: "#18181b",
  text: "#fafafa",
  muted: "#a1a1a1",
  dim: "#737373",
  // Glass fill ladder (white alphas — mobile GlassTheme parity)
  fillSection: "rgba(255,255,255,0.04)",
  fillRow: "rgba(255,255,255,0.05)",
  fillCard: "rgba(255,255,255,0.06)",
  fillActive: "rgba(255,255,255,0.15)",
  // Glass stroke ladder (1px hairlines, never fractional)
  strokeRow: "rgba(255,255,255,0.06)",
  strokeSection: "rgba(255,255,255,0.08)",
  strokeCard: "rgba(255,255,255,0.10)",
  strokeStrong: "rgba(255,255,255,0.12)",
  strokeActive: "rgba(255,255,255,0.20)",
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
  statusTodo: "#fafafa",
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
  // washes); backlog/todo degrade to the NEUTRAL zinc accent.
  tintBacklog: "rgba(161,161,170,0.10)",
  tintTodo: "rgba(161,161,170,0.10)",
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
  termBg: "#18181b",
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

// Desktop window metrics (window-local px) — the POST-EXP-253/282 shell:
// no top bar (tabs are glass chips in the 34px titlebar row), ONE expanded
// labelled rail (164), the issue-list tool window at 520, the detail
// properties sidebar at 192 (surface.rs DETAIL_SIDEBAR_WIDTH).
export const WIN = {
  w: 1568,
  h: 980,
  x: 176, // comp position
  y: 50,
  radius: 10,
  titleBar: 34,
  rail: 164, // expanded labelled rail (sidebar.rs RAIL_EXPANDED_W)
  sidebar: 520, // issue-list tool window (sidebar.rs DEFAULT_DOCK_WIDTH)
  propsPanel: 192,
  row: 28, // board row height
  dockExpanded: 240,
  dockStrip: 29,
  dockTabs: 29,
} as const

export const EASE = Easing.bezier(0.16, 1, 0.3, 1)
// Named spring configs (use with remotion spring()):
export const POP = { damping: 12, stiffness: 200 } as const // pills, badges, tabs
export const SETTLE = { damping: 16, stiffness: 140 } as const // dock resize, dialogs
