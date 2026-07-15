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

// App-exact palette (design-tokens + web styles.css conversions; matched against ref/*.png).
export const C = {
  canvas: "#09090b", // video canvas only
  bg: "#0a0a0a", // app background
  panel: "#171717", // sidebar / cards / popovers / dialogs
  accentBg: "#262626", // hover / secondary / active-pill
  text: "#fafafa",
  muted: "#a1a1a1",
  dim: "#737373",
  border: "rgba(255,255,255,0.10)",
  borderSoft: "rgba(255,255,255,0.05)",
  borderRow: "rgba(255,255,255,0.03)",
  input: "rgba(255,255,255,0.15)",
  primary: "#e5e5e5", // primary button bg (text #171717)
  primaryFg: "#171717",
  destructive: "#ff6467",
  indigo: "#4f46e5", // New Issue button
  indigoSoft: "#6366f1", // project dot / rail active / brand accent
  indigoGlow: "#818cf8",
  // status
  statusBacklog: "#a1a1a1",
  statusTodo: "#fafafa",
  statusInProgress: "#eab308",
  statusDone: "#22c55e",
  // priority
  prioUrgent: "#ef4444",
  prioHigh: "#f97316",
  prioMedium: "#eab308",
  prioLow: "#3b82f6",
  // group header tints
  tintBacklog: "rgba(113,113,122,0.08)",
  tintTodo: "rgba(212,212,216,0.08)",
  tintInProgress: "rgba(234,179,8,0.10)",
  tintDone: "rgba(34,197,94,0.10)",
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
  // terminal (real claude CLI grammar)
  termToolDot: "#22c55e", // ● before tool names
  termProseDot: "#fafafa", // ● before Claude prose
  termSpinner: "#eab308", // ✳ Vibing…
  termWarn: "#f97316",
  termBypass: "#ef4444", // "bypass permissions on"
  green: "#22c55e",
  greenSoft: "#34d399",
} as const

// Desktop window metrics (window-local px; storyboard §0.1, calibrated on ref shots).
export const WIN = {
  w: 1568,
  h: 980,
  x: 176, // comp position
  y: 50,
  radius: 10,
  topBar: 38,
  rail: 44,
  sidebar: 260,
  propsPanel: 288,
  row: 28, // board row height
  dockExpanded: 240,
  dockStrip: 29,
  dockTabs: 29,
} as const

export const EASE = Easing.bezier(0.16, 1, 0.3, 1)
// Named spring configs (use with remotion spring()):
export const POP = { damping: 12, stiffness: 200 } as const // pills, badges, tabs
export const SETTLE = { damping: 16, stiffness: 140 } as const // dock resize, dialogs
