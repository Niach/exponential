// VAPP-92: the shapes of a THEME. A theme is data, never code: one JSON file
// (`catalog/theme.schema.json` validates it) of token VALUES per mode plus
// component RECIPES, loaded at runtime by every renderer. `ThemeSource` is
// the file; `ResolvedTheme` is what `loadTheme` hands a painter: the
// `extends` chain flattened, every token present, recipes merged.

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

/** The per-mode values: colours and shadows (shadows differ in the dark). */
export interface ThemeMode {
  color: Record<string, HexColor>
  shadow: Record<string, Shadow[]>
}
export type ModeName = `light` | `dark`

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
  borderRadius?: number | string
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
  boxShadow?: string
  opacity?: number | string
  /** The painter may use the platform's own control for this part. */
  native?: boolean
}

export type RecipeWhenValue = string | number | boolean | (string | number | boolean)[]

/** One rule: applies when every `when` entry matches (`state` = every listed
 *  state is active; any other key = the recipe prop equals the value or is
 *  in the list). Rules merge in order, later wins; a child theme's rules come
 *  after its parent's. No `when` = the part's base. */
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
 *  numbers, shadows as lists, families as names. */
export type ResolvedValue = number | string | boolean | Shadow[]
export type ResolvedStyle = Record<string, ResolvedValue>

export interface ThemeIssue {
  path: string
  message: string
}
