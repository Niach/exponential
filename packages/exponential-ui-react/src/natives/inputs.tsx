// VAPP-87 + round 1: the form controls. Text inputs are HOST-OWNED
// (`useHostOwnedValue`: local state, debounced `change` with a revision,
// `commit` on blur/Enter, echoes applied only when idle and acknowledged);
// the rest are controlled from the resolved prop with a local mirror
// (`useBoundState`: a bound prop is written through to the data model and
// `change` fires). Every named control speaks the Form protocol
// (`useField`: all failing checks under the field, `<id>.error` linked by
// aria-describedby, the `invalid` recipe state, Enter submits the Form).

import { useEffect, useId, useMemo, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react"
import { Checkbox as CheckboxPrimitive, RadioGroup as RadioPrimitive, Slider as SliderPrimitive, Switch as SwitchPrimitive } from "radix-ui"
import { useSurfaceContext } from "../context"
import { boundPath, hostPath } from "../data"
import { FieldErrors, joinIds, useField } from "../form"
import { CHROME, IconGlyph } from "../icons"
import { useHostOwnedValue } from "../inputs"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"
import { useBoundState } from "./bound"
import { arr, bool, BuiltinIcon, num, str, useParts, TextPart } from "./shared"
import type { SurfaceInputEvent } from "../host"

export function useInputSender(node: NativeProps[`node`], scope: string, prop: string, domId: string = node.id) {
  const ctx = useSurfaceContext()
  const path = boundPath(node.props, prop, scope)
  const name = str(node.props.name, node.id)
  const send = useMemo(
    () => (value: unknown, revision: number, kind: `change` | `commit`) => {
      const e: SurfaceInputEvent = { surfaceId: ctx.surfaceId, componentId: domId, name, path: hostPath(path), value, revision, kind }
      return ctx.host.onInput?.(e)
    },
    [ctx.surfaceId, ctx.host, domId, name, path]
  )
  const writeLocal = useMemo(() => (path ? (value: unknown) => ctx.setData(path, value) : undefined), [path, ctx])
  return { send, writeLocal, path, name }
}

export function InputNative(p: NativeProps) {
  return <TextFieldNative {...p} multiline={false} />
}
export function TextareaNative(p: NativeProps) {
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

function TextFieldNative({ node, props, rootProps, emit, scope, domId, multiline }: NativeProps & { multiline: boolean }) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const { send, writeLocal } = useInputSender(node, scope, `value`, domId)
  const external = str(props.value)
  const field = useHostOwnedValue<string>({ external, send, writeLocal })
  const focusRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null)
  const f = useField({ node, domId, props, value: field.value, focusRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const label = str(props.label)
  const placeholder = str(props.placeholder)
  const description = str(props.description)
  const rows = Math.max(1, num(props.rows, 3))
  const maxRows = props.maxRows === undefined ? null : Math.max(rows, num(props.maxRows, rows))
  const auto = multiline && bool(props.autosize)
  useEffect(() => {
    if (auto && focusRef.current) autosize(focusRef.current as HTMLTextAreaElement, rows, maxRows)
  }, [auto, field.value, rows, maxRows])
  const shared = {
    id,
    name: str(props.name, node.id),
    value: field.value,
    placeholder: placeholder || undefined,
    disabled,
    "aria-invalid": f.invalid ? true : undefined,
    "aria-describedby": joinIds(description && `${id}-d`, f.describedBy),
    onFocus: () => {
      field.onFocus()
      void emit(`focus`)
    },
    onBlur: () => {
      field.onBlur()
      f.touch()
      void emit(`blur`)
    },
    onChange: (e: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      field.edit(e.target.value)
      f.change(e.target.value)
      void emit(`change`, { value: e.target.value, revision: field.revision + 1 })
    },
  }
  const fieldStates = [disabled && `disabled`, field.focused && `focus`, f.invalid && `invalid`] as const
  void ctx
  return (
    <div {...(rootProps as Record<string, unknown>)} data-pending={field.pending ? `true` : undefined} data-revision={field.revision}>
      <TextPart part={part(`label`)} text={label} as="label" htmlFor={id} />
      {multiline ? (
        <textarea ref={focusRef as React.Ref<HTMLTextAreaElement>} {...(part(`field`, ...fieldStates) as Record<string, string>)} {...shared} rows={rows} data-autosize={auto ? `true` : undefined} />
      ) : (
        <input
          ref={focusRef as React.Ref<HTMLInputElement>}
          {...(part(`field`, ...fieldStates) as Record<string, string>)}
          {...shared}
          type={str(props.type, `text`)}
          onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
            if (e.key === `Enter`) {
              e.preventDefault()
              field.commit()
              void emit(`submit`, { value: field.value })
              f.submit()
            }
          }}
        />
      )}
      <TextPart part={part(`description`)} text={description} id={`${id}-d`} />
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

const localeDigits = new Map<string, Map<string, string>>()

/** The locale's own digits (`٠`…`٩` for `ar-EG`) → ASCII `0`…`9`. */
function digitMap(locale: string): Map<string, string> {
  let hit = localeDigits.get(locale)
  if (!hit) {
    hit = new Map()
    const fmt = new Intl.NumberFormat(locale, { useGrouping: false })
    for (let d = 0; d <= 9; d++) {
      const glyph = fmt.format(d)
      if (glyph !== String(d)) hit.set(glyph, String(d))
    }
    localeDigits.set(locale, hit)
  }
  return hit
}

/** A typed number in the surface locale (its group and decimal separators
 *  AND its digits; ASCII digits are always accepted too) → the number, or
 *  null. */
export function parseLocaleNumber(text: string, locale: string): number | null {
  const parts = new Intl.NumberFormat(locale).formatToParts(12345.6)
  const group = parts.find((p) => p.type === `group`)?.value ?? `,`
  const decimal = parts.find((p) => p.type === `decimal`)?.value ?? `.`
  const digits = digitMap(locale)
  const ascii = digits.size ? Array.from(text, (ch) => digits.get(ch) ?? ch).join(``) : text
  const clean = ascii.trim().split(group).join(``).replace(/[\s\u200e\u200f\u061c]/g, ``).replace(decimal, `.`).replace(/[−–]/g, `-`)
  if (clean === `` || clean === `-`) return null
  const n = Number(clean)
  return Number.isFinite(n) ? n : null
}

export function NumberFieldNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const id = useId()
  const min = props.min === undefined ? null : num(props.min)
  const max = props.max === undefined ? null : num(props.max)
  const step = num(props.step, 1) || 1
  const precision = props.precision === undefined ? decimalsOf(step) : Math.max(0, num(props.precision))
  const external = props.value === undefined || props.value === null || props.value === `` ? null : num(props.value)
  const [value, setValue] = useBoundState<number | null>(node, scope, `value`, external)
  const [text, setText] = useState<string | null>(null)
  const focusRef = useRef<HTMLInputElement | null>(null)
  const f = useField({ node, domId, props, value, focusRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const format = useMemo(() => new Intl.NumberFormat(ctx.locale, { minimumFractionDigits: precision, maximumFractionDigits: precision }), [ctx.locale, precision])
  const clamp = (n: number) => {
    let v = n
    if (min !== null) v = Math.max(min, v)
    if (max !== null) v = Math.min(max, v)
    return Number(v.toFixed(precision))
  }
  const commit = (next: number | null) => {
    setValue(next)
    f.change(next)
    void emit(`change`, { value: next })
  }
  const stepBy = (delta: number) => {
    const base = value ?? (min !== null && min > 0 ? min : 0)
    commit(clamp(base + delta))
    setText(null)
  }
  const shown = text ?? (value === null ? `` : format.format(value))
  const unit = str(props.unit)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <div {...(part(`field`, disabled && `disabled`, f.invalid && `invalid`) as Record<string, string>)} data-xui-numberfield="">
        <button type="button" {...(part(`decrement`, disabled && `disabled`) as Record<string, string>)} aria-label={ctx.t(`decrement`)} tabIndex={-1} disabled={disabled || (min !== null && value !== null && value <= min)} onClick={() => stepBy(-step)}>
          <BuiltinIcon slot="NumberField.decrement" />
        </button>
        <input
          ref={focusRef}
          id={id}
          className="xui-NumberField-input"
          type="text"
          inputMode={precision > 0 ? `decimal` : `numeric`}
          role="spinbutton"
          aria-valuenow={value ?? undefined}
          aria-valuemin={min ?? undefined}
          aria-valuemax={max ?? undefined}
          aria-valuetext={value === null ? undefined : `${format.format(value)}${unit ? ` ${unit}` : ``}`}
          aria-invalid={f.invalid || undefined}
          aria-describedby={f.describedBy}
          name={str(props.name, node.id)}
          placeholder={str(props.placeholder) || undefined}
          disabled={disabled}
          value={shown}
          onFocus={() => {
            setText(value === null ? `` : String(value).replace(`.`, format.formatToParts(1.1).find((p) => p.type === `decimal`)?.value ?? `.`))
            void emit(`focus`)
          }}
          onChange={(e) => {
            setText(e.target.value)
            const n = parseLocaleNumber(e.target.value, ctx.locale)
            if (n !== null || e.target.value.trim() === ``) commit(n)
          }}
          onBlur={() => {
            if (value !== null && clamp(value) !== value) commit(clamp(value))
            setText(null)
            f.touch()
            void emit(`blur`)
          }}
          onKeyDown={(e) => {
            const big = e.shiftKey ? 10 : 1
            if (e.key === `ArrowUp`) {
              e.preventDefault()
              stepBy(step * big)
            } else if (e.key === `ArrowDown`) {
              e.preventDefault()
              stepBy(-step * big)
            } else if (e.key === `PageUp`) {
              e.preventDefault()
              stepBy(step * 10)
            } else if (e.key === `PageDown`) {
              e.preventDefault()
              stepBy(-step * 10)
            } else if (e.key === `Home` && min !== null) {
              e.preventDefault()
              commit(min)
              setText(null)
            } else if (e.key === `End` && max !== null) {
              e.preventDefault()
              commit(max)
              setText(null)
            } else if (e.key === `Enter`) {
              e.preventDefault()
              const v = value === null ? null : clamp(value)
              if (v !== value) commit(v)
              setText(null)
              void emit(`submit`, { value: v })
              f.submit()
            }
          }}
        />
        {unit ? <span {...(part(`unit`) as Record<string, string>)}>{unit}</span> : null}
        <button type="button" {...(part(`increment`, disabled && `disabled`) as Record<string, string>)} aria-label={ctx.t(`increment`)} tabIndex={-1} disabled={disabled || (max !== null && value !== null && value >= max)} onClick={() => stepBy(step)}>
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

export function CheckboxNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const id = useId()
  const [checked, setChecked] = useBoundState(node, scope, `checked`, bool(props.checked))
  const focusRef = useRef<HTMLButtonElement | null>(null)
  const f = useField({ node, domId, props, value: checked, focusRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <CheckboxPrimitive.Root
        ref={focusRef}
        id={id}
        {...(part(`box`, checked && `checked`, disabled && `disabled`, f.invalid && `invalid`) as Record<string, string>)}
        checked={checked}
        disabled={disabled}
        name={str(props.name, node.id)}
        aria-invalid={f.invalid || undefined}
        aria-describedby={f.describedBy}
        onCheckedChange={(next) => {
          const value = next === true
          setChecked(value)
          f.change(value)
          f.touch()
          void emit(`change`, { checked: value })
        }}
      >
        <CheckboxPrimitive.Indicator {...(part(`check`) as Record<string, string>)}>
          <BuiltinIcon slot="Checkbox.check" />
        </CheckboxPrimitive.Indicator>
      </CheckboxPrimitive.Root>
      <div className="xui-field-text">
        <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
        <TextPart part={part(`description`)} text={props.description} />
        <FieldErrors part={part} field={f} />
      </div>
    </div>
  )
}

export function SwitchNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const id = useId()
  const ctx = useSurfaceContext()
  const [checked, setChecked] = useBoundState(node, scope, `checked`, bool(props.checked))
  const focusRef = useRef<HTMLButtonElement | null>(null)
  const f = useField({ node, domId, props, value: checked, focusRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const travel = useMemo(() => {
    const trackWidth = num((ctx.theme.recipes.Switch?.track?.[0]?.style as { width?: number } | undefined)?.width, 32)
    const thumb = num((ctx.theme.recipes.Switch?.thumb?.[0]?.style as { width?: number } | undefined)?.width, 16)
    return Math.max(0, trackWidth - thumb - 4)
  }, [ctx.theme])
  const description = str(props.description)
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <SwitchPrimitive.Root
        ref={focusRef}
        id={id}
        {...(part(`track`, checked && `checked`, disabled && `disabled`) as Record<string, string>)}
        checked={checked}
        disabled={disabled}
        name={str(props.name, node.id)}
        aria-describedby={joinIds(description && `${id}-d`, f.describedBy)}
        style={{ "--xui-switch-travel": `${travel}px` } as React.CSSProperties}
        onCheckedChange={(next) => {
          setChecked(next)
          f.change(next)
          void emit(`change`, { checked: next })
        }}
      >
        <SwitchPrimitive.Thumb {...(part(`thumb`, checked && `checked`) as Record<string, string>)} />
      </SwitchPrimitive.Root>
      <div className="xui-field-text">
        <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
        <TextPart part={part(`description`)} text={description} id={`${id}-d`} />
        <FieldErrors part={part} field={f} />
      </div>
    </div>
  )
}

interface Option {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

export function RadioNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const options = arr<Option>(props.options)
  const [value, setValue] = useBoundState(node, scope, `value`, str(props.value))
  const focusRef = useRef<HTMLDivElement | null>(null)
  // Focus goes to the checked item (else the first), the group's tab stop.
  const itemRef = useMemo(
    () => ({
      get current(): HTMLElement | null {
        return (focusRef.current?.querySelector(`[data-state="checked"]`) ?? focusRef.current?.querySelector(`button`)) as HTMLElement | null
      },
    }),
    []
  )
  const f = useField({ node, domId, props, value, focusRef: itemRef })
  return (
    <div {...(rootProps as Record<string, unknown>)} role="group" aria-labelledby={props.label ? `${id}-l` : undefined} aria-describedby={f.describedBy}>
      <TextPart part={part(`label`)} text={props.label} id={`${id}-l`} />
      <RadioPrimitive.Root
        ref={focusRef}
        className="xui-radio-items"
        value={value}
        name={str(props.name, node.id)}
        orientation={str(props.orientation, `vertical`) as `vertical` | `horizontal`}
        dir={ctx.direction}
        loop
        aria-invalid={f.invalid || undefined}
        onValueChange={(next) => {
          setValue(next)
          f.change(next)
          void emit(`change`, { value: next })
        }}
      >
        {options.map((o) => {
          const checked = o.value === value
          const oid = `${id}-${o.value}`
          return (
            <label key={o.value} className="xui-radio-row" htmlFor={oid}>
              <RadioPrimitive.Item id={oid} value={o.value} disabled={bool(o.disabled)} {...(part(`item`, checked && `checked`, bool(o.disabled) && `disabled`, f.invalid && `invalid`) as Record<string, string>)}>
                <RadioPrimitive.Indicator {...(part(`dot`, checked && `checked`) as Record<string, string>)} />
              </RadioPrimitive.Item>
              {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} className="xui-icon" width={16} height={16} /> : null}
              <span {...(part(`label`) as Record<string, string>)}>{str(o.label)}</span>
            </label>
          )
        })}
      </RadioPrimitive.Root>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

export function SliderNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const min = num(props.min, 0)
  const max = num(props.max, 100)
  const step = num(props.step, 1)
  const [value, setValue] = useBoundState(node, scope, `value`, num(props.value, min))
  const format = useMemo(() => new Intl.NumberFormat(ctx.locale), [ctx.locale])
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <div className="xui-slider-head">
        <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
        <span {...(part(`value`) as Record<string, string>)}>{format.format(value)}</span>
      </div>
      <SliderPrimitive.Root
        className="xui-slider-root"
        value={[value]}
        min={min}
        max={max}
        step={step}
        dir={ctx.direction}
        name={str(props.name) || undefined}
        onValueChange={([next]) => setValue(next)}
        onValueCommit={([next]) => void emit(`change`, { value: next })}
      >
        <SliderPrimitive.Track {...(part(`track`) as Record<string, string>)}>
          <SliderPrimitive.Range {...(part(`range`) as Record<string, string>)} />
        </SliderPrimitive.Track>
        <SliderPrimitive.Thumb id={id} {...(part(`thumb`) as Record<string, string>)} aria-label={str(props.label) || undefined} />
      </SliderPrimitive.Root>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Composer
// ---------------------------------------------------------------------------

export function ComposerNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const { send, writeLocal } = useInputSender(node, scope, `value`, domId)
  const external = str(props.value)
  const field = useHostOwnedValue<string>({ external, send, writeLocal })
  const busy = bool(props.busy)
  const ref = useRef<HTMLTextAreaElement>(null)
  const grow = () => {
    const el = ref.current
    if (!el) return
    el.style.height = `auto`
    el.style.height = `${Math.min(200, el.scrollHeight)}px`
  }
  useEffect(grow, [field.value])
  const submit = () => {
    if (busy) {
      void emit(`stop`)
      return
    }
    const text = field.value.trim()
    if (!text) return
    field.commit()
    void emit(`submit`, { value: text })
    field.edit(``)
  }
  const placeholder = str(props.placeholder)
  const submitLabel = str(props.submitLabel) || ctx.t(`send`)
  return (
    <div {...(rootProps as Record<string, unknown>)} data-busy={busy ? `true` : undefined}>
      <textarea
        ref={ref}
        {...(part(`field`, field.focused && `focus`) as Record<string, string>)}
        rows={1}
        placeholder={placeholder || undefined}
        value={field.value}
        aria-label={placeholder || submitLabel}
        onFocus={field.onFocus}
        onBlur={field.onBlur}
        onChange={(e) => {
          field.edit(e.target.value)
          void emit(`change`, { value: e.target.value })
        }}
        onKeyDown={(e) => {
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            submit()
          }
        }}
      />
      <div className="xui-composer-bar">
        {bool(props.attachments) ? (
          <button type="button" {...(part(`attachment`) as Record<string, string>)} aria-label={ctx.t(`browse`)} onClick={() => void emit(`attach`)}>
            <BuiltinIcon slot="Composer.attachment" />
          </button>
        ) : null}
        <button type="button" {...(part(`send`, busy && `pressed`) as Record<string, string>)} aria-label={busy ? ctx.t(`stop`) : submitLabel} disabled={!busy && field.value.trim() === ``} onClick={submit}>
          {busy ? <BuiltinIcon slot="Composer.stop" /> : <BuiltinIcon slot="Composer.send" />}
        </button>
      </div>
    </div>
  )
}

export { CHROME, mergeStyle }
