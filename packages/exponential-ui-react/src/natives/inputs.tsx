// VAPP-87: the form controls. Text inputs are HOST-OWNED (`useHostOwnedValue`:
// local state, debounced `change` with a revision, `commit` on blur/Enter,
// echoes applied only when idle and acknowledged); the rest are controlled
// from the resolved prop with a local mirror so a press shows at once and
// the host's value wins when it changes. A bound `value`/`checked`
// (`{path}`) is also written through to the surface's data model.

import { useEffect, useId, useMemo, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react"
import { Checkbox as CheckboxPrimitive, Popover as PopoverPrimitive, RadioGroup as RadioPrimitive, Select as SelectPrimitive, Slider as SliderPrimitive, Switch as SwitchPrimitive } from "radix-ui"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { boundPath } from "../data"
import { CHROME, IconGlyph } from "../icons"
import { useHostOwnedValue } from "../inputs"
import type { NativeProps } from "../node-view"
import { arr, bool, failingCheck, num, str, useParts, TextPart } from "./shared"
import type { SurfaceInputEvent } from "../host"

function useInputSender(node: NativeProps[`node`], scope: string, prop: string) {
  const ctx = useSurfaceContext()
  const path = boundPath(node.props, prop, scope)
  const name = str(node.props.name, node.id)
  const send = useMemo(
    () => (value: unknown, revision: number, kind: `change` | `commit`) => {
      const e: SurfaceInputEvent = { surfaceId: ctx.surfaceId, componentId: node.id, name, path, value, revision, kind }
      return ctx.host.onInput?.(e)
    },
    [ctx.surfaceId, ctx.host, node.id, name, path]
  )
  const writeLocal = useMemo(() => (path ? (value: unknown) => ctx.setData(path, value) : undefined), [path, ctx])
  return { send, writeLocal, path, name }
}

function useValidation(props: Record<string, unknown>, value: unknown) {
  // `checks` resolved against the data model already; a check that reads
  // the field's own (local, unsent) value is re-run on the local value when
  // the condition is a plain `required`.
  const error = failingCheck(props.checks)
  const [touched, setTouched] = useState(false)
  const [submitted, setSubmitted] = useState(false)
  const on = str(props.validateOn, `blur`)
  const show = on === `change` ? true : on === `blur` ? touched : submitted
  const localEmpty = value === undefined || value === null || value === ``
  const effective = error && !(localEmpty === false && /required/i.test(error) === false && false) ? error : null
  return { error: show ? effective : null, touch: () => setTouched(true), submit: () => setSubmitted(true) }
}

export function InputNative(p: NativeProps) {
  return <TextFieldNative {...p} multiline={false} />
}
export function TextareaNative(p: NativeProps) {
  return <TextFieldNative {...p} multiline />
}

function TextFieldNative({ node, props, rootProps, emit, scope, multiline }: NativeProps & { multiline: boolean }) {
  const part = useParts(node, props)
  const id = useId()
  const { send, writeLocal } = useInputSender(node, scope, `value`)
  const external = str(props.value)
  const field = useHostOwnedValue<string>({ external, send, writeLocal })
  const disabled = bool(props.disabled)
  const validation = useValidation(props, field.value)
  const label = str(props.label)
  const placeholder = str(props.placeholder)
  const description = str(props.description)
  const shared = {
    id,
    name: str(props.name, node.id),
    value: field.value,
    placeholder: placeholder || undefined,
    disabled,
    "aria-invalid": validation.error ? true : undefined,
    "aria-describedby": description ? `${id}-d` : undefined,
    onFocus: () => {
      field.onFocus()
      void emit(`focus`)
    },
    onBlur: () => {
      field.onBlur()
      validation.touch()
      void emit(`blur`)
    },
    onChange: (e: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      field.edit(e.target.value)
      void emit(`change`, { value: e.target.value, revision: field.revision + 1 })
    },
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} data-pending={field.pending ? `true` : undefined} data-revision={field.revision}>
      <TextPart part={part(`label`)} text={label} as="label" htmlFor={id} />
      {multiline ? (
        <textarea {...(part(`field`, disabled && `disabled`, field.focused && `focus`) as Record<string, string>)} {...shared} rows={num(props.rows, 3)} />
      ) : (
        <input
          {...(part(`field`, disabled && `disabled`, field.focused && `focus`) as Record<string, string>)}
          {...shared}
          type={str(props.type, `text`)}
          onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
            if (e.key === `Enter`) {
              field.commit()
              validation.submit()
              void emit(`submit`, { value: field.value })
            }
          }}
        />
      )}
      <TextPart part={part(`description`)} text={description} id={`${id}-d`} />
      {validation.error ? <TextPart part={part(`error`)} text={validation.error} /> : null}
    </div>
  )
}

export function CheckboxNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const id = useId()
  const ctx = useSurfaceContext()
  const path = boundPath(node.props, `checked`, scope)
  const external = bool(props.checked)
  const [checked, setChecked] = useState(external)
  useEffect(() => setChecked(external), [external])
  const disabled = bool(props.disabled)
  const Check = CHROME.check
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <CheckboxPrimitive.Root
        id={id}
        {...(part(`box`, checked && `checked`, disabled && `disabled`) as Record<string, string>)}
        checked={checked}
        disabled={disabled}
        name={str(props.name, node.id)}
        onCheckedChange={(next) => {
          const value = next === true
          setChecked(value)
          if (path) ctx.setData(path, value)
          void emit(`change`, { checked: value })
        }}
      >
        <CheckboxPrimitive.Indicator {...(part(`check`) as Record<string, string>)}>
          <Check aria-hidden="true" />
        </CheckboxPrimitive.Indicator>
      </CheckboxPrimitive.Root>
      <div className="xui-field-text">
        <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
        <TextPart part={part(`description`)} text={props.description} />
      </div>
    </div>
  )
}

export function SwitchNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const id = useId()
  const ctx = useSurfaceContext()
  const path = boundPath(node.props, `checked`, scope)
  const external = bool(props.checked)
  const [checked, setChecked] = useState(external)
  useEffect(() => setChecked(external), [external])
  const disabled = bool(props.disabled)
  const travel = useMemo(() => {
    const trackWidth = num((ctx.theme.recipes.Switch?.track?.[0]?.style as { width?: number } | undefined)?.width, 32)
    const thumb = num((ctx.theme.recipes.Switch?.thumb?.[0]?.style as { width?: number } | undefined)?.width, 16)
    return Math.max(0, trackWidth - thumb - 4)
  }, [ctx.theme])
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <SwitchPrimitive.Root
        id={id}
        {...(part(`track`, checked && `checked`, disabled && `disabled`) as Record<string, string>)}
        checked={checked}
        disabled={disabled}
        name={str(props.name, node.id)}
        style={{ "--xui-switch-travel": `${travel}px` } as React.CSSProperties}
        onCheckedChange={(next) => {
          setChecked(next)
          if (path) ctx.setData(path, next)
          void emit(`change`, { checked: next })
        }}
      >
        <SwitchPrimitive.Thumb {...(part(`thumb`, checked && `checked`) as Record<string, string>)} />
      </SwitchPrimitive.Root>
      <div className="xui-field-text">
        <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
        <TextPart part={part(`description`)} text={props.description} />
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

export function RadioNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const path = boundPath(node.props, `value`, scope)
  const options = arr<Option>(props.options)
  const external = str(props.value)
  const [value, setValue] = useState(external)
  useEffect(() => setValue(external), [external])
  return (
    <div {...(rootProps as Record<string, unknown>)} role="group" aria-labelledby={props.label ? `${id}-l` : undefined}>
      <TextPart part={part(`label`)} text={props.label} id={`${id}-l`} />
      <RadioPrimitive.Root
        className="xui-radio-items"
        value={value}
        name={str(props.name, node.id)}
        orientation={str(props.orientation, `vertical`) as `vertical` | `horizontal`}
        dir={ctx.direction}
        onValueChange={(next) => {
          setValue(next)
          if (path) ctx.setData(path, next)
          void emit(`change`, { value: next })
        }}
      >
        {options.map((o) => {
          const checked = o.value === value
          const oid = `${id}-${o.value}`
          return (
            <label key={o.value} className="xui-radio-row" htmlFor={oid}>
              <RadioPrimitive.Item id={oid} value={o.value} disabled={bool(o.disabled)} {...(part(`item`, checked && `checked`, bool(o.disabled) && `disabled`) as Record<string, string>)}>
                <RadioPrimitive.Indicator {...(part(`dot`, checked && `checked`) as Record<string, string>)} />
              </RadioPrimitive.Item>
              {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} className="xui-icon" width={16} height={16} /> : null}
              <span {...(part(`label`) as Record<string, string>)}>{str(o.label)}</span>
            </label>
          )
        })}
      </RadioPrimitive.Root>
    </div>
  )
}

export function SliderNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const path = boundPath(node.props, `value`, scope)
  const min = num(props.min, 0)
  const max = num(props.max, 100)
  const step = num(props.step, 1)
  const external = num(props.value, min)
  const [value, setValue] = useState(external)
  useEffect(() => setValue(external), [external])
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      {props.label !== undefined || true ? (
        <div className="xui-slider-head">
          <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
          <span {...(part(`value`) as Record<string, string>)}>{value}</span>
        </div>
      ) : null}
      <SliderPrimitive.Root
        className="xui-slider-root"
        value={[value]}
        min={min}
        max={max}
        step={step}
        dir={ctx.direction}
        onValueChange={([next]) => {
          setValue(next)
          if (path) ctx.setData(path, next)
        }}
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

export function SelectNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const path = boundPath(node.props, `value`, scope)
  const options = arr<Option>(props.options)
  const multiple = bool(props.multiple)
  const searchable = bool(props.searchable)
  const disabled = bool(props.disabled)
  const placeholder = str(props.placeholder, `Choose`)
  const raw = props.value
  const external = multiple ? (Array.isArray(raw) ? raw.map(String) : raw ? [String(raw)] : []) : str(raw)
  const [value, setValue] = useState<string | string[]>(external)
  useEffect(() => setValue(external), [JSON.stringify(external)])
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState(``)
  const Chevron = CHROME.chevronDown
  const Check = CHROME.check
  const commit = (next: string | string[]) => {
    setValue(next)
    if (path) ctx.setData(path, next)
    void emit(`change`, { value: next })
  }
  const label = <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
  const source = str(props.source)
  if (!multiple && !searchable) {
    const chosen = options.find((o) => o.value === value)
    return (
      <div {...(rootProps as Record<string, unknown>)} data-source={source || undefined}>
        {label}
        <SelectPrimitive.Root value={(value as string) || undefined} onValueChange={commit} disabled={disabled} open={open} onOpenChange={setOpen} name={str(props.name, node.id)} dir={ctx.direction}>
          <SelectPrimitive.Trigger id={id} {...(part(`trigger`, open && `open`, disabled && `disabled`) as Record<string, string>)}>
            <span>{chosen ? <SelectPrimitive.Value>{str(chosen.label)}</SelectPrimitive.Value> : <span {...(part(`placeholder`) as Record<string, string>)}>{placeholder}</span>}</span>
            <SelectPrimitive.Icon asChild>
              <Chevron aria-hidden="true" />
            </SelectPrimitive.Icon>
          </SelectPrimitive.Trigger>
          <SelectPrimitive.Portal container={ctx.portal ?? undefined}>
            <SelectPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} position="popper" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Select" style={{ minWidth: `var(--radix-select-trigger-width)` }}>
              <SelectPrimitive.Viewport>
                {options.map((o) => (
                  <SelectPrimitive.Item key={o.value} value={o.value} disabled={bool(o.disabled)} {...(part(`item`, o.value === value && `selected`, bool(o.disabled) && `disabled`) as Record<string, string>)}>
                    {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                    <SelectPrimitive.ItemText>{str(o.label)}</SelectPrimitive.ItemText>
                    <SelectPrimitive.ItemIndicator {...(part(`check`) as Record<string, string>)} className={`${part(`check`).className as string} xui-Select-check`}>
                      <Check aria-hidden="true" width={16} height={16} />
                    </SelectPrimitive.ItemIndicator>
                  </SelectPrimitive.Item>
                ))}
              </SelectPrimitive.Viewport>
            </SelectPrimitive.Content>
          </SelectPrimitive.Portal>
        </SelectPrimitive.Root>
      </div>
    )
  }
  // Searchable and/or multiple: a popover list with an optional filter.
  const selected = multiple ? (value as string[]) : value ? [value as string] : []
  const chosen = options.filter((o) => selected.includes(o.value))
  const shown = query ? options.filter((o) => str(o.label).toLowerCase().includes(query.toLowerCase())) : options
  const toggle = (o: Option) => {
    if (multiple) {
      const next = selected.includes(o.value) ? selected.filter((v) => v !== o.value) : [...selected, o.value]
      commit(next)
    } else {
      commit(o.value)
      setOpen(false)
    }
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} data-source={source || undefined}>
      {label}
      <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
        <PopoverPrimitive.Trigger id={id} {...(part(`trigger`, open && `open`, disabled && `disabled`) as Record<string, string>)} disabled={disabled} role="combobox" aria-expanded={open}>
          <span>{chosen.length ? chosen.map((o) => str(o.label)).join(`, `) : <span {...(part(`placeholder`) as Record<string, string>)}>{placeholder}</span>}</span>
          <Chevron aria-hidden="true" />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content {...(part(`content`, open && `open`) as Record<string, string>)} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="Select" style={{ minWidth: `var(--radix-popover-trigger-width)` }} onOpenAutoFocus={(e) => (searchable ? undefined : e.preventDefault())}>
            {searchable ? <input className="xui-Select-search" placeholder="Search…" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search options" /> : null}
            <div role="listbox" aria-multiselectable={multiple || undefined} style={{ display: `flex`, flexDirection: `column` }}>
              {shown.map((o) => {
                const isSel = selected.includes(o.value)
                return (
                  <button type="button" key={o.value} role="option" aria-selected={isSel} disabled={bool(o.disabled)} {...(part(`item`, isSel && `selected`, bool(o.disabled) && `disabled`) as Record<string, string>)} onClick={() => toggle(o)}>
                    {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                    <span>{str(o.label)}</span>
                    {isSel ? (
                      <span {...(part(`check`) as Record<string, string>)} className={`${part(`check`).className as string} xui-Select-check`}>
                        <Check aria-hidden="true" width={16} height={16} />
                      </span>
                    ) : null}
                  </button>
                )
              })}
            </div>
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
    </div>
  )
}

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, `0`)}-${String(d.getDate()).padStart(2, `0`)}`
const parseIso = (s: string): Date | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s)
  return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])) : null
}

export function DatePickerNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const path = boundPath(node.props, `value`, scope)
  const external = str(props.value)
  const [value, setValue] = useState(external)
  useEffect(() => setValue(external), [external])
  const [open, setOpen] = useState(false)
  const selected = value ? parseIso(value) : null
  const [view, setView] = useState(() => selected ?? new Date())
  const min = props.min ? parseIso(str(props.min)) : null
  const max = props.max ? parseIso(str(props.max)) : null
  const Calendar = CHROME.calendar
  const Prev = CHROME.chevronLeft
  const Next = CHROME.chevronRight
  const first = new Date(view.getFullYear(), view.getMonth(), 1)
  const start = (first.getDay() + 6) % 7
  const days: Date[] = []
  for (let i = 0; i < 42; i++) days.push(new Date(first.getFullYear(), first.getMonth(), 1 - start + i))
  const weekdays = useMemo(() => {
    const base = new Date(2024, 0, 1)
    return Array.from({ length: 7 }, (_, i) => new Date(base.getFullYear(), base.getMonth(), base.getDate() + i).toLocaleDateString(undefined, { weekday: `narrow` }))
  }, [])
  const label = selected ? selected.toLocaleDateString(undefined, { year: `numeric`, month: `short`, day: `numeric` }) : ``
  const pick = (d: Date) => {
    const next = iso(d)
    setValue(next)
    if (path) ctx.setData(path, next)
    void emit(`change`, { value: next })
    setOpen(false)
  }
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
        <PopoverPrimitive.Trigger id={id} {...(part(`trigger`, open && `open`) as Record<string, string>)} data-value={value || undefined}>
          <span>{label || <span {...(part(`placeholder`) as Record<string, string>)}>{str(props.placeholder, `Pick a date`)}</span>}</span>
          <Calendar aria-hidden="true" />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content {...(part(`calendar`, open && `open`) as Record<string, string>)} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="DatePicker">
            <div className="xui-calendar-head">
              <button type="button" className="xui-calendar-nav" aria-label="Previous month" onClick={() => setView(new Date(view.getFullYear(), view.getMonth() - 1, 1))}>
                <Prev aria-hidden="true" />
              </button>
              <span>{view.toLocaleDateString(undefined, { month: `long`, year: `numeric` })}</span>
              <button type="button" className="xui-calendar-nav" aria-label="Next month" onClick={() => setView(new Date(view.getFullYear(), view.getMonth() + 1, 1))}>
                <Next aria-hidden="true" />
              </button>
            </div>
            <div className="xui-calendar-grid" role="grid">
              {weekdays.map((w, i) => (
                <span key={i} className="xui-calendar-weekday" aria-hidden="true">
                  {w}
                </span>
              ))}
              {days.map((d) => {
                const key = iso(d)
                const isSel = selected ? iso(selected) === key : false
                const outside = d.getMonth() !== view.getMonth()
                const disabled = (min !== null && d < min) || (max !== null && d > max)
                return (
                  <button type="button" key={key} {...(part(`day`, isSel && `selected`, disabled && `disabled`) as Record<string, string>)} data-outside={outside ? `` : undefined} disabled={disabled} aria-selected={isSel} aria-label={d.toDateString()} onClick={() => pick(d)}>
                    {d.getDate()}
                  </button>
                )
              })}
            </div>
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
    </div>
  )
}

export function ComposerNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const { send, writeLocal } = useInputSender(node, scope, `value`)
  const external = str(props.value)
  const field = useHostOwnedValue<string>({ external, send, writeLocal })
  const busy = bool(props.busy)
  const ref = useRef<HTMLTextAreaElement>(null)
  const Send = CHROME.send
  const Stop = CHROME.stop
  const Attach = CHROME.attach
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
  return (
    <div {...(rootProps as Record<string, unknown>)} data-busy={busy ? `true` : undefined}>
      <textarea
        ref={ref}
        {...(part(`field`, field.focused && `focus`) as Record<string, string>)}
        rows={1}
        placeholder={str(props.placeholder, `Message`)}
        value={field.value}
        aria-label={str(props.placeholder, `Message`)}
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
          <button type="button" {...(part(`attachment`) as Record<string, string>)} aria-label="Attach">
            <Attach aria-hidden="true" />
          </button>
        ) : null}
        <button type="button" {...(part(`send`, busy && `pressed`) as Record<string, string>)} aria-label={busy ? `Stop` : str(props.submitLabel, `Send`)} disabled={!busy && field.value.trim() === ``} onClick={submit}>
          {busy ? <Stop aria-hidden="true" /> : <Send aria-hidden="true" />}
        </button>
      </div>
      <span className="xui-sr-only">{ctx.surfaceId}</span>
    </div>
  )
}
