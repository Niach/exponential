// FEED-128 (React's natives/choice-tiles.tsx + the headless half of
// primitives/choice-tiles.tsx, `ChoiceTilesFrame`): ChoiceTiles (EXP-1280),
// picture tiles picking one value or several. A `radiogroup` of `radio`
// buttons (single) or a `group` of `checkbox` buttons (multiple); the picked
// tile (else the first enabled one) is the tab stop; arrows move focus with
// wrap (a single pick follows focus); Space/Enter toggle (the button's own
// click). Every tile has a picture box once ANY option has an image. The
// parts wear the theme's recipe: `tile.<i>` (`selected`, `disabled`,
// `invalid`), `picture.<i>`, `label.<i>`, `description.<i>`. The value is
// bindable, comma-joined when `type: multiple`; a pick writes it and fires
// `change {value}`.
//
// The React frame is a Tailwind-dressed shadcn primitive ONLY in its app
// skin (`ChoiceTiles`); the eUI native passes the recipe part attributes,
// so the native's DOM carries no Tailwind class and needs no extra sheet.

import { createMemo, Index, Show, type Component, type JSX } from "solid-js"
import { useSurfaceContext } from "../context"
import { createField, FieldErrors } from "../form"
import { createMediaSource } from "../media"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, objects, parts, str } from "./shared"

interface ChoiceTileOption {
  value: string
  label: string
  description?: string | null
  image?: string | null
  disabled?: boolean
}

/** `columns` clamped to 1–4 (default 2). */
function choiceColumns(columns: unknown): number {
  const n = typeof columns === `number` && Number.isFinite(columns) ? Math.round(columns) : 2
  return Math.min(4, Math.max(1, n))
}

/** The values a comma-joined eUI `value` picks (single: the whole string). */
function choiceValues(value: unknown, multiple: boolean): string[] {
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
function choiceToggle(picked: readonly string[], value: string, multiple: boolean, order: readonly string[]): string[] {
  if (!multiple) return [value]
  const next = picked.includes(value) ? picked.filter((v) => v !== value) : [...picked, value]
  const rank = (v: string) => {
    const i = order.indexOf(v)
    return i < 0 ? Number.MAX_SAFE_INTEGER : i
  }
  return next.slice().sort((a, b) => rank(a) - rank(b))
}

function TilePicture(props: { src: string | null }): JSX.Element {
  const ctx = useSurfaceContext()
  const media = createMediaSource(() => ctx.host, () => props.src ?? ``)
  return (
    <Show when={props.src && media().url && !media().error} fallback={<BuiltinIcon slot="Image.fallback" />}>
      <img src={media().url} alt="" loading="lazy" decoding="async" />
    </Show>
  )
}

export function ChoiceTilesNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const multiple = () => str(props.props.type, `single`) === `multiple`
  // Tiles stay mounted across a bind pass (a pick keeps the focused tile):
  // by INDEX, the option's fields followed live.
  const options = createMemo(
    () =>
      objects<Record<string, unknown>>(props.props.options).map(
        (o): ChoiceTileOption => ({ value: str(o.value), label: str(o.label), description: o.description === undefined ? null : str(o.description), image: typeof o.image === `string` && o.image ? o.image : null, disabled: bool(o.disabled) })
      ),
    undefined,
    { equals: (a, b) => JSON.stringify(a) === JSON.stringify(b) }
  )
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, () => str(props.props.value))
  let rootEl: HTMLDivElement | undefined
  const buttons: (HTMLButtonElement | undefined)[] = []
  // The field's focus target: the tab stop (the picked tile, else the first).
  const f = createField({
    node: () => props.node,
    domId: () => props.domId,
    props: () => props.props,
    value,
    focusEl: () => (rootEl?.querySelector(`button[tabindex="0"]`) ?? rootEl?.querySelector(`button`)) as HTMLElement | null,
  })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const picked = createMemo(() => choiceValues(value(), multiple()))
  const order = createMemo(() => options().map((o) => o.value))
  const pictures = () => options().some((o) => Boolean(o.image))
  const enabled = (i: number) => !disabled() && !options()[i]?.disabled
  const tabStop = () => {
    const firstPicked = options().findIndex((o, i) => picked().includes(o.value) && enabled(i))
    return firstPicked >= 0 ? firstPicked : options().findIndex((_, i) => enabled(i))
  }
  const dir = () => (ctx.direction === `rtl` ? `rtl` : `ltr`)
  const onPick = (next: string[]) => {
    const joined = next.join(`,`)
    setValue(joined)
    f.change(joined)
    f.touch()
    void props.emit(`change`, { value: joined }, { value: joined })
  }
  const pick = (i: number) => onPick(choiceToggle(picked(), options()[i]!.value, multiple(), order()))
  const move = (from: number, delta: number) => {
    const n = options().length
    for (let step = 1; step <= n; step++) {
      const i = (((from + delta * step) % n) + n) % n
      if (enabled(i)) return i
    }
    return -1
  }
  const onKeyDown = (i: number, e: KeyboardEvent) => {
    const forward = dir() === `rtl` ? `ArrowLeft` : `ArrowRight`
    const back = dir() === `rtl` ? `ArrowRight` : `ArrowLeft`
    const delta = e.key === forward || e.key === `ArrowDown` ? 1 : e.key === back || e.key === `ArrowUp` ? -1 : 0
    if (delta === 0) return
    e.preventDefault()
    const next = move(i, delta)
    if (next < 0) return
    buttons[next]?.focus()
    if (!multiple() && !picked().includes(options()[next]!.value)) pick(next)
  }
  const rootRest = createMemo(() => {
    const { class: rootClass, style: _style, ...rest } = props.rootProps
    return { ...rest, class: `${str(rootClass)} xui-choice-tiles`, "aria-describedby": f.describedBy }
  })
  const style = () => ({ display: `grid`, "grid-template-columns": `repeat(${choiceColumns(props.props.columns)}, minmax(0, 1fr))`, ...((props.rootProps.style as Record<string, string> | undefined) ?? {}) })
  const states = (selected: boolean, off: boolean) => [selected && `selected`, off && `disabled`, f.invalid && `invalid`] as const
  return (
    <div ref={rootEl} role={multiple() ? `group` : `radiogroup`} aria-invalid={f.invalid || undefined} aria-disabled={disabled() || undefined} dir={dir()} {...rootRest()} style={style()}>
      <Index each={options()}>
        {(opt, index) => {
          const i = () => index
          const o = {
            get value() {
              return opt().value
            },
            get label() {
              return opt().label
            },
            get description() {
              return opt().description
            },
            get image() {
              return opt().image
            },
          }
          const selected = () => picked().includes(o.value)
          const off = () => !enabled(i())
          const description = () => (o.description ? String(o.description) : ``)
          return (
            <button
              ref={(el) => (buttons[i()] = el)}
              type="button"
              role={multiple() ? `checkbox` : `radio`}
              aria-checked={selected()}
              aria-label={o.label}
              disabled={off()}
              tabindex={i() === tabStop() ? 0 : -1}
              data-state={selected() ? `checked` : `unchecked`}
              data-value={o.value}
              onClick={() => pick(i())}
              onKeyDown={(e) => onKeyDown(i(), e)}
              {...part.at(`tile`, i(), ...states(selected(), off()))}
            >
              <Show when={pictures()}>
                <span aria-hidden="true" {...part.at(`picture`, i(), ...states(selected(), off()))}>
                  <TilePicture src={o.image ? String(o.image) : null} />
                </span>
              </Show>
              <span {...part.at(`label`, i(), ...states(selected(), off()))}>{o.label}</span>
              <Show when={description()}>
                <span {...part.at(`description`, i(), ...states(selected(), off()))}>{description()}</span>
              </Show>
            </button>
          )
        }}
      </Index>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

export const NATIVES_CHOICE_TILES: Record<string, Component<NativeProps>> = { ChoiceTiles: ChoiceTilesNative }
