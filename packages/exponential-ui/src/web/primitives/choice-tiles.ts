// EXP-1280: picture choice tiles. The pure half (the option shape, the
// pick arithmetic) and the app-token skin of `ChoiceTiles`; the headless
// frame lives with each renderer.

export interface ChoiceTileOption {
  value: string
  label: string
  description?: string | null
  /** The picture's url (a relative `/api/attachments/{id}` resolves in the
   *  app through `renderImage`). */
  image?: string | null
  disabled?: boolean
}

/** `columns` clamped to 1–4 (default 2). */
export function choiceColumns(columns: unknown): number {
  const n = typeof columns === `number` && Number.isFinite(columns) ? Math.round(columns) : 2
  return Math.min(4, Math.max(1, n))
}

/** The values a comma-joined eUI `value` picks (single: the whole string). */
export function choiceValues(value: unknown, multiple: boolean): string[] {
  if (Array.isArray(value)) return value.map(String).filter(Boolean)
  const raw = value === undefined || value === null ? `` : String(value)
  if (!multiple) return raw ? [raw] : []
  return raw
    .split(`,`)
    .map((s) => s.trim())
    .filter(Boolean)
}

/** A press on `value`: single picks it; multiple toggles it, the list kept in
 *  OPTION order (fixtures/interactions-round1.json `choiceTiles`). */
export function choiceToggle(picked: readonly string[], value: string, multiple: boolean, order: readonly string[]): string[] {
  if (!multiple) return [value]
  const next = picked.includes(value) ? picked.filter((v) => v !== value) : [...picked, value]
  const rank = (v: string) => {
    const i = order.indexOf(v)
    return i < 0 ? Number.MAX_SAFE_INTEGER : i
  }
  return next.slice().sort((a, b) => rank(a) - rank(b))
}

export interface TileState {
  index: number
  selected: boolean
  disabled: boolean
}

export const CHOICE_TILES_CLASSES = {
  root: `gap-[8px]`,
  // The tile in PX, never rem steps (the app's desktop rem is 18.5px, which
  // would inflate them): THE ChoiceTiles recipe = the exponential theme's
  // recipes.ChoiceTiles = packages/domain-contract/fixtures/plan-card.json
  // `tiles` (the SDK imports nothing from the app, so the numbers are copied
  // here): padding 8 (7 when selected, under the 2px `selectedRing`), `gap` 8
  // between tiles, `radius` 10, `pictureRadius` 6 at `pictureAspect` 1.6,
  // `stroke` 1, `placeholderGlyph` 20 at 0.6, `labelGap` 4 picture→label and
  // label→description, `inertOpacity` 0.5. Text stays the sm/xs steps.
  tile: `flex min-w-0 cursor-pointer flex-col gap-[4px] rounded-[10px] border-[1px] border-border bg-card p-[8px] text-start outline-none transition-colors hover:bg-secondary focus-visible:border-ring disabled:cursor-not-allowed disabled:opacity-50`,
  tileSelected: `border-[2px] border-primary bg-accent p-[7px] hover:bg-accent`,
  tileInvalid: `border-destructive`,
  picture: `relative flex aspect-[1.6] w-full shrink-0 items-center justify-center overflow-hidden rounded-[6px] bg-muted text-muted-foreground`,
  image: `absolute inset-0 size-full object-cover`,
  placeholder: `size-[20px] opacity-60`,
  label: `min-w-0 text-sm leading-5 font-medium text-foreground`,
  description: `min-w-0 text-xs leading-4 text-muted-foreground`,
} as const
