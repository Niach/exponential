// FEED-128 (React's natives/inputs.tsx): the form controls. Text inputs are
// HOST-OWNED (`createHostOwnedValue`: local state, debounced `change` with a
// revision, `commit` on blur/Enter, echoes applied only when idle and
// acknowledged); the rest are controlled from the resolved prop with a
// local mirror (`createBoundState`: a bound prop is written through to the
// data model and `change` fires). Every named control speaks the Form
// protocol (`createField`: all failing checks under the field, `<id>.error`
// linked by aria-describedby, the `invalid` recipe state, Enter submits the
// Form).
//
// Radix paints React's Checkbox, Switch, RadioGroup and Slider; these are
// the same DOM hand-rolled on plain elements (Kobalte's parts write their
// own attribute set): the Radix attributes (`data-state`, `data-disabled`,
// `data-orientation`, `data-radix-collection-item`, `value="on"`, the
// roving tab stop, the slider's thumb wrapper and its in-bounds offset)
// and, inside a <form>, Radix's hidden BUBBLE inputs after the control.

import { For, Show, createEffect, createMemo, createSignal, createUniqueId, onCleanup, onMount, untrack, type JSX } from "solid-js"
import { boundPath, hostPath, parseLocaleNumber } from "@exponential_app/ui/web"
import { useSurfaceContext } from "../context"
import { FieldErrors, createField, joinIds } from "../form"
import { IconGlyph } from "../icons"
import { createHostOwnedValue } from "../inputs"
import type { NativeProps } from "../node-view"
import type { SurfaceInputEvent } from "../host"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, objects, parts, str, TextPart } from "./shared"
import { BACK_KEYS, BubbleCheck, clampTo, createElementSize, createInForm, decimalCount, inBoundsOffset, KEY_INTENT, linearScale, PAGE_KEYS, percentOf, roundValue, rovingFocusFirst, SliderBubble } from "../behaviors"

/** React's `useInputSender`: the host `onInput` sender of a host-owned
 *  field and its local write-through (the bound data path). */
export function createInputSender(props: NativeProps, prop: string) {
  const ctx = useSurfaceContext()
  const path = createMemo(() => boundPath(props.node.props, prop, props.scope))
  const name = () => str(props.node.props.name, props.node.id)
  const send = (value: unknown, revision: number, kind: `change` | `commit`) =>
    untrack(() => {
      const e: SurfaceInputEvent = { surfaceId: ctx.surfaceId, componentId: props.domId, name: name(), path: hostPath(path()), value, revision, kind }
      return ctx.host.onInput?.(e)
    })
  const writeLocal = (value: unknown) => {
    const p = untrack(path)
    if (p) ctx.setData(p, value)
  }
  return { send, writeLocal, path, name }
}

export function InputNative(p: NativeProps): JSX.Element {
  return <TextFieldNative {...p} multiline={false} />
}
export function TextareaNative(p: NativeProps): JSX.Element {
  return <TextFieldNative {...p} multiline />
}

/** Textarea autosize (contract §3): from `rows` lines up to `maxRows`, then
 *  it scrolls. */
function autosize(el: HTMLTextAreaElement, rows: number, maxRows: number | null) {
  const cs = getComputedStyle(el)
  const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.4 || 20
  const pad = (parseFloat(cs.paddingTop) || 0) + (parseFloat(cs.paddingBottom) || 0) + (parseFloat(cs.borderTopWidth) || 0) + (parseFloat(cs.borderBottomWidth) || 0)
  const min = rows * lh + pad
  const max = maxRows ? maxRows * lh + pad : Infinity
  el.style.height = `auto`
  const h = Math.min(max, Math.max(min, el.scrollHeight + (parseFloat(cs.borderTopWidth) || 0) + (parseFloat(cs.borderBottomWidth) || 0)))
  el.style.height = `${h}px`
  el.style.overflowY = el.scrollHeight > h ? `auto` : `hidden`
}

function TextFieldNative(props: NativeProps & { multiline: boolean }): JSX.Element {
  const part = parts(props)
  const id = createUniqueId()
  const { send, writeLocal } = createInputSender(props, `value`)
  const field = createHostOwnedValue<string>({ external: () => str(props.props.value), send, writeLocal })
  let el: HTMLInputElement | HTMLTextAreaElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value: () => field.value, focusEl: () => el })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const label = () => str(props.props.label)
  const placeholder = () => str(props.props.placeholder)
  const description = () => str(props.props.description)
  const rows = () => Math.max(1, num(props.props.rows, 3))
  const maxRows = () => (props.props.maxRows === undefined ? null : Math.max(rows(), num(props.props.maxRows, rows())))
  const auto = () => props.multiline && bool(props.props.autosize)
  createEffect(() => {
    const a = auto()
    void field.value
    const r = rows()
    const m = maxRows()
    if (a && el) autosize(el as HTMLTextAreaElement, r, m)
  })
  const fieldStates = () => [disabled() && `disabled`, field.focused && `focus`, f.invalid && `invalid`] as const
  const onFocus = () => {
    field.onFocus()
    void props.emit(`focus`)
  }
  const onBlur = () => {
    field.onBlur()
    f.touch()
    void props.emit(`blur`)
  }
  const onInput = (e: InputEvent & { currentTarget: HTMLInputElement | HTMLTextAreaElement }) => {
    const v = e.currentTarget.value
    // React's closure reads the revision of the render before the edit.
    const revision = field.revision + 1
    field.edit(v)
    f.change(v)
    void props.emit(`change`, { value: v, revision }, { value: v })
  }
  return (
    <div {...props.rootProps} data-pending={field.pending ? `true` : undefined} data-revision={field.revision}>
      <TextPart part={part(`label`)} text={label()} as="label" htmlFor={id} />
      <Show
        when={props.multiline}
        fallback={
          <input
            ref={(e) => (el = e)}
            {...part(`field`, ...fieldStates())}
            id={id}
            name={str(props.props.name, props.node.id)}
            value={field.value}
            prop:defaultValue={field.value}
            placeholder={placeholder() || undefined}
            disabled={disabled()}
            aria-invalid={f.invalid ? true : undefined}
            aria-describedby={joinIds(description() && `${id}-d`, f.describedBy)}
            onFocus={onFocus}
            onBlur={onBlur}
            onInput={onInput}
            type={str(props.props.type, `text`)}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                field.commit()
                void props.emit(`submit`, { value: field.value })
                f.submit()
              }
            }}
          />
        }
      >
        <textarea
          ref={(e) => (el = e)}
          {...part(`field`, ...fieldStates())}
          id={id}
          name={str(props.props.name, props.node.id)}
          value={field.value}
          prop:defaultValue={field.value}
          placeholder={placeholder() || undefined}
          disabled={disabled()}
          aria-invalid={f.invalid ? true : undefined}
          aria-describedby={joinIds(description() && `${id}-d`, f.describedBy)}
          onFocus={onFocus}
          onBlur={onBlur}
          onInput={onInput}
          rows={rows()}
          data-autosize={auto() ? `true` : undefined}
        />
      </Show>
      <TextPart part={part(`description`)} text={description()} id={`${id}-d`} />
      <FieldErrors part={part} field={f} />
    </div>
  )
}

// ---------------------------------------------------------------------------
// NumberField
// ---------------------------------------------------------------------------

const decimalsOf = (n: number) => {
  const s = String(n)
  const i = s.indexOf(`.`)
  return i < 0 ? 0 : s.length - i - 1
}

export function NumberFieldNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const id = createUniqueId()
  const min = () => (props.props.min === undefined ? null : num(props.props.min))
  const max = () => (props.props.max === undefined ? null : num(props.props.max))
  const step = () => num(props.props.step, 1) || 1
  const precision = () => (props.props.precision === undefined ? decimalsOf(step()) : Math.max(0, num(props.props.precision)))
  const external = () => (props.props.value === undefined || props.props.value === null || props.props.value === `` ? null : num(props.props.value))
  const [value, setValue] = createBoundState<number | null>(() => props.node, () => props.scope, `value`, external)
  const [text, setText] = createSignal<string | null>(null)
  let el: HTMLInputElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value, focusEl: () => el })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  // Display through the surface Formatter (round 2 §3); the typed text is
  // parsed in the locale's separators.
  const decimalSep = createMemo(() => new Intl.NumberFormat(ctx.locale).formatToParts(1.1).find((p) => p.type === `decimal`)?.value ?? `.`)
  const format = (n: number) => ctx.formatter.number(n, { decimals: precision() })
  const clamp = (n: number) => {
    let v = n
    const lo = min()
    const hi = max()
    if (lo !== null) v = Math.max(lo, v)
    if (hi !== null) v = Math.min(hi, v)
    return Number(v.toFixed(precision()))
  }
  const commit = (next: number | null) => {
    setValue(next)
    f.change(next)
    void props.emit(`change`, { value: next }, { value: next })
  }
  const stepBy = (delta: number) => {
    const lo = min()
    const base = value() ?? (lo !== null && lo > 0 ? lo : 0)
    commit(clamp(base + delta))
    setText(null)
  }
  const shown = () => text() ?? (value() === null ? `` : format(value()!))
  const unit = () => str(props.props.unit)
  return (
    <div {...props.rootProps}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <div {...part(`field`, disabled() && `disabled`, f.invalid && `invalid`)} data-xui-numberfield="">
        <button
          type="button"
          {...part(`decrement`, disabled() && `disabled`)}
          aria-label={ctx.t(`decrement`)}
          tabindex={-1}
          disabled={disabled() || (min() !== null && value() !== null && value()! <= min()!)}
          onClick={() => stepBy(-step())}
        >
          <BuiltinIcon slot="NumberField.decrement" />
        </button>
        <input
          ref={(e) => (el = e)}
          id={id}
          {...part(`input`)}
          type="text"
          inputmode={precision() > 0 ? `decimal` : `numeric`}
          role="spinbutton"
          aria-valuenow={value() ?? undefined}
          aria-valuemin={min() ?? undefined}
          aria-valuemax={max() ?? undefined}
          aria-valuetext={value() === null ? undefined : `${format(value()!)}${unit() ? ` ${unit()}` : ``}`}
          aria-invalid={f.invalid || undefined}
          aria-describedby={f.describedBy}
          name={str(props.props.name, props.node.id)}
          placeholder={str(props.props.placeholder) || undefined}
          disabled={disabled()}
          value={shown()}
          prop:defaultValue={shown()}
          onFocus={() => {
            const v = value()
            setText(v === null ? `` : String(v).replace(`.`, decimalSep()))
            void props.emit(`focus`)
          }}
          onInput={(e) => {
            const raw = e.currentTarget.value
            setText(raw)
            const n = parseLocaleNumber(raw, ctx.locale)
            if (n !== null || raw.trim() === ``) commit(n)
          }}
          onBlur={() => {
            const v = value()
            if (v !== null && clamp(v) !== v) commit(clamp(v))
            setText(null)
            f.touch()
            void props.emit(`blur`)
          }}
          onKeyDown={(e) => {
            const big = e.shiftKey ? 10 : 1
            const lo = min()
            const hi = max()
            if (e.key === `ArrowUp`) {
              e.preventDefault()
              stepBy(step() * big)
            } else if (e.key === `ArrowDown`) {
              e.preventDefault()
              stepBy(-step() * big)
            } else if (e.key === `PageUp`) {
              e.preventDefault()
              stepBy(step() * 10)
            } else if (e.key === `PageDown`) {
              e.preventDefault()
              stepBy(-step() * 10)
            } else if (e.key === `Home` && lo !== null) {
              e.preventDefault()
              commit(lo)
              setText(null)
            } else if (e.key === `End` && hi !== null) {
              e.preventDefault()
              commit(hi)
              setText(null)
            } else if (e.key === `Enter`) {
              e.preventDefault()
              const cur = value()
              const v = cur === null ? null : clamp(cur)
              if (v !== cur) commit(v)
              setText(null)
              void props.emit(`submit`, { value: v }, v !== cur ? { value: v } : undefined)
              f.submit()
            }
          }}
        />
        {unit() ? <span {...part(`unit`)}>{unit()}</span> : null}
        <button
          type="button"
          {...part(`increment`, disabled() && `disabled`)}
          aria-label={ctx.t(`increment`)}
          tabindex={-1}
          disabled={disabled() || (max() !== null && value() !== null && value()! >= max()!)}
          onClick={() => stepBy(step())}
        >
          <BuiltinIcon slot="NumberField.increment" />
        </button>
      </div>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

// ---------------------------------------------------------------------------
// Checkbox, Switch, Radio, Slider
// ---------------------------------------------------------------------------

export function CheckboxNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const id = createUniqueId()
  const [checked, setCheckedState] = createBoundState(() => props.node, () => props.scope, `checked`, () => bool(props.props.checked))
  const [box, setBox] = createSignal<HTMLButtonElement>()
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value: checked, focusEl: box })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const inForm = createInForm(box)
  let stopped = false
  return (
    <div {...props.rootProps}>
      <button
        type="button"
        role="checkbox"
        aria-checked={checked()}
        data-state={checked() ? `checked` : `unchecked`}
        data-disabled={disabled() ? `` : undefined}
        disabled={disabled()}
        value="on"
        ref={setBox}
        id={id}
        {...part(`box`, checked() && `checked`, disabled() && `disabled`, f.invalid && `invalid`)}
        aria-invalid={f.invalid || undefined}
        aria-describedby={f.describedBy}
        onKeyDown={(e) => {
          if (e.key === `Enter`) e.preventDefault()
        }}
        onClick={(e) => {
          const value = !checked()
          setCheckedState(value)
          f.change(value)
          f.touch()
          void props.emit(`change`, { checked: value }, { checked: value })
          if (inForm()) {
            stopped = e.cancelBubble
            if (!stopped) e.stopPropagation()
          }
        }}
      >
        <Show when={checked()}>
          <span data-state="checked" data-disabled={disabled() ? `` : undefined} {...part(`check`)} style={css({ pointerEvents: `none` })}>
            <BuiltinIcon slot="Checkbox.check" />
          </span>
        </Show>
      </button>
      <Show when={inForm()}>
        <BubbleCheck type="checkbox" control={box()} checked={checked()} name={str(props.props.name, props.node.id)} value="on" disabled={disabled()} transformFirst={false} bubbles={() => !stopped} />
      </Show>
      <div class="xui-field-text">
        <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
        <TextPart part={part(`description`)} text={props.props.description} />
        <FieldErrors part={part} field={f} />
      </div>
    </div>
  )
}

export function SwitchNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const id = createUniqueId()
  const ctx = useSurfaceContext()
  const [checked, setCheckedState] = createBoundState(() => props.node, () => props.scope, `checked`, () => bool(props.props.checked))
  const [track, setTrack] = createSignal<HTMLButtonElement>()
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value: checked, focusEl: track })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const inForm = createInForm(track)
  let stopped = false
  const travel = createMemo(() => {
    const trackWidth = num((ctx.theme.recipes.Switch?.track?.[0]?.style as { width?: number } | undefined)?.width, 32)
    const thumb = num((ctx.theme.recipes.Switch?.thumb?.[0]?.style as { width?: number } | undefined)?.width, 16)
    return Math.max(0, trackWidth - thumb - 4)
  })
  const description = () => str(props.props.description)
  // Round 2 §7: [label + description][switch] — label first, the track at
  // the END; the whole row toggles (the label already clicks the control).
  const onRow = (e: MouseEvent) => {
    const target = e.target as Element
    if (disabled() || target.closest(`button,label,a,input`)) return
    track()?.click()
  }
  return (
    <div {...props.rootProps} onClick={onRow}>
      <div {...part(`body`)}>
        <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
        <TextPart part={part(`description`)} text={description()} id={`${id}-d`} />
        <FieldErrors part={part} field={f} />
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked()}
        data-state={checked() ? `checked` : `unchecked`}
        data-disabled={disabled() ? `` : undefined}
        disabled={disabled()}
        value="on"
        ref={setTrack}
        id={id}
        {...part(`track`, checked() && `checked`, disabled() && `disabled`)}
        aria-describedby={joinIds(description() && `${id}-d`, f.describedBy)}
        style={css({ "--xui-switch-travel": `${travel()}px` })}
        onClick={(e) => {
          const next = !checked()
          setCheckedState(next)
          f.change(next)
          void props.emit(`change`, { checked: next }, { checked: next })
          if (inForm()) {
            stopped = e.cancelBubble
            if (!stopped) e.stopPropagation()
          }
        }}
      >
        <span data-state={checked() ? `checked` : `unchecked`} data-disabled={disabled() ? `` : undefined} {...part(`thumb`, checked() && `checked`)} />
      </button>
      <Show when={inForm()}>
        <BubbleCheck type="checkbox" control={track()} checked={checked()} name={str(props.props.name, props.node.id)} value="on" disabled={disabled()} transformFirst bubbles={() => !stopped} />
      </Show>
    </div>
  )
}

interface Option {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

const ARROWS = [`ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`]

export function RadioNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const options = createMemo(() => objects<Option>(props.props.options))
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, () => str(props.props.value))
  let group: HTMLDivElement | undefined
  // Focus goes to the checked item (else the first), the group's tab stop.
  const itemEl = () => (group?.querySelector(`[data-state="checked"]`) ?? group?.querySelector(`button`)) as HTMLElement | null
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value, focusEl: itemEl })
  const orientation = () => str(props.props.orientation, `vertical`) as `vertical` | `horizontal`
  const name = () => str(props.props.name, props.node.id)
  const inForm = createInForm(() => group)
  // Radix's RovingFocusGroup: the current tab stop, the group's own tab
  // stop while it has focusable items, entry focus to the checked item.
  const [tabStop, setTabStop] = createSignal<string | null>(null)
  const [tabbingBack, setTabbingBack] = createSignal(false)
  const focusable = () => options().filter((o) => !bool(o.disabled)).length
  let clickFocus = false
  let arrowDown = false
  const onDocKeyDown = (e: KeyboardEvent) => {
    if (ARROWS.includes(e.key)) arrowDown = true
  }
  const onDocKeyUp = () => (arrowDown = false)
  onMount(() => {
    document.addEventListener(`keydown`, onDocKeyDown)
    document.addEventListener(`keyup`, onDocKeyUp)
  })
  onCleanup(() => {
    document.removeEventListener(`keydown`, onDocKeyDown)
    document.removeEventListener(`keyup`, onDocKeyUp)
  })
  const itemNodes = () => Array.from(group?.querySelectorAll<HTMLButtonElement>(`[role="radio"]`) ?? []).filter((b) => !b.disabled)
  const choose = (next: string) => {
    setValue(next)
    f.change(next)
    void props.emit(`change`, { value: next }, { value: next })
  }
  let stopped = false
  return (
    <div {...props.rootProps} role="group" aria-labelledby={props.props.label ? `${id}-l` : undefined} aria-describedby={f.describedBy}>
      <TextPart part={part(`label`)} text={props.props.label} id={`${id}-l`} />
      <div
        role="radiogroup"
        aria-required={false}
        aria-orientation={orientation()}
        dir={ctx.direction}
        tabindex={tabbingBack() || focusable() === 0 ? -1 : 0}
        data-orientation={orientation()}
        ref={group}
        {...part(`items`)}
        class={`${part(`items`).class as string} xui-radio-items`}
        aria-invalid={f.invalid || undefined}
        style={css({ outline: `none` })}
        onMouseDown={() => (clickFocus = true)}
        onFocus={(e) => {
          const keyboard = !clickFocus
          if (e.target === e.currentTarget && keyboard && !tabbingBack()) {
            const nodes = itemNodes()
            const active = nodes.find((n) => n.value === untrack(value))
            const current = nodes.find((n) => n.value === untrack(tabStop))
            rovingFocusFirst([active, current, ...nodes])
          }
          clickFocus = false
        }}
        onBlur={() => setTabbingBack(false)}
      >
        <For each={options()}>
          {(o, i) => {
            const checked = () => o.value === value()
            const isDisabled = () => bool(o.disabled)
            const oid = `${id}-${o.value}`
            let btn: HTMLButtonElement | undefined
            const [btnEl, setBtnEl] = createSignal<HTMLButtonElement>()
            // Placed parts (round 2 §6): the option ROW = `item.<i>`, its
            // circle = `dot.<i>` (the recipe's item), its text = `label.<i>`.
            return (
              <label class="xui-radio-row" for={oid} data-xui-id={`${part.domId}.item.${i()}`}>
                <button
                  tabindex={tabStop() === o.value ? 0 : -1}
                  data-orientation={orientation()}
                  type="button"
                  role="radio"
                  aria-checked={checked()}
                  data-state={checked() ? `checked` : `unchecked`}
                  data-disabled={isDisabled() ? `` : undefined}
                  disabled={isDisabled()}
                  value={o.value}
                  id={oid}
                  data-radix-collection-item=""
                  ref={(e) => {
                    btn = e
                    setBtnEl(e)
                  }}
                  {...part(`item`, checked() && `checked`, isDisabled() && `disabled`, f.invalid && `invalid`)}
                  data-xui-id={`${part.domId}.dot.${i()}`}
                  onMouseDown={(e) => {
                    if (isDisabled()) e.preventDefault()
                    else setTabStop(o.value)
                  }}
                  onFocus={() => {
                    setTabStop(o.value)
                    if (arrowDown) btn?.click()
                  }}
                  onKeyDown={(e) => {
                    if (e.key === `Tab` && e.shiftKey) {
                      setTabbingBack(true)
                      return
                    }
                    if (e.target !== e.currentTarget) return
                    const key = ctx.direction !== `rtl` ? e.key : e.key === `ArrowLeft` ? `ArrowRight` : e.key === `ArrowRight` ? `ArrowLeft` : e.key
                    const vertical = orientation() === `vertical`
                    if (vertical && (key === `ArrowLeft` || key === `ArrowRight`)) return
                    if (!vertical && (key === `ArrowUp` || key === `ArrowDown`)) return
                    const intent = KEY_INTENT[key]
                    if (intent !== undefined) {
                      if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
                      e.preventDefault()
                      let candidates = itemNodes() as HTMLElement[]
                      if (intent === `last`) candidates.reverse()
                      else if (intent === `prev` || intent === `next`) {
                        if (intent === `prev`) candidates.reverse()
                        const at = candidates.indexOf(e.currentTarget)
                        candidates = candidates.map((_, k) => candidates[(at + 1 + k) % candidates.length])
                      }
                      setTimeout(() => rovingFocusFirst(candidates))
                    } else if (e.key === `Enter`) e.preventDefault()
                  }}
                  onClick={(e) => {
                    if (!checked()) choose(o.value)
                    if (inForm()) {
                      stopped = e.cancelBubble
                      if (!stopped) e.stopPropagation()
                    }
                  }}
                >
                  <Show when={checked()}>
                    <span data-state="checked" data-disabled={isDisabled() ? `` : undefined} {...part(`dot`, checked() && `checked`)} />
                  </Show>
                </button>
                <Show when={inForm()}>
                  <BubbleCheck type="radio" control={btnEl()} checked={checked()} name={name()} value={o.value} disabled={isDisabled()} transformFirst bubbles={() => !stopped} />
                </Show>
                {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} class="xui-icon" width={16} height={16} /> : null}
                <span {...part.at(`label`, i())}>{str(o.label)}</span>
              </label>
            )
          }}
        </For>
      </div>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

/** Radix's single-thumb horizontal Slider. */
export function SliderNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const min = () => num(props.props.min, 0)
  const max = () => num(props.props.max, 100)
  const step = () => num(props.props.step, 1)
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, () => num(props.props.value, min()))
  let root: HTMLSpanElement | undefined
  const [thumb, setThumb] = createSignal<HTMLSpanElement>()
  const thumbSize = createElementSize(thumb)
  const inForm = createInForm(thumb)
  const fromLeft = () => ctx.direction !== `rtl`
  const startEdge = () => (fromLeft() ? `left` : `right`)
  const endEdge = () => (fromLeft() ? `right` : `left`)
  const percent = () => percentOf(value(), min(), max())
  const offset = () => {
    const w = thumbSize()?.width
    return w ? inBoundsOffset(w, percent(), fromLeft() ? 1 : -1) : 0
  }
  /** Radix's `updateValues`: snap to the step, clamp, write; a commit
   *  fires `change`. Every change focuses the thumb (Radix's onChange). */
  const update = (raw: number, commit: boolean) => {
    const snapped = roundValue(Math.round((raw - min()) / step()) * step() + min(), decimalCount(step()))
    const next = clampTo(snapped, [min(), max()])
    if (next === value()) return
    thumb()?.focus()
    setValue(next)
    if (commit) void props.emit(`change`, { value: next }, { value: next })
  }
  let rect: DOMRect | undefined
  let before = 0
  const fromPointer = (x: number) => {
    const r = rect ?? root!.getBoundingClientRect()
    rect = r
    return linearScale([0, r.width], fromLeft() ? [min(), max()] : [max(), min()])(x - r.left)
  }
  const rangeStyle = () => {
    const s: Record<string, string> = {}
    s[startEdge()] = `0%`
    s[endEdge()] = `${100 - percent()}%`
    return s
  }
  const wrapperStyle = () => {
    const s: Record<string, string> = { transform: `var(--radix-slider-thumb-transform)`, position: `absolute` }
    s[startEdge()] = `calc(${percent()}% + ${offset()}px)`
    return s
  }
  return (
    <div {...props.rootProps}>
      <div {...part(`header`)} class={`${part(`header`).class as string} xui-slider-head`}>
        <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
        <span {...part(`value`)}>{ctx.formatter.number(value())}</span>
      </div>
      <span
        ref={root}
        aria-disabled={false}
        dir={ctx.direction}
        data-orientation="horizontal"
        class="xui-slider-root"
        style={css({ "--radix-slider-thumb-transform": `translateX(-50%)` })}
        onKeyDown={(e) => {
          if (e.key === `Home`) {
            update(min(), true)
            e.preventDefault()
          } else if (e.key === `End`) {
            update(max(), true)
            e.preventDefault()
          } else if (PAGE_KEYS.includes(e.key) || ARROWS.includes(e.key)) {
            const back = BACK_KEYS[fromLeft() ? `from-left` : `from-right`].includes(e.key)
            const skip = PAGE_KEYS.includes(e.key) || (e.shiftKey && ARROWS.includes(e.key))
            update(value() + step() * (skip ? 10 : 1) * (back ? -1 : 1), true)
            e.preventDefault()
          }
        }}
        onPointerDown={(e) => {
          before = value()
          const target = e.target as HTMLElement
          target.setPointerCapture?.(e.pointerId)
          e.preventDefault()
          if (target === thumb()) target.focus()
          else update(fromPointer(e.clientX), false)
        }}
        onPointerMove={(e) => {
          const target = e.target as HTMLElement
          if (target.hasPointerCapture?.(e.pointerId)) update(fromPointer(e.clientX), false)
        }}
        onPointerUp={(e) => {
          const target = e.target as HTMLElement
          if (target.hasPointerCapture?.(e.pointerId)) {
            target.releasePointerCapture(e.pointerId)
            rect = undefined
            const next = value()
            if (next !== before) void props.emit(`change`, { value: next }, { value: next })
          }
        }}
      >
        <span data-orientation="horizontal" {...part(`track`)}>
          <span data-orientation="horizontal" {...part(`range`)} style={rangeStyle()} />
        </span>
        <span style={wrapperStyle()}>
          <span
            role="slider"
            aria-label={str(props.props.label) || undefined}
            aria-valuemin={min()}
            aria-valuenow={value()}
            aria-valuemax={max()}
            aria-orientation="horizontal"
            data-orientation="horizontal"
            tabindex={0}
            id={id}
            data-radix-collection-item=""
            ref={setThumb}
            {...part(`thumb`)}
            attr:style=""
          />
          <Show when={inForm()}>
            <SliderBubble name={str(props.props.name) || undefined} value={value()} />
          </Show>
        </span>
      </span>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Composer
// ---------------------------------------------------------------------------

export function ComposerNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const { send, writeLocal } = createInputSender(props, `value`)
  const field = createHostOwnedValue<string>({ external: () => str(props.props.value), send, writeLocal })
  const busy = () => bool(props.props.busy)
  let ref: HTMLTextAreaElement | undefined
  const grow = () => {
    const el = ref
    if (!el) return
    el.style.height = `auto`
    el.style.height = `${Math.min(200, el.scrollHeight)}px`
  }
  createEffect(() => {
    void field.value
    grow()
  })
  const submit = () => {
    if (busy()) {
      void props.emit(`stop`)
      return
    }
    const text = field.value.trim()
    if (!text) return
    field.commit()
    void props.emit(`submit`, { value: text })
    field.edit(``)
  }
  const placeholder = () => str(props.props.placeholder)
  const submitLabel = () => str(props.props.submitLabel) || ctx.t(`send`)
  return (
    <div {...props.rootProps} data-busy={busy() ? `true` : undefined}>
      <textarea
        ref={ref}
        {...part(`field`, field.focused && `focus`)}
        rows={1}
        placeholder={placeholder() || undefined}
        value={field.value}
        prop:defaultValue={field.value}
        aria-label={ctx.t(`message`)}
        onFocus={() => field.onFocus()}
        onBlur={() => field.onBlur()}
        onInput={(e) => {
          field.edit(e.currentTarget.value)
          void props.emit(`change`, { value: e.currentTarget.value }, { value: e.currentTarget.value })
        }}
        onKeyDown={(e) => {
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            submit()
          }
        }}
      />
      <div class="xui-composer-bar">
        {bool(props.props.attachments) ? (
          <button type="button" {...part(`attachment`)} aria-label={ctx.t(`browse`)} onClick={() => void props.emit(`attach`)}>
            <BuiltinIcon slot="Composer.attachment" />
          </button>
        ) : null}
        <button type="button" {...part(`send`, busy() && `pressed`)} aria-label={busy() ? ctx.t(`stop`) : submitLabel()} disabled={!busy() && field.value.trim() === ``} onClick={submit}>
          {busy() ? <BuiltinIcon slot="Composer.stop" /> : <BuiltinIcon slot="Composer.send" />}
        </button>
      </div>
    </div>
  )
}

export const NATIVES_INPUTS: Record<string, (props: NativeProps) => JSX.Element> = {
  Input: InputNative,
  Textarea: TextareaNative,
  NumberField: NumberFieldNative,
  Checkbox: CheckboxNative,
  Radio: RadioNative,
  Switch: SwitchNative,
  Slider: SliderNative,
  Composer: ComposerNative,
}
