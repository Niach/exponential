import * as React from "react"
import { ImageIcon } from "lucide-react"
import { CHOICE_TILES_CLASSES, choiceColumns, choiceToggle, choiceValues, type ChoiceTileOption, type TileState } from "@exponential_app/ui/web/primitives"

import { cn } from "./cn"

export { choiceColumns, choiceToggle, choiceValues } from "@exponential_app/ui/web/primitives"
export type { ChoiceTileOption, TileState } from "@exponential_app/ui/web/primitives"

// EXP-1280: picture choice tiles, ONE implementation for two skins.
//
// `ChoiceTilesFrame` is the headless half: the grid, one button per option
// (radio tiles for a single pick, checkbox tiles for several), the roving tab
// stop and the keys of catalog/a11y.json `ChoiceTiles`. The eUI native
// (`natives/choice-tiles.tsx`) dresses it with the theme's recipe classes;
// `ChoiceTiles` below dresses it with the app's Tailwind tokens at the SAME
// geometry, so the plan card and an agent's surface draw the same tile.

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

/** THE picture choice (EXP-1280): the plan card's question blocks and the
 *  standalone ask, at the eUI `ChoiceTiles` look. */
function ChoiceTiles({ options, value, multiple = false, columns, disabled = false, invalid = false, onChange, renderImage, className, dir, ...rest }: ChoiceTilesProps) {
  const picked = choiceValues(value, multiple)
  const image = (src: string | null, label: string) =>
    src ? (renderImage ? renderImage(src, label) : <img src={src} alt={label} loading="lazy" decoding="async" className={CHOICE_TILES_CLASSES.image} />) : <ImageIcon className={CHOICE_TILES_CLASSES.placeholder} />
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
      rootProps={{ "data-slot": `choice-tiles`, className: cn(CHOICE_TILES_CLASSES.root, className), "aria-label": rest[`aria-label`], "aria-labelledby": rest[`aria-labelledby`], "data-testid": rest[`data-testid`] }}
      tileProps={(s) => ({ "data-slot": `choice-tile`, className: cn(CHOICE_TILES_CLASSES.tile, s.selected && CHOICE_TILES_CLASSES.tileSelected, invalid && !s.selected && CHOICE_TILES_CLASSES.tileInvalid) })}
      pictureProps={() => ({ "data-slot": `choice-tile-picture`, className: CHOICE_TILES_CLASSES.picture })}
      labelProps={() => ({ className: CHOICE_TILES_CLASSES.label })}
      descriptionProps={() => ({ className: CHOICE_TILES_CLASSES.description })}
      renderPicture={(src, s) => image(src, options[s.index]?.label ?? ``)}
    />
  )
}

export { ChoiceTiles }
