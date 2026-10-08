// VAPP-92: colour parsing for the theme loader and the builder's importer.
// A theme file carries ONLY `#rrggbb[aa]`; everything a shadcn `globals.css`
// or a tweakcn export may contain (oklch, hsl, rgb, bare HSL triplets, hex)
// is converted here, once, on import. Zero dependencies; OKLCH → sRGB uses
// Björn Ottosson's matrices, the same ones packages/design-tokens uses.

export interface Rgba {
  r: number
  g: number
  b: number
  /** 0–1 */
  a: number
}

const HEX = /^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/

function clamp01(n: number): number {
  return Math.max(0, Math.min(1, n))
}

function srgbGamma(linear: number): number {
  const c = clamp01(linear)
  return c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055
}

function oklabToRgb(L: number, a: number, b: number): [number, number, number] {
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b
  const s_ = L - 0.0894841775 * a - 1.291485548 * b
  const l = l_ * l_ * l_
  const m = m_ * m_ * m_
  const s = s_ * s_ * s_
  return [
    srgbGamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    srgbGamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    srgbGamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ]
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const k = (n: number) => (n + h / 30) % 12
  const a = s * Math.min(l, 1 - l)
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)))
  return [f(0), f(8), f(4)]
}

function alphaOf(raw: string | undefined): number {
  if (raw === undefined) return 1
  const t = raw.trim()
  if (t.endsWith(`%`)) return clamp01(parseFloat(t) / 100)
  return clamp01(parseFloat(t))
}

function pct(raw: string): number {
  const t = raw.trim()
  return t.endsWith(`%`) ? parseFloat(t) / 100 : parseFloat(t)
}

function hue(raw: string): number {
  const t = raw.trim()
  if (t.endsWith(`deg`)) return parseFloat(t)
  if (t.endsWith(`turn`)) return parseFloat(t) * 360
  if (t.endsWith(`rad`)) return (parseFloat(t) * 180) / Math.PI
  return parseFloat(t)
}

/** Any CSS colour a theme import may meet → `Rgba`, or null when it is not
 *  one (a `var()`, a word we do not know). */
export function parseColor(input: unknown): Rgba | null {
  if (typeof input !== `string`) return null
  const s = input.trim()
  const hex = HEX.exec(s)
  if (hex) {
    let h = hex[1]
    if (h.length <= 4) h = [...h].map((c) => c + c).join(``)
    const n = (i: number) => parseInt(h.slice(i, i + 2), 16)
    return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 }
  }
  const named: Record<string, string> = { white: `#ffffff`, black: `#000000`, transparent: `#00000000` }
  if (named[s.toLowerCase()]) return parseColor(named[s.toLowerCase()])

  const fn = /^([a-z]+)\((.*)\)$/i.exec(s)
  if (fn) {
    const name = fn[1].toLowerCase()
    const [body, alphaPart] = fn[2].split(`/`)
    const parts = body.trim().split(/[\s,]+/).filter(Boolean)
    const alpha = alphaOf(alphaPart ?? (parts.length === 4 ? parts[3] : undefined))
    if (name === `oklch` && parts.length >= 3) {
      const L = pct(parts[0])
      const C = parts[1].endsWith(`%`) ? (parseFloat(parts[1]) / 100) * 0.4 : parseFloat(parts[1])
      const H = hue(parts[2])
      const [r, g, b] = oklabToRgb(L, C * Math.cos((H * Math.PI) / 180), C * Math.sin((H * Math.PI) / 180))
      return { r: to255(r), g: to255(g), b: to255(b), a: alpha }
    }
    if (name === `oklab` && parts.length >= 3) {
      const [r, g, b] = oklabToRgb(pct(parts[0]), parseFloat(parts[1]), parseFloat(parts[2]))
      return { r: to255(r), g: to255(g), b: to255(b), a: alpha }
    }
    if ((name === `hsl` || name === `hsla`) && parts.length >= 3) {
      const [r, g, b] = hslToRgb(hue(parts[0]), pct(parts[1]), pct(parts[2]))
      return { r: to255(r), g: to255(g), b: to255(b), a: alpha }
    }
    if ((name === `rgb` || name === `rgba`) && parts.length >= 3) {
      const ch = (v: string) => (v.endsWith(`%`) ? to255(parseFloat(v) / 100) : Math.round(parseFloat(v)))
      return { r: ch(parts[0]), g: ch(parts[1]), b: ch(parts[2]), a: alpha }
    }
    return null
  }
  // shadcn's pre-v4 form: a bare HSL triplet `0 0% 100%`.
  const triplet = /^(-?[\d.]+(?:deg)?)\s+([\d.]+)%\s+([\d.]+)%$/.exec(s)
  if (triplet) {
    const [r, g, b] = hslToRgb(hue(triplet[1]), parseFloat(triplet[2]) / 100, parseFloat(triplet[3]) / 100)
    return { r: to255(r), g: to255(g), b: to255(b), a: 1 }
  }
  return null
}

function to255(unit: number): number {
  return Math.round(clamp01(unit) * 255)
}

function byte(n: number): string {
  return Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, `0`)
}

/** `#rrggbb`, or `#rrggbbaa` when the alpha is below 1. Lowercase. */
export function toHex(c: Rgba): `#${string}` {
  const base = `#${byte(c.r)}${byte(c.g)}${byte(c.b)}` as const
  return c.a >= 1 ? base : (`${base}${byte(c.a * 255)}` as `#${string}`)
}

/** True for the ONE form a theme file accepts: `#rrggbb` / `#rrggbbaa`. */
export function isThemeHex(value: unknown): value is `#${string}` {
  return typeof value === `string` && /^#([0-9a-f]{6}|[0-9a-f]{8})$/.test(value)
}

/** Any CSS colour → the theme form, or null. */
export function toThemeHex(input: unknown): `#${string}` | null {
  const c = parseColor(input)
  return c ? toHex(c) : null
}

/** Relative luminance (WCAG) of an opaque colour, 0–1. */
export function luminance(c: Rgba): number {
  const lin = (v: number) => {
    const s = v / 255
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4)
  }
  return 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/** WCAG contrast ratio between two opaque colours. */
export function contrast(a: Rgba, b: Rgba): number {
  const la = luminance(a)
  const lb = luminance(b)
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
}
