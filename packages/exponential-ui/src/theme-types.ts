// VAPP-92: the shapes of a THEME. A theme is data, never code: one JSON file
// (`catalog/theme.schema.json` validates it) of token VALUES per mode plus
// component RECIPES, loaded at runtime by every renderer. `ThemeSource` is
// the file; `ResolvedTheme` is what `loadTheme` hands a painter: the
// `extends` chain flattened, every token present, recipes merged.
//
// Round 1 (docs/round-1-contract.md §5): `breakpoint` (px), `ease` (cubic
// bezier control points) and `density` (multipliers) token groups, a
// high-contrast `contrast` overlay per mode, and the `Density`/`ModeSetting`
// surface settings a renderer applies with `applyDensity`/`applyContrast`/
// `resolveMode`.

/** `#rrggbb` or `#rrggbbaa`, lowercase. The only colour form a theme carries
 *  (the builder converts oklch/hsl/rgb on import, `src/color.ts`). */
export type HexColor = `#${string}`

export interface Shadow {
  x: number
  y: number
  blur: number
  spread: number
  color: HexColor
}

/** `cubic-bezier(x1, y1, x2, y2)` control points, P0 = (0,0) and P3 = (1,1)
 *  implicit — the one form web, Compose, SwiftUI and gpui all accept. */
export type Easing = [number, number, number, number]

/** The per-mode values: colours and shadows (shadows differ in the dark). */
export interface ThemeMode {
  color: Record<string, HexColor>
  shadow: Record<string, Shadow[]>
}
export type ModeName = `light` | `dark`
/** What a host asks for: a mode, or `system` = the platform's preference. */
export type ModeSetting = ModeName | `system`
/** The surface density; `default` = the tokens as written. */
export type Density = `compact` | `default` | `comfortable`

export interface ThemeTokens {
  spacing: Record<string, number>
  radius: Record<string, number>
  type: {
    size: Record<string, number>
    lineHeight: Record<string, number>
    weight: Record<string, number>
    /** Family NAMES (`Inter`); `fonts` describes how the host registers them. */
    family: Record<string, string>
  }
  control: Record<string, number>
  shadow?: never
  opacity: Record<string, number>
  border: Record<string, number>
  /** Milliseconds. */
  motion: Record<string, number>
  /** Surface widths in px the style conditions may name (`$breakpoint.md`). */
  breakpoint: Record<string, number>
  ease: Record<string, Easing>
  /** Multipliers over `control` and `spacing` for the non-default densities. */
  density: Record<string, number>
  /** Round 2: backdrop blur radii in px (`backdropBlur: $blur.md`). */
  blur: Record<string, number>
}

/** How a host registers a family the theme names: the files are the host's
 *  (per platform); the theme only says what to fall back to. */
export interface FontSpec {
  fallback?: string
  weights?: number[]
  /** `system` = the platform's own family of that kind, nothing to load. */
  source?: `system` | `host`
}

/** The values a recipe style may set (catalog/recipes.json `keys`): the
 *  whitelisted subset of the Box visual keys, token references allowed. */
export interface RecipeStyle {
  backgroundColor?: string
  color?: string
  borderColor?: string
  borderWidth?: number | string
  borderTopWidth?: number | string
  borderRightWidth?: number | string
  borderBottomWidth?: number | string
  borderLeftWidth?: number | string
  borderRadius?: number | string
  borderStyle?: string
  padding?: number | string
  paddingHorizontal?: number | string
  paddingVertical?: number | string
  gap?: number | string
  width?: number | string
  height?: number | string
  minWidth?: number | string
  minHeight?: number | string
  fontSize?: number | string
  fontWeight?: number | string
  lineHeight?: number | string
  fontFamily?: string
  letterSpacing?: number
  textDecoration?: string
  textTransform?: string
  fontStyle?: string
  boxShadow?: string
  opacity?: number | string
  /** `$motion.<name>`: the duration this part's changes animate with. */
  transition?: string
  /** `$ease.<name>`. */
  transitionEasing?: string
  /** `translate(…) scale(…) rotate(…)`, paint-only. */
  transform?: string
  /** Round 2: `$blur.<name>`: blur what is behind the part. */
  backdropBlur?: string
  /** Round 2: a keyframe set of catalog/style.json `animations`. */
  animation?: string
  /** The painter may use the platform's own control for this part. */
  native?: boolean
}

export type RecipeWhenValue = string | number | boolean | (string | number | boolean)[]

/** One rule: applies when every `when` entry matches (`state` = every listed
 *  state is active; any other key = the recipe prop equals the value or is
 *  in the list). Rules merge by specificity, ties in order, later wins; a
 *  child theme's rules come after its parent's. No `when` = the part's base. */
export interface RecipeRule {
  when?: Record<string, RecipeWhenValue>
  style: RecipeStyle
}

/** Component → part → rules. */
export type ThemeRecipes = Record<string, Record<string, RecipeRule[]>>

/** The theme FILE. With `extends`, every section is optional and overrides
 *  the parent's; without it, every token name needs a value. */
export interface ThemeSource {
  $schema?: string
  $comment?: string
  id: string
  name: string
  extends?: string
  modes?: Partial<Record<ModeName, Partial<ThemeMode>>>
  /** Round 1: PARTIAL colour/shadow overlays a renderer applies over the
   *  mode when the platform asks for high contrast (`applyContrast`). */
  contrast?: Partial<Record<ModeName, Partial<ThemeMode>>>
  tokens?: Partial<Omit<ThemeTokens, `type`>> & { type?: Partial<ThemeTokens[`type`]> }
  fonts?: Record<string, FontSpec>
  recipes?: ThemeRecipes
}

/** What a painter receives: complete, flat, `extends` gone. Recipes keep
 *  their token references (colours depend on the mode); `resolveRecipe`
 *  turns a part's rules into concrete values for one mode. */
export interface ResolvedTheme {
  id: string
  name: string
  /** The chain this theme was built from, root first. */
  chain: string[]
  modes: Record<ModeName, ThemeMode>
  /** The high-contrast overlays per mode (partial tables, may be empty). */
  contrast: Record<ModeName, ThemeMode>
  tokens: ThemeTokens
  fonts: Record<string, FontSpec>
  recipes: ThemeRecipes
}

/** The question a painter asks: which part of which component, with which
 *  recipe props and which interaction states. */
export interface RecipeQuery {
  component: string
  part: string
  props?: Record<string, unknown>
  states?: readonly string[]
}

/** A recipe's concrete values for one mode: colours as hex, lengths as
 *  numbers, shadows as lists, families as names, easings as 4 numbers. */
export type ResolvedValue = number | string | boolean | Shadow[] | Easing
export type ResolvedStyle = Record<string, ResolvedValue>

export interface ThemeIssue {
  path: string
  message: string
}
