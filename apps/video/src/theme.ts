import { loadFont } from "@remotion/google-fonts/Inter"

export const { fontFamily } = loadFont("normal", {
  weights: ["500", "600", "700"],
  subsets: ["latin"],
  ignoreTooManyRequestsWarning: true,
})

export const COLORS = {
  bg: "#09090b",
  bgSoft: "#111113",
  panel: "#18181b",
  border: "rgba(255,255,255,0.08)",
  text: "#fafafa",
  textMuted: "#a1a1aa",
  accent: "#6366f1",
  accentSoft: "#818cf8",
}
