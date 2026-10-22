import { For, Show, createMemo, type JSX } from "solid-js"
import { ImageIcon } from "lucide-solid"
import { CHOICE_TILES_CLASSES, choiceColumns, choiceToggle, choiceValues, type ChoiceTileOption, type TileState } from "@exponential-at/ui/web/primitives"

import { PaintIcon } from "../icons"
import { css, type StyleInput } from "../style"
import { cn } from "./cn"

export { choiceColumns, choiceToggle, choiceValues } from "@exponential-at/ui/web/primitives"
export type { ChoiceTileOption, TileState } from "@exponential-at/ui/web/primitives"

// EXP-1280: picture choice tiles, React's `ChoiceTilesFrame` one to one (the
// grid, one button per option, the roving tab stop, the keys of
// catalog/a11y.json `ChoiceTiles`) and the `ChoiceTiles` skin.

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
  /** The root's attributes; `style` is a React-form object (camelCase). */
  rootProps?: Attrs
  tileProps: (state: TileState) => Attrs
  pictureProps: (state: TileState) => Attrs
  labelProps: (state: TileState) => Attrs
  descriptionProps: (state: TileState) => Attrs
  /** The picture's content (the frame keeps the box; a missing picture gets
   *  the placeholder). */
  renderPicture: (src: string | null, state: TileState) => JSX.Element
  /** Under the tiles (the field's errors), spanning every column. */
  after?: JSX.Element
  rootRef?: (el: HTMLDivElement) => void
}

/** The headless tiles: a `radiogroup` of `radio` buttons (single) or a
 *  `group` of `checkbox` buttons (multiple). The picked tile (else the
 *  first enabled one) is the tab stop; arrows move focus with wrap (a single
 *  pick follows focus); Space/Enter toggle (the button's own click). Every
 *  tile has a picture box once ANY option has an image, so a row keeps one
 *  height. */
export function ChoiceTilesFrame(props: ChoiceTilesFrameProps) {
  const buttons: (HTMLButtonElement | undefined)[] = []
  const order = createMemo(() => props.options.map((o) => o.value))
  const pictures = () => props.options.some((o) => Boolean(o.image))
  const enabled = (i: number) => !props.disabled && !props.options[i]?.disabled
  const tabStop = createMemo(() => {
    const firstPicked = props.options.findIndex((o, i) => props.picked.includes(o.value) && enabled(i))
    return firstPicked >= 0 ? firstPicked : props.options.findIndex((_, i) => enabled(i))
  })
  const pick = (i: number) => {
    const value = props.options[i]!.value
    props.onPick(choiceToggle(props.picked, value, props.multiple, order()), value)
  }
  const move = (from: number, delta: number) => {
    const n = props.options.length
    for (let step = 1; step <= n; step++) {
      const i = (((from + delta * step) % n) + n) % n
      if (enabled(i)) return i
    }
    return -1
  }
  const onKeyDown = (i: number, e: KeyboardEvent) => {
    const rtl = (props.dir ?? `ltr`) === `rtl`
    const forward = rtl ? `ArrowLeft` : `ArrowRight`
    const back = rtl ? `ArrowRight` : `ArrowLeft`
    const delta = e.key === forward || e.key === `ArrowDown` ? 1 : e.key === back || e.key === `ArrowUp` ? -1 : 0
    if (delta === 0) return
    e.preventDefault()
    const next = move(i, delta)
    if (next < 0) return
    buttons[next]?.focus()
    if (!props.multiple && !props.picked.includes(props.options[next]!.value)) pick(next)
  }
  const rootStyle = () => {
    const own = (props.rootProps?.style as StyleInput | undefined) ?? {}
    return css({ display: `grid`, gridTemplateColumns: `repeat(${props.columns}, minmax(0, 1fr))`, ...own })
  }
  return (
    <div
      ref={props.rootRef}
      role={props.multiple ? `group` : `radiogroup`}
      aria-invalid={props.invalid || undefined}
      aria-disabled={props.disabled || undefined}
      dir={props.dir ?? `ltr`}
      {...props.rootProps}
      style={rootStyle()}
    >
      <For each={props.options}>
        {(o, index) => {
          const i = index()
          const state = (): TileState => ({ index: i, selected: props.picked.includes(o.value), disabled: !enabled(i) })
          const description = o.description ? String(o.description) : ``
          return (
            <button
              ref={(el) => {
                buttons[i] = el
              }}
              type="button"
              role={props.multiple ? `checkbox` : `radio`}
              aria-checked={state().selected}
              aria-label={o.label}
              disabled={state().disabled}
              tabindex={i === tabStop() ? 0 : -1}
              data-state={state().selected ? `checked` : `unchecked`}
              data-value={o.value}
              onClick={() => pick(i)}
              onKeyDown={(e) => onKeyDown(i, e)}
              {...props.tileProps(state())}
            >
              <Show when={pictures()}>
                <span aria-hidden="true" {...props.pictureProps(state())}>
                  {props.renderPicture(o.image ? String(o.image) : null, state())}
                </span>
              </Show>
              <span {...props.labelProps(state())}>{o.label}</span>
              <Show when={description}>
                <span {...props.descriptionProps(state())}>{description}</span>
              </Show>
            </button>
          )
        }}
      </For>
      {props.after}
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
  /** The picture element; default a plain lazy <img>. */
  renderImage?: (src: string, alt: string) => JSX.Element
  class?: string
  "aria-label"?: string
  "aria-labelledby"?: string
  "data-testid"?: string
  dir?: `ltr` | `rtl`
}

/** THE picture choice (EXP-1280) at the eUI `ChoiceTiles` look. */
function ChoiceTiles(props: ChoiceTilesProps) {
  const multiple = () => props.multiple ?? false
  const picked = createMemo(() => choiceValues(props.value, multiple()))
  const image = (src: string | null, label: string) =>
    src ? (props.renderImage ? props.renderImage(src, label) : <img src={src} alt={label} loading="lazy" decoding="async" class={CHOICE_TILES_CLASSES.image} />) : <PaintIcon icon={ImageIcon} class={CHOICE_TILES_CLASSES.placeholder} />
  return (
    <ChoiceTilesFrame
      options={props.options}
      picked={picked()}
      multiple={multiple()}
      columns={choiceColumns(props.columns)}
      disabled={props.disabled ?? false}
      invalid={props.invalid ?? false}
      dir={props.dir}
      onPick={(next) => props.onChange?.(multiple() ? next : (next[0] ?? ``))}
      rootProps={{ "data-slot": `choice-tiles`, class: cn(CHOICE_TILES_CLASSES.root, props.class), "aria-label": props[`aria-label`], "aria-labelledby": props[`aria-labelledby`], "data-testid": props[`data-testid`] }}
      tileProps={(s) => ({ "data-slot": `choice-tile`, class: cn(CHOICE_TILES_CLASSES.tile, s.selected && CHOICE_TILES_CLASSES.tileSelected, props.invalid && !s.selected && CHOICE_TILES_CLASSES.tileInvalid) })}
      pictureProps={() => ({ "data-slot": `choice-tile-picture`, class: CHOICE_TILES_CLASSES.picture })}
      labelProps={() => ({ class: CHOICE_TILES_CLASSES.label })}
      descriptionProps={() => ({ class: CHOICE_TILES_CLASSES.description })}
      renderPicture={(src, s) => image(src, props.options[s.index]?.label ?? ``)}
    />
  )
}

export { ChoiceTiles }
