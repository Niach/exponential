// VAPP-92: the built-in themes, compiled in. `exponential` is GENERATED from
// packages/design-tokens (the app's values) + the hand-authored glass
// recipes; `neutral` is stock shadcn (light + dark) and the ROOT the others
// extend; `playful` is the deliberately different test theme (rounded,
// colourful, another family) the conformance suite renders on every painter.

import exponentialJson from "../themes/exponential.theme.json" with { type: "json" }
import neutralJson from "../themes/neutral.theme.json" with { type: "json" }
import playfulJson from "../themes/playful.theme.json" with { type: "json" }
import { loadTheme } from "./theme"
import type { ResolvedTheme, ThemeSource } from "./theme-types"

export const neutralTheme = neutralJson as unknown as ThemeSource
export const exponentialTheme = exponentialJson as unknown as ThemeSource
export const playfulTheme = playfulJson as unknown as ThemeSource

/** Source order = resolution order: a built-in may only extend one before it. */
export const BUILTIN_THEMES: readonly ThemeSource[] = [neutralTheme, exponentialTheme, playfulTheme]
export const BUILTIN_THEME_IDS: readonly string[] = BUILTIN_THEMES.map((t) => t.id)
export const DEFAULT_THEME_ID = `exponential`

const resolved = new Map<string, ResolvedTheme>()

/** A built-in, resolved once per process. */
export function builtinTheme(id: string): ResolvedTheme {
  const cached = resolved.get(id)
  if (cached) return cached
  const source = BUILTIN_THEMES.find((t) => t.id === id)
  if (!source) throw new Error(`unknown built-in theme "${id}"; known: ${BUILTIN_THEME_IDS.join(`|`)}`)
  const theme = loadTheme(source, { themes: BUILTIN_THEMES })
  resolved.set(id, theme)
  return theme
}

/** Every built-in resolved, in source order. */
export function builtinThemes(): ResolvedTheme[] {
  return BUILTIN_THEME_IDS.map(builtinTheme)
}
