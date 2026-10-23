// Round 1 (contract §3 Form): the FORM native and the field protocol every
// named control speaks. A Form collects every field beneath it that has a
// `name` (a nested Form collects its own). Submit — a `submit` Button or
// Enter in a single-line field — runs EVERY field's checks regardless of
// `validateOn`; any failure: no `submit`, `invalid {errors}` fires, every
// failing field shows ALL its messages (id `<field id>.error`, linked by
// aria-describedby), `summary` also lists them above the fields, focus
// moves to the first invalid field (document order) and `invalidFields` is
// announced. `busy` refuses submits (the submit Button shows loading),
// `disabled` makes every field inert (a disabled fieldset).

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { FormRegistry, fieldErrorsVisible, joinIds, validateOnDefault } from "@exponential_app/ui/web"
import { useSurfaceContext } from "./context"
import type { NativeProps } from "./node-view"
import { bool, failingChecks, str, useParts, type PartFn } from "./natives/shared"

export interface FieldHandle {
  name: string
  domId: string
  value: () => unknown
  errors: () => string[]
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
  /** The messages to SHOW now (all failing checks once validation shows). */
  errors: string[]
  invalid: boolean
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
  const all = failingChecks(props.checks, ctx.t(`invalidValue`))
  const [touched, setTouched] = useState(false)
  const [dirty, setDirty] = useState(false)
  const [submitted, setSubmitted] = useState(false)
  const on = str(props.validateOn, validateOnDefault(node.component) ?? `blur`)
  const show = fieldErrorsVisible({ validateOn: on, submitted, attempts: form?.attempts ?? 0, dirty, touched })
  const latest = useRef({ value, all })
  latest.current = { value, all }
  const name = str(props.name)
  useEffect(() => {
    if (!form || !name) return
    return form.register({
      name,
      domId,
      value: () => latest.current.value,
      errors: () => latest.current.all,
      focus: () => focusRef.current?.focus(),
      element: () => focusRef.current,
    })
  }, [form, name, domId, focusRef])
  const errors = show ? all : []
  const errorId = `${domId}.error`
  return {
    errors,
    invalid: errors.length > 0,
    errorId,
    describedBy: errors.length ? errorId : undefined,
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

/** Every failing message under a field, linked by `errorId`. */
export function FieldErrors({ part, field }: { part: PartFn; field: FieldState }) {
  if (!field.errors.length) return null
  return (
    <div {...(part(`error`) as Record<string, string>)} id={field.errorId} role="alert">
      {field.errors.map((m, i) => (
        <span key={i} className="xui-field-error">
          {m}
        </span>
      ))}
    </div>
  )
}

export { joinIds }

export type { ReactNode }
