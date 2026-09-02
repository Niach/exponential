#!/usr/bin/env bun
// Emits the canonical design tokens from tokens.json into native sources.
// Run from the repo root with: `bun run --filter @exp/design-tokens generate`.
//
// The `palette` colors are authored in OKLCH (verbatim from the web theme in
// apps/web/src/styles.css) and converted to sRGB hex here, so the native
// palette is derived — never hand-transcribed — from the web source of truth.
// Wired: Android (Compose), iOS (SwiftUI), and desktop (Rust/gpui via the
// theme crate).

import { writeFileSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const repoRoot = join(__dirname, "..", "..", "..")

interface Tokens {
  palette: Record<string, string>
  semantic: Record<string, string>
  // Ordered avatar fallback hues (EXP-698 r4): key order IS the hash index,
  // so every emitter also writes the list form the hash indexes into.
  avatar: Record<string, string>
  glass: Record<string, string>
  radius: Record<string, number>
  size: Record<string, number>
  // Nested, unlike every group above: durations are integer milliseconds and
  // easings are 4-element CSS cubic-bezier control points [x1, y1, x2, y2]
  // (P0 = (0,0) and P3 = (1,1) implicit). The group's `$comment` lives one
  // level up, so both leaf maps stay cleanly typed.
  motion: {
    duration: Record<string, number>
    ease: Record<string, number[]>
  }
  type: { fontFamily: string; baseSize: number }
}

const tokens: Tokens = JSON.parse(
  readFileSync(join(__dirname, "..", "tokens.json"), "utf8")
)

// ── Color conversion ───────────────────────────────────────────────────────

interface Rgba {
  r: number
  g: number
  b: number
  a: number
} // each 0–255

function srgbGamma(linear: number): number {
  const c = Math.max(0, Math.min(1, linear))
  return c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055
}

// OKLCH → linear OKLab → linear sRGB → gamma sRGB (Björn Ottosson's matrices).
function oklchToRgb(
  L: number,
  C: number,
  hDeg: number
): [number, number, number] {
  const h = (hDeg * Math.PI) / 180
  const a = C * Math.cos(h)
  const b = C * Math.sin(h)

  const l_ = L + 0.3963377774 * a + 0.2158037573 * b
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b
  const s_ = L - 0.0894841775 * a - 1.291485548 * b

  const l = l_ * l_ * l_
  const m = m_ * m_ * m_
  const s = s_ * s_ * s_

  const r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
  const g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
  const bl = -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s

  return [srgbGamma(r), srgbGamma(g), srgbGamma(bl)]
}

function to255(unit: number): number {
  return Math.round(Math.max(0, Math.min(1, unit)) * 255)
}

function parseColor(input: string): Rgba {
  const s = input.trim()

  if (s.startsWith(`#`)) {
    const hex = s.slice(1)
    const r = parseInt(hex.slice(0, 2), 16)
    const g = parseInt(hex.slice(2, 4), 16)
    const b = parseInt(hex.slice(4, 6), 16)
    const a = hex.length >= 8 ? parseInt(hex.slice(6, 8), 16) : 255
    return { r, g, b, a }
  }

  const m = s.match(
    /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)(%?)\s*)?\)$/
  )
  if (!m) throw new Error(`Unparseable color: ${input}`)
  const L = parseFloat(m[1])
  const C = parseFloat(m[2])
  const H = parseFloat(m[3])
  let a = 255
  if (m[4] !== undefined) {
    const raw = parseFloat(m[4])
    a = to255(m[5] === `%` ? raw / 100 : raw)
  }
  const [r, g, b] = oklchToRgb(L, C, H)
  return { r: to255(r), g: to255(g), b: to255(b), a }
}

function byte(n: number): string {
  return n.toString(16).toUpperCase().padStart(2, `0`)
}

// Compose Color literal: 0xAARRGGBB.
function kotlinColor(input: string): string {
  const { r, g, b, a } = parseColor(input)
  return `Color(0x${byte(a)}${byte(r)}${byte(g)}${byte(b)})`
}

// SwiftUI Color literal: 0–1 component doubles (NOT 0–255), opacity always
// included so the alpha-carrying palette colors (border/input/sidebarBorder)
// round-trip.
function swiftColor(input: string): string {
  const { r, g, b, a } = parseColor(input)
  const f = (n: number): string => (n / 255).toFixed(4)
  return `Color(red: ${f(r)}, green: ${f(g)}, blue: ${f(b)}, opacity: ${f(a)})`
}

// Rust `Srgb8` literal (explicit named bytes — sidesteps gpui's
// rgb(0xRRGGBB) vs rgba(0xRRGGBBAA) byte-order hazard; the hand-written
// struct lives in apps/desktop/crates/theme/src/lib.rs).
function rustSrgb8(name: string, input: string): string {
  const { r, g, b, a } = parseColor(input)
  return `pub const ${screamingSnake(name)}: Srgb8 = Srgb8 { r: ${r}, g: ${g}, b: ${b}, a: ${a} };`
}

function rustF32(name: string, v: number): string {
  // always a float literal so it's f32-typed
  return `pub const ${screamingSnake(name)}: f32 = ${decimal(v)};`
}

// ── Motion helpers (EXP-523) ─────────────────────────────────────────────────

// Always a DECIMAL literal. `0` would be an Int in Kotlin and an integer
// literal in Rust, and neither coerces where a Float/f32 is wanted.
function decimal(n: number): string {
  return Number.isInteger(n) ? n.toFixed(1) : `${n}`
}

const kotlinFloat = (n: number): string => `${decimal(n)}f`
const swiftDouble = (n: number): string => decimal(n)
const rustFloat = (n: number): string => decimal(n)

// Easing control points are the ONE cross-platform form: [x1, y1, x2, y2],
// with P0 = (0,0) and P3 = (1,1) implicit. Every target takes the same four
// numbers in the same order — only the literal syntax differs.
function bezier(v: number[], fmt: (n: number) => string): string {
  if (v.length !== 4) {
    throw new Error(
      `Easing must be 4 cubic-bezier control points, got ${v.length}: ${JSON.stringify(v)}`
    )
  }
  return v.map(fmt).join(`, `)
}

// tokens.json stores integer milliseconds; SwiftUI's unit is seconds.
// `toFixed(3)` rather than a bare `v / 1000` so the literal is deterministic
// and never float-print-dependent.
function swiftSeconds(ms: number): string {
  return (ms / 1000).toFixed(3)
}

// The motion leaves have no `$comment` of their own (it lives on the group),
// but filter anyway so the emitters stay uniform with the color/dim groups.
function motionEntries<T>(group: Record<string, T>): [string, T][] {
  return Object.entries(group).filter(([k]) => !k.startsWith(`$`))
}

// ── Emit helpers ─────────────────────────────────────────────────────────────

function pascalCase(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function screamingSnake(s: string): string {
  return s
    .replace(/([a-z0-9])([A-Z])/g, `$1_$2`) // camelCase word breaks
    .replace(/[^A-Za-z0-9]+/g, `_`) // non-alnum → _
    .toUpperCase()
}

const HEADER_KOTLIN = `// AUTO-GENERATED by packages/design-tokens/scripts/generate.ts — do not edit.
// Single source of truth: packages/design-tokens/tokens.json.
`

function emitKotlin(): string {
  const palette = Object.entries(tokens.palette)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        val ${pascalCase(k)}: Color = ${kotlinColor(v)}`)
    .join(`\n`)
  const semantic = Object.entries(tokens.semantic)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        val ${pascalCase(k)}: Color = ${kotlinColor(v)}`)
    .join(`\n`)
  const avatarEntries = Object.entries(tokens.avatar).filter(
    ([k]) => !k.startsWith(`$`)
  )
  const avatar = avatarEntries
    .map(([k, v]) => `        val ${pascalCase(k)}: Color = ${kotlinColor(v)}`)
    .join(`\n`)
  const avatarList = avatarEntries.map(([k]) => pascalCase(k)).join(`, `)
  const glass = Object.entries(tokens.glass)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        val ${pascalCase(k)}: Color = ${kotlinColor(v)}`)
    .join(`\n`)
  const radius = Object.entries(tokens.radius)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        val ${pascalCase(k)}: Dp = ${v}.dp`)
    .join(`\n`)
  const size = Object.entries(tokens.size)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        val ${pascalCase(k)}: Dp = ${v}.dp`)
    .join(`\n`)

  const motionDuration = motionEntries(tokens.motion.duration)
    .map(([k, v]) => `            const val ${pascalCase(k)}: Int = ${v}`)
    .join(`\n`)
  const motionEase = motionEntries(tokens.motion.ease)
    .map(
      ([k, v]) =>
        `            val ${pascalCase(k)}: Easing = CubicBezierEasing(${bezier(v, kotlinFloat)})`
    )
    .join(`\n`)

  return `${HEADER_KOTLIN}
package com.exponential.app.ui.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

object DesignTokens {
    // Forced-dark surface palette, derived from the web OKLCH theme.
    object Palette {
${palette}
    }

    // Fixed brand accents (status / priority / due-date).
    object Semantic {
${semantic}
    }

    // Avatar fallback hues (EXP-698 r4) — index = fnv1a32(userId) % Hues.size,
    // see ui/components/Avatars.kt. Order is the contract; never reorder.
    object Avatar {
${avatar}
        val Hues: List<Color> = listOf(${avatarList})
    }

    // Glass surfaces (EXP-269) — read through the ui/theme/Glass.kt GlassTokens
    // aliases; the styleguide Components group renders the same values (EXP-698).
    object Glass {
${glass}
    }

    // Corner radii (px ≡ dp), matching the web rounded-* scale.
    object Radius {
${radius}
    }

    // Control geometry, matching the web control heights.
    object Size {
${size}
    }

    // Motion (EXP-523) — durations in MILLISECONDS (Compose's \`tween\` unit),
    // easings as CSS cubic-bezier control points. Read these through
    // ui/theme/Motion.kt, which collapses them to \`snap()\` when the OS has
    // animations turned off; never call \`tween(…)\` with a literal.
    object Motion {
        object Duration {
${motionDuration}
        }

        object Ease {
${motionEase}
        }
    }
}
`
}

const HEADER_SWIFT = `// AUTO-GENERATED by packages/design-tokens/scripts/generate.ts — do not edit.
// Single source of truth: packages/design-tokens/tokens.json.
`

function emitSwift(): string {
  // Swift keeps the raw lowerCamel token keys (so call sites read
  // DesignTokens.Semantic.green); Android pascal-cases them for Compose.
  const palette = Object.entries(tokens.palette)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        public static let ${k}: Color = ${swiftColor(v)}`)
    .join(`\n`)
  const semantic = Object.entries(tokens.semantic)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        public static let ${k}: Color = ${swiftColor(v)}`)
    .join(`\n`)
  const avatarEntries = Object.entries(tokens.avatar).filter(
    ([k]) => !k.startsWith(`$`)
  )
  const avatar = avatarEntries
    .map(([k, v]) => `        public static let ${k}: Color = ${swiftColor(v)}`)
    .join(`\n`)
  const avatarList = avatarEntries.map(([k]) => k).join(`, `)
  const glass = Object.entries(tokens.glass)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        public static let ${k}: Color = ${swiftColor(v)}`)
    .join(`\n`)
  const radius = Object.entries(tokens.radius)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        public static let ${k}: CGFloat = ${v}`)
    .join(`\n`)
  const size = Object.entries(tokens.size)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `        public static let ${k}: CGFloat = ${v}`)
    .join(`\n`)
  const motionDuration = motionEntries(tokens.motion.duration)
    .map(
      ([k, v]) =>
        `            public static let ${k}: TimeInterval = ${swiftSeconds(v)}`
    )
    .join(`\n`)
  const motionEase = motionEntries(tokens.motion.ease)
    .map(([k, v]) => {
      const [x1, y1, x2, y2] = v.map(swiftDouble)
      return `            public static let ${k} = BezierCurve(x1: ${x1}, y1: ${y1}, x2: ${x2}, y2: ${y2})`
    })
    .join(`\n`)

  return `${HEADER_SWIFT}
import SwiftUI

public enum DesignTokens {
    // Forced-dark surface palette, derived from the web OKLCH theme.
    public enum Palette {
${palette}
    }

    // Fixed brand accents (status / priority / due-date).
    public enum Semantic {
${semantic}
    }

    // Avatar fallback hues (EXP-698 r4) — index = fnv1a32(userId) % hues.count,
    // see ExpCore AvatarColor.swift. Order is the contract; never reorder.
    public enum Avatar {
${avatar}
        public static let hues: [Color] = [${avatarList}]
    }

    // Glass surfaces (EXP-269) — read through ExpUI GlassTokens.swift; the
    // styleguide Components group renders the same values (EXP-698).
    public enum Glass {
${glass}
    }

    // Corner radii (px ≡ pt), matching the web rounded-* scale.
    public enum Radius {
${radius}
    }

    // Control geometry, matching the web control heights.
    public enum Size {
${size}
    }

    // Motion (EXP-523) — durations in SECONDS (SwiftUI's unit; tokens.json
    // stores integer milliseconds), easings as CSS cubic-bezier control
    // points. \`BezierCurve\` is hand-written in ExpUI/Sources/Motion.swift,
    // the same arrangement as the desktop's hand-written \`Srgb8\`. Read these
    // through \`@Environment(\\.motion)\`, which returns nil under Reduce Motion.
    public enum Motion {
        public enum Duration {
${motionDuration}
        }

        public enum Ease {
${motionEase}
        }
    }
}
`
}

const HEADER_RUST = `// AUTO-GENERATED by packages/design-tokens/scripts/generate.ts — do not edit.
// Single source of truth: packages/design-tokens/tokens.json.
`

// Rust keeps the tokens flat at the module top level (consumers read
// tokens::BACKGROUND); the palette/semantic split is documentary only.
// Radii and sizes land in `radius`/`size` sub-modules so their short keys
// can never collide with color token names.
function emitRust(): string {
  const palette = Object.entries(tokens.palette)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => rustSrgb8(k, v))
    .join(`\n`)
  const semantic = Object.entries(tokens.semantic)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => rustSrgb8(k, v))
    .join(`\n`)
  const avatarEntries = Object.entries(tokens.avatar).filter(
    ([k]) => !k.startsWith(`$`)
  )
  const avatar = avatarEntries
    .map(([k, v]) => `    ${rustSrgb8(k, v)}`)
    .join(`\n`)
  const avatarList = avatarEntries.map(([k]) => screamingSnake(k)).join(`, `)
  const glass = Object.entries(tokens.glass)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `    ${rustSrgb8(k, v)}`)
    .join(`\n`)
  const radius = Object.entries(tokens.radius)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `    ${rustF32(k, v)}`)
    .join(`\n`)
  const size = Object.entries(tokens.size)
    .filter(([k]) => !k.startsWith(`$`))
    .map(([k, v]) => `    ${rustF32(k, v)}`)
    .join(`\n`)
  const motionDuration = motionEntries(tokens.motion.duration)
    .map(([k, v]) => `        pub const ${screamingSnake(k)}_MS: u64 = ${v};`)
    .join(`\n`)
  const motionEase = motionEntries(tokens.motion.ease)
    .map(
      ([k, v]) =>
        `        pub const ${screamingSnake(k)}: [f32; 4] = [${bezier(v, rustFloat)}];`
    )
    .join(`\n`)

  return `${HEADER_RUST}use crate::Srgb8;

// Forced-dark surface palette, derived from the web OKLCH theme.
${palette}

// Fixed brand accents (status / priority / due-date).
${semantic}

// Avatar fallback hues (EXP-698 r4) — index = fnv1a32(user_id) % HUES.len(),
// see crates/ui/src/user_avatar.rs. Order is the contract; never reorder.
pub mod avatar {
    use crate::Srgb8;
${avatar}
    pub const HUES: [Srgb8; ${avatarEntries.length}] = [${avatarList}];
}

// Glass surfaces (EXP-269) — the mobile GlassTheme transcription. The nested
// module keeps the short fill/stroke keys from colliding with palette names.
pub mod glass {
    use crate::Srgb8;
${glass}
}

// Corner radii in px, matching the web rounded-* scale.
pub mod radius {
${radius}
}

// Control geometry in px, matching the web control heights.
pub mod size {
${size}
}

// Motion (EXP-523) — durations in milliseconds (u64, so \`Duration::from_millis\`
// takes them verbatim), easings as CSS cubic-bezier control points. Read these
// through \`theme::motion\`, which wraps the millis in \`Duration\` and SOLVES the
// curve for x. Do NOT hand them to \`gpui_component::animation::cubic_bezier\`:
// that helper evaluates y over the RAW progress and throws its own x away
// (\`let _x = …\`), which is a different curve from CSS / SwiftUI / Compose.
pub mod motion {
    pub mod duration {
${motionDuration}
    }

    pub mod ease {
${motionEase}
    }
}
`
}

const kotlinPath = join(
  repoRoot,
  `apps/android/app/src/main/java/com/exponential/app/ui/theme/DesignTokens.generated.kt`
)

writeFileSync(kotlinPath, emitKotlin())

// eslint-disable-next-line no-console
console.log(`design-tokens: wrote ${kotlinPath}`)

const swiftPath = join(
  repoRoot,
  `apps/ios/ExpUI/Sources/DesignTokens.generated.swift`
)

writeFileSync(swiftPath, emitSwift())

// eslint-disable-next-line no-console
console.log(`design-tokens: wrote ${swiftPath}`)

const rustPath = join(
  repoRoot,
  `apps/desktop/crates/theme/src/tokens.generated.rs`
)

writeFileSync(rustPath, emitRust())

// eslint-disable-next-line no-console
console.log(`design-tokens: wrote ${rustPath}`)
