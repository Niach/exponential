import * as React from "react"
import { ImageIcon } from "lucide-react"

import { cn } from "./cn"

// EXP-1280: picture choice tiles, ONE implementation for two skins.
//
// `ChoiceTilesFrame` is the headless half: the grid, one button per option
// (radio tiles for a single pick, checkbox tiles for several), the roving tab
// stop and the keys of catalog/a11y.json `ChoiceTiles`. The eUI native
// (`natives/choice-tiles.tsx`) dresses it with the theme's recipe classes;
// `ChoiceTiles` below dresses it with the app's Tailwind tokens at the SAME
// geometry, so the plan card and an agent's surface draw the same tile.

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

type Attrs = Record<string, unknown>

export interface ChoiceTilesFrameProps {
  options: readonly ChoiceTileOption[]
  /** The picked values. */
  picked: readonly string[]
  multiple: boolean
  columns: number
  disabled?: boolean
  invalid?: boolean
  dir?: `ltr` | `rtl`
  /** A pick (already toggled for `multiple`): the next picked values. */
  onPick: (next: string[], value: string) => void
  rootProps?: Attrs
  tileProps: (state: TileState) => Attrs
  pictureProps: (state: TileState) => Attrs
  labelProps: (state: TileState) => Attrs
  descriptionProps: (state: TileState) => Attrs
  /** The picture's content (the frame keeps the box; a missing picture gets
   *  the placeholder). */
  renderPicture: (src: string | null, state: TileState) => React.ReactNode
  /** Under the tiles (the field's errors), spanning every column. */
  after?: React.ReactNode
  rootRef?: React.Ref<HTMLDivElement>
}

/** The headless tiles: a `radiogroup` of `radio` buttons (single) or a
 *  `group` of `checkbox` buttons (multiple). The picked tile (else the
 *  first enabled one) is the tab stop; arrows move focus with wrap (a single
 *  pick follows focus); Space/Enter toggle (the button's own click). Every
 *  tile has a picture box once ANY option has an image, so a row keeps one
 *  height. */
export function ChoiceTilesFrame({ options, picked, multiple, columns, disabled = false, invalid = false, dir = `ltr`, onPick, rootProps, tileProps, pictureProps, labelProps, descriptionProps, renderPicture, after, rootRef }: ChoiceTilesFrameProps) {
  const buttons = React.useRef<(HTMLButtonElement | null)[]>([])
  const order = React.useMemo(() => options.map((o) => o.value), [options])
  const pictures = options.some((o) => Boolean(o.image))
  const enabled = (i: number) => !disabled && !options[i]?.disabled
  const firstPicked = options.findIndex((o, i) => picked.includes(o.value) && enabled(i))
  const tabStop = firstPicked >= 0 ? firstPicked : options.findIndex((_, i) => enabled(i))
  const pick = (i: number) => {
    const value = options[i]!.value
    onPick(choiceToggle(picked, value, multiple, order), value)
  }
  const move = (from: number, delta: number) => {
    const n = options.length
    for (let step = 1; step <= n; step++) {
      const i = (((from + delta * step) % n) + n) % n
      if (enabled(i)) return i
    }
    return -1
  }
  const onKeyDown = (i: number) => (e: React.KeyboardEvent<HTMLButtonElement>) => {
    const forward = dir === `rtl` ? `ArrowLeft` : `ArrowRight`
    const back = dir === `rtl` ? `ArrowRight` : `ArrowLeft`
    const delta = e.key === forward || e.key === `ArrowDown` ? 1 : e.key === back || e.key === `ArrowUp` ? -1 : 0
    if (delta === 0) return
    e.preventDefault()
    const next = move(i, delta)
    if (next < 0) return
    buttons.current[next]?.focus()
    if (!multiple && !picked.includes(options[next]!.value)) pick(next)
  }
  return (
    <div
      ref={rootRef}
      role={multiple ? `group` : `radiogroup`}
      aria-invalid={invalid || undefined}
      aria-disabled={disabled || undefined}
      dir={dir}
      {...rootProps}
      style={{ display: `grid`, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`, ...((rootProps?.style as React.CSSProperties | undefined) ?? {}) }}
    >
      {options.map((o, i) => {
        const state: TileState = { index: i, selected: picked.includes(o.value), disabled: !enabled(i) }
        const description = o.description ? String(o.description) : ``
        return (
          <button
            key={`${o.value}-${i}`}
            ref={(el) => {
              buttons.current[i] = el
            }}
            type="button"
            role={multiple ? `checkbox` : `radio`}
            aria-checked={state.selected}
            aria-label={o.label}
            disabled={state.disabled}
            tabIndex={i === tabStop ? 0 : -1}
            data-state={state.selected ? `checked` : `unchecked`}
            data-value={o.value}
            onClick={() => pick(i)}
            onKeyDown={onKeyDown(i)}
            {...tileProps(state)}
          >
            {pictures ? (
              <span aria-hidden="true" {...pictureProps(state)}>
                {renderPicture(o.image ? String(o.image) : null, state)}
              </span>
            ) : null}
            <span {...labelProps(state)}>{o.label}</span>
            {description ? <span {...descriptionProps(state)}>{description}</span> : null}
          </button>
        )
      })}
      {after}
    </div>
  )
}

export interface ChoiceTilesProps {
  options: readonly ChoiceTileOption[]
  /** The pick: a string (single) or the picked values (multiple). */
  value: string | readonly string[] | null | undefined
  multiple?: boolean
  /** Tiles per row, 1–4 (default 2). */
  columns?: number
  disabled?: boolean
  /** Paints the destructive border (a required answer missing). */
  invalid?: boolean
  /** The next value: a string for a single pick, the values for `multiple`. */
  onChange?: (value: string | string[]) => void
  /** The picture element (the app passes an authorised <img> for
   *  `/api/attachments/{id}`); default a plain lazy <img>. */
  renderImage?: (src: string, alt: string) => React.ReactNode
  className?: string
  "aria-label"?: string
  "aria-labelledby"?: string
  "data-testid"?: string
  dir?: `ltr` | `rtl`
}

// The tile in PX, never rem steps (the app's desktop rem is 18.5px, which
// would inflate them): THE ChoiceTiles recipe = the exponential theme's
// recipes.ChoiceTiles = packages/domain-contract/fixtures/plan-card.json
// `tiles` (the SDK imports nothing from the app, so the numbers are copied
// here): padding 8 (7 when selected, under the 2px `selectedRing`), `gap` 8
// between tiles, `radius` 10, `pictureRadius` 6 at `pictureAspect` 1.6,
// `stroke` 1, `placeholderGlyph` 20 at 0.6, `labelGap` 4 picture→label and
// label→description, `inertOpacity` 0.5. Text stays the sm/xs steps.
const TILE = `flex min-w-0 cursor-pointer flex-col gap-[4px] rounded-[10px] border-[1px] border-border bg-card p-[8px] text-start outline-none transition-colors hover:bg-secondary focus-visible:border-ring disabled:cursor-not-allowed disabled:opacity-50`
const TILE_SELECTED = `border-[2px] border-primary bg-accent p-[7px] hover:bg-accent`
const TILE_INVALID = `border-destructive`
const PICTURE = `relative flex aspect-[1.6] w-full shrink-0 items-center justify-center overflow-hidden rounded-[6px] bg-muted text-muted-foreground`

/** THE picture choice (EXP-1280): the plan card's question blocks and the
 *  standalone ask, at the eUI `ChoiceTiles` look. */
function ChoiceTiles({ options, value, multiple = false, columns, disabled = false, invalid = false, onChange, renderImage, className, dir, ...rest }: ChoiceTilesProps) {
  const picked = choiceValues(value, multiple)
  const image = (src: string | null, label: string) =>
    src ? (renderImage ? renderImage(src, label) : <img src={src} alt={label} loading="lazy" decoding="async" className="absolute inset-0 size-full object-cover" />) : <ImageIcon className="size-[20px] opacity-60" />
  return (
    <ChoiceTilesFrame
      options={options}
      picked={picked}
      multiple={multiple}
      columns={choiceColumns(columns)}
      disabled={disabled}
      invalid={invalid}
      dir={dir}
      onPick={(next) => onChange?.(multiple ? next : (next[0] ?? ``))}
      rootProps={{ "data-slot": `choice-tiles`, className: cn(`gap-[8px]`, className), "aria-label": rest[`aria-label`], "aria-labelledby": rest[`aria-labelledby`], "data-testid": rest[`data-testid`] }}
      tileProps={(s) => ({ "data-slot": `choice-tile`, className: cn(TILE, s.selected && TILE_SELECTED, invalid && !s.selected && TILE_INVALID) })}
      pictureProps={() => ({ "data-slot": `choice-tile-picture`, className: PICTURE })}
      labelProps={() => ({ className: `min-w-0 text-sm leading-5 font-medium text-foreground` })}
      descriptionProps={() => ({ className: `min-w-0 text-xs leading-4 text-muted-foreground` })}
      renderPicture={(src, s) => image(src, options[s.index]?.label ?? ``)}
    />
  )
}

export { ChoiceTiles }
