// FEED-128: React's inline-style semantics for the Solid renderer. A
// painter writes the SAME style object as its React twin (camelCase keys,
// numbers for lengths) and passes it through `css()`, which does what
// react-dom's `setValueForStyle` does: camelCase → kebab-case (`ms` →
// `-ms-`), a non-zero number gets `px` unless the property is unitless
// (React's own list), strings are trimmed, null / boolean / '' drop the
// property, custom properties (`--x`) pass as they are. Key order is kept,
// so the serialised `style` attribute matches React's.

export type StyleInput = Record<string, string | number | null | undefined | boolean>
export type SolidStyle = Record<string, string>

const UNITLESS = new Set(
  `animationIterationCount aspectRatio borderImageOutset borderImageSlice borderImageWidth boxFlex boxFlexGroup boxOrdinalGroup columnCount columns flex flexGrow flexPositive flexShrink flexNegative flexOrder gridArea gridRow gridRowEnd gridRowSpan gridRowStart gridColumn gridColumnEnd gridColumnSpan gridColumnStart fontWeight lineClamp lineHeight opacity order orphans scale tabSize widows zIndex zoom fillOpacity floodOpacity stopOpacity strokeDasharray strokeDashoffset strokeMiterlimit strokeOpacity strokeWidth MozAnimationIterationCount MozBoxFlex MozBoxFlexGroup MozLineClamp msAnimationIterationCount msFlex msZoom msFlexGrow msFlexNegative msFlexOrder msFlexPositive msFlexShrink msGridColumn msGridColumnSpan msGridRow msGridRowSpan WebkitAnimationIterationCount WebkitBoxFlex WebKitBoxFlexGroup WebkitBoxOrdinalGroup WebkitColumnCount WebkitColumns WebkitFlex WebkitFlexGrow WebkitFlexPositive WebkitFlexShrink WebkitLineClamp`.split(` `)
)

const hyphenated = new Map<string, string>()
function hyphenate(name: string): string {
  let hit = hyphenated.get(name)
  if (hit === undefined) {
    hit = name.includes(`-`) ? name : name.replace(/([A-Z])/g, `-$1`).toLowerCase().replace(/^ms-/, `-ms-`)
    hyphenated.set(name, hit)
  }
  return hit
}

/** A React style object → Solid's (kebab-case keys, string values). */
export function css(style: StyleInput | undefined | null): SolidStyle | undefined {
  if (!style) return undefined
  const out: SolidStyle = {}
  for (const [name, value] of Object.entries(style)) {
    if (value === null || value === undefined || typeof value === `boolean` || value === ``) continue
    if (name.startsWith(`--`)) {
      out[name] = String(value)
      continue
    }
    const key = name === `cssFloat` ? `float` : hyphenate(name)
    out[key] = typeof value === `number` && value !== 0 && !UNITLESS.has(name) ? `${value}px` : String(value).trim()
  }
  return out
}

/** A length prop (`width={390}` / `"100%"`) the React way. */
export const px = (v: number | string): string => (typeof v === `number` ? (v === 0 ? `0` : `${v}px`) : v)
