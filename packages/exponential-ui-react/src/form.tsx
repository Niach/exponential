// Round 1 (contract §3 Form): the FORM native and the field protocol every
// named control speaks. A Form collects every field beneath it that has a
// `name` (a nested Form collects its own). Submit — a `submit` Button or
// Enter in a single-line field — runs EVERY field's checks regardless of
// `validateOn`; any failing `error`: no `submit`, `invalid {errors}` fires,
// every failing field shows ONE message (round 9 §6.6: its first `error`,
// else `warning`, else `info`; id `<field id>.error`, linked by
// aria-describedby), `summary` lists every error above the fields, focus
// moves to the first invalid field (document order) and `invalidFields` is
// announced. A `warning` / `info` never blocks. `busy` refuses submits (the submit Button shows loading),
// `disabled` makes every field inert (a disabled fieldset).

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { FormRegistry, fieldErrorsVisible, joinIds, validateOnDefault } from "@exponential_app/eui/web"
import type { FieldChecks } from "@exponential_app/eui"
import { useSurfaceContext } from "./context"
import type { NativeProps } from "./node-view"
import { bool, resolvedFieldChecks, str, useParts, type PartFn } from "./natives/shared"

export interface FieldHandle {
  name: string
  domId: string
  value: () => unknown
  errors: () => string[]
  advisory: () => boolean
  focus: () => void
  element: () => HTMLElement | null
}

export interface FormContextValue {
  register: (field: FieldHandle) => () => void
  /** Run the checks and fire `submit` or `invalid`. */
  submit: () => void
  /** A field's value changed (Form `change {name, value}`). */
  changed: (name: string, value: unknown) => void
  disabled: boolean
  busy: boolean
  /** Bumps on every refused submit: every field shows its errors. */
  attempts: number
}

export const FormContext = createContext<FormContextValue | null>(null)

/** Round 4 (VAPP-103, `submitClosesOverlay`): the NEAREST overlay around a
 *  Form decides — a dismissible Dialog or Drawer provides its close, every
 *  other overlay (a Popover, a non-dismissible Dialog = AlertDialog)
 *  provides null; a valid Form submit beneath calls it right after
 *  `submit` (the author never resets the bound `open` flag). */
export const CloseOnSubmitContext = createContext<(() => void) | null>(null)

/** #EXP-1760: an IconPicker / ColorPicker's Popover provides its close; a
 *  pick beneath (`isPickerPick`: a cell, a swatch, the No icon row) calls it
 *  right after `press`, as the core and the IDE's pickers do. */
export const ClosePickerContext = createContext<(() => void) | null>(null)

export function useForm(): FormContextValue | null {
  return useContext(FormContext)
}

export function FormNative({ node, props, rootProps, emit, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const closeOverlay = useContext(CloseOnSubmitContext)
  const registry = useRef<FormRegistry<FieldHandle> | null>(null)
  if (!registry.current) registry.current = new FormRegistry<FieldHandle>()
  const fields = registry.current
  const [attempts, setAttempts] = useState(0)
  const [summary, setSummary] = useState<{ name: string; message: string }[]>([])
  const disabled = bool(props.disabled)
  const busy = bool(props.busy)
  const busyRef = useRef(busy)
  busyRef.current = busy
  const register = useCallback((field: FieldHandle) => fields.register(field), [fields])
  const submit = useCallback(() => {
    if (busyRef.current) return
    const result = fields.check()
    if (!result.ok) {
      setAttempts((n) => n + 1)
      setSummary(result.errors)
      result.first?.focus()
      ctx.announce(ctx.t(`invalidFields`), `assertive`)
      void emit(`invalid`, { errors: result.errors })
      return
    }
    setSummary([])
    // A submit reveals the advisories it let through (round 9 §6.6).
    if (result.advised) setAttempts((n) => n + 1)
    void emit(`submit`, { values: result.values })
    closeOverlay?.()
  }, [emit, ctx, closeOverlay, fields])
  const changed = useCallback((name: string, value: unknown) => void emit(`change`, { name, value }), [emit])
  const value = useMemo<FormContextValue>(() => ({ register, submit, changed, disabled, busy, attempts }), [register, submit, changed, disabled, busy, attempts])
  // Summary entries clear as their fields become valid.
  const live = fields.stillFailing(summary)
  return (
    <FormContext.Provider value={value}>
      <form
        {...(rootProps as Record<string, unknown>)}
        noValidate
        aria-busy={busy || undefined}
        aria-disabled={disabled || undefined}
        name={str(props.name) || undefined}
        onSubmit={(e) => {
          e.preventDefault()
          e.stopPropagation()
          submit()
        }}
      >
        {bool(props.summary) && live.length ? (
          <div {...(part(`summary`) as Record<string, string>)} role="alert" id={`${node.id}.summary`}>
            <span className="xui-sr-only">{ctx.t(`invalidFields`)}</span>
            <ul className="xui-form-summary-list">
              {live.map((e, i) => (
                <li key={`${e.name}-${i}`}>{e.message}</li>
              ))}
            </ul>
          </div>
        ) : null}
        <fieldset className="xui-form-fields" disabled={disabled}>
          {children}
        </fieldset>
      </form>
    </FormContext.Provider>
  )
}

export interface FieldOptions {
  node: NativeProps[`node`]
  domId: string
  props: Record<string, unknown>
  /** The field's current (local) value. */
  value: unknown
  /** The element focus moves to when the field is the first invalid one. */
  focusRef: { readonly current: HTMLElement | null }
}

export interface FieldState {
  /** The `error` message to SHOW now (at most one, round 9 §6.6). */
  errors: string[]
  invalid: boolean
  /** The one message shown now (any severity), null = none. */
  shown: FieldChecks[`shown`]
  /** `<field id>.error` — put it in the control's aria-describedby. */
  errorId: string
  describedBy: string | undefined
  /** Report a blur / a change / an Enter submit. */
  touch: () => void
  change: (value: unknown) => void
  submit: () => void
  form: FormContextValue | null
}

/** A named control's validation + its registration with the nearest Form. */
export function useField({ node, domId, props, value, focusRef }: FieldOptions): FieldState {
  const form = useForm()
  const ctx = useSurfaceContext()
  const checks = resolvedFieldChecks(props.checks, ctx.t(`checkFailed`))
  const all = checks.errors
  const advised = checks.shown !== null && checks.shown.severity !== `error`
  const [touched, setTouched] = useState(false)
  const [dirty, setDirty] = useState(false)
  const [submitted, setSubmitted] = useState(false)
  const on = str(props.validateOn, validateOnDefault(node.component) ?? `blur`)
  const show = fieldErrorsVisible({ validateOn: on, submitted, attempts: form?.attempts ?? 0, dirty, touched })
  const latest = useRef({ value, all, advised })
  latest.current = { value, all, advised }
  const name = str(props.name)
  useEffect(() => {
    if (!form || !name) return
    return form.register({
      name,
      domId,
      value: () => latest.current.value,
      errors: () => latest.current.all,
      advisory: () => latest.current.advised,
      focus: () => focusRef.current?.focus(),
      element: () => focusRef.current,
    })
  }, [form, name, domId, focusRef])
  const shown = show ? checks.shown : null
  const errors = shown?.severity === `error` ? [shown.message] : []
  const errorId = `${domId}.error`
  return {
    errors,
    invalid: errors.length > 0,
    shown,
    errorId,
    describedBy: shown ? errorId : undefined,
    touch: () => setTouched(true),
    change: (v: unknown) => {
      setDirty(true)
      if (form && name) form.changed(name, v)
    },
    submit: () => {
      setSubmitted(true)
      form?.submit()
    },
    form,
  }
}

/** The field's one message, linked by `errorId`: an `error` is an alert in
 *  the `error` part; a `warning` / `info` the same part in that recipe state,
 *  a quiet `status`. */
export function FieldErrors({ part, field }: { part: PartFn; field: FieldState }) {
  const shown = field.shown
  if (!shown) return null
  const severe = shown.severity === `error`
  return (
    <div {...(part(`error`, !severe && shown.severity) as Record<string, string>)} id={field.errorId} role={severe ? `alert` : `status`}>
      <span className="xui-field-error">{shown.message}</span>
    </div>
  )
}

export { joinIds }

export type { ReactNode }
