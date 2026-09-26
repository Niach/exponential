// showreel/theme.ts — the reel's palette + type (EXP-1100). Colours come from
// the ClosedLoop rig's app-exact glass palette; the faces are the marketing
// site's self-hosted Geist / Inter / JetBrains Mono (fonts.ts loads them from
// public/fonts, so a render never touches Google Fonts).

import { C } from "../ships/theme"

export { C }

export const DISPLAY = `"Geist", "Inter", ui-sans-serif, system-ui, sans-serif`
export const UI = `"Inter", ui-sans-serif, system-ui, sans-serif`
export const MONO = `"JetBrains Mono", "Geist Mono", ui-monospace, SFMono-Regular, monospace`

// The wallpaper violet (rig.tsx WALLPAPER_BLOBS) as an accent.
export const VIOLET = "#8b5cf6"
export const VIOLET_SOFT = "rgba(139,92,246,0.35)"

// tokens.json `avatar` hues, in order.
export const AVATAR = [
  "#F87171",
  "#FB923C",
  "#FACC15",
  "#4ADE80",
  "#2DD4BF",
  "#60A5FA",
  "#A78BFA",
  "#F472B6",
] as const

export const W = 1920
export const H = 1080
