// EXP-1324: the Text `shimmer` (catalog/style.json `textShimmer`): the
// colours a painter fills the glyphs with while a TraceItem runs. Pure; the
// Rust core's `text_shimmer` ports it and fixtures/text-shimmer.json locks it.

import styleJson from "../catalog/style.json" with { type: "json" }
import { parseColor, toHex } from "./color"

export const TEXT_SHIMMER = styleJson.textShimmer

export interface TextShimmer {
  /** The text colour (`#rrggbb[aa]`). */
  base: string
  /** The band's centre. */
  highlight: string
  /** The 50% OKLab mix at ± spread/2. */
  mid: string
  durationMs: number
  angleDeg: number
  spreadPx: number
}

type Lab = [number, number, number]

const toLinear = (c: number) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4))
const toGamma = (c: number) => {
  const x = Math.max(0, Math.min(1, c))
  return x <= 0.0031308 ? 12.92 * x : 1.055 * Math.pow(x, 1 / 2.4) - 0.055
}

function rgbToOklab(r: number, g: number, b: number): Lab {
  const [lr, lg, lb] = [toLinear(r / 255), toLinear(g / 255), toLinear(b / 255)]
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb)
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb)
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb)
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ]
}

function oklabToRgb([L, a, b]: Lab): [number, number, number] {
  const l = Math.pow(L + 0.3963377774 * a + 0.2158037573 * b, 3)
  const m = Math.pow(L - 0.1055613458 * a - 0.0638541728 * b, 3)
  const s = Math.pow(L - 0.0894841775 * a - 1.291485548 * b, 3)
  return [
    toGamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s) * 255,
    toGamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s) * 255,
    toGamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s) * 255,
  ]
}

const hex = (lab: Lab, alpha: number) => {
  const [r, g, b] = oklabToRgb(lab)
  return toHex({ r, g, b, a: alpha })
}

/** The shimmer colours for a text colour (`#rrggbb[aa]`); `dark` = the
 *  surface's dark mode. An unparsable colour shimmers from black. */
export function textShimmer(color: string, dark: boolean): TextShimmer {
  const t = TEXT_SHIMMER
  const c = parseColor(color) ?? { r: 0, g: 0, b: 0, a: 1 }
  const base = rgbToOklab(c.r, c.g, c.b)
  let high: Lab = base
  let highAlpha = c.a * t.lightAlphaFactor
  if (dark) {
    high = [Math.min(1, Math.max(t.highlightLightnessMin, base[0] + t.highlightLightnessBoost)), base[1], base[2]]
    highAlpha = Math.min(1, c.a + t.highlightAlphaBoost)
  }
  const sum = c.a + highAlpha
  const mid: Lab = sum > 0 ? [0, 1, 2].map((i) => (base[i] * c.a + high[i] * highAlpha) / sum) as Lab : base
  return {
    base: toHex(c),
    highlight: hex(high, highAlpha),
    mid: hex(mid, sum / 2),
    durationMs: t.durationMs,
    angleDeg: t.angleDeg,
    spreadPx: t.spreadPx,
  }
}
