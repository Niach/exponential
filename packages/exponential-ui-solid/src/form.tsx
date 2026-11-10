// FEED-128: the FORM native and the field protocol every named control
// speaks, on Solid (React's form.tsx; round 1 contract §3). A Form collects
// every field beneath it that has a `name`. Submit runs EVERY field's
// checks; any failure: no `submit`, `invalid {errors}` fires, every failing
// field shows ALL its messages (id `<field id>.error`), `summary` lists
// them above the fields, focus moves to the first invalid field and
// `invalidFields` is announced. `busy` refuses submits, `disabled` makes
// every field inert (a disabled fieldset). The registry is the web layer's.

import { For, Show, createContext, createMemo, createSignal, createEffect, onCleanup, untrack, useContext, type Accessor, type JSX } from "solid-js"
import { FormRegistry, fieldErrorsVisible, joinIds, validateOnDefault } from "@exponential_app/eui/web"
import type { FieldChecks } from "@exponential_app/eui"
import type { UiNode } from "@exponential_app/eui"
import { useSurfaceContext } from "./context"
import type { NativeProps } from "./node-view"
import { bool, parts, resolvedFieldChecks, str, type PartFn } from "./natives/shared"

export interface FieldHandle {
  name: string
  domId: string
  value: () => unknown
  errors: () => string[]
  advisory: () => boolean
  focus: () => void
  element: () => HTMLElement | null
}

/** The nearest Form (live getters for its state). */
export interface FormContextValue {
  register: (field: FieldHandle) => () => void
  /** Run the checks and fire `submit` or `invalid`. */
  submit: () => void
  /** A field's value changed (Form `change {name, value}`). */
  changed: (name: string, value: unknown) => void
  readonly disabled: boolean
  readonly busy: boolean
  /** Bumps on every refused submit: every field shows its errors. */
  readonly attempts: number
}

export const FormContext = createContext<FormContextValue | null>(null)

/** Round 4 (`submitClosesOverlay`): the NEAREST overlay around a Form
 *  decides; a dismissible Dialog or Drawer provides its close, every other
 *  overlay provides null. */
export const CloseOnSubmitContext = createContext<(() => void) | null>(null)

/** #EXP-1760: an IconPicker / ColorPicker's Popover provides its close; a
 *  pick beneath (`isPickerPick`) calls it right after `press`. */
export const ClosePickerContext = createContext<(() => void) | null>(null)

export function useForm(): FormContextValue | null {
  return useContext(FormContext) ?? null
}

export function FormNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const closeOverlay = useContext(CloseOnSubmitContext) ?? null
  const fields = new FormRegistry<FieldHandle>()
  const [attempts, setAttempts] = createSignal(0)
  const [summary, setSummary] = createSignal<{ name: string; message: string }[]>([])
  const disabled = () => bool(props.props.disabled)
  const busy = () => bool(props.props.busy)
  const submit = () => {
    if (untrack(busy)) return
    const result = fields.check()
    if (!result.ok) {
      setAttempts((n) => n + 1)
      setSummary(result.errors)
      result.first?.focus()
      ctx.announce(ctx.t(`invalidFields`), `assertive`)
      void props.emit(`invalid`, { errors: result.errors })
      return
    }
    setSummary([])
    // A submit reveals the advisories it let through (round 9 §6.6).
    if (result.advised) setAttempts((n) => n + 1)
    void props.emit(`submit`, { values: result.values })
    closeOverlay?.()
  }
  const value: FormContextValue = {
    register: (field) => fields.register(field),
    submit,
    changed: (name, v) => void props.emit(`change`, { name, value: v }),
    get disabled() {
      return disabled()
    },
    get busy() {
      return busy()
    },
    get attempts() {
      return attempts()
    },
  }
  // Summary entries clear as their fields become valid (re-read with the
  // data and the attempts).
  const live = createMemo(() => {
    ctx.data
    attempts()
    return fields.stillFailing(summary())
  })
  return (
    <FormContext.Provider value={value}>
      <form
        {...props.rootProps}
        novalidate
        aria-busy={busy() || undefined}
        aria-disabled={disabled() || undefined}
        name={str(props.props.name) || undefined}
        onSubmit={(e) => {
          e.preventDefault()
          e.stopPropagation()
          submit()
        }}
      >
        <Show when={bool(props.props.summary) && live().length}>
          <div {...part(`summary`)} role="alert" id={`${props.node.id}.summary`}>
            <span class="xui-sr-only">{ctx.t(`invalidFields`)}</span>
            <ul class="xui-form-summary-list">
              <For each={live()}>{(e) => <li>{e.message}</li>}</For>
            </ul>
          </div>
        </Show>
        <fieldset class="xui-form-fields" disabled={disabled()}>
          {props.children}
        </fieldset>
      </form>
    </FormContext.Provider>
  )
}

export interface FieldOptions {
  node: Accessor<UiNode>
  domId: Accessor<string>
  props: Accessor<Record<string, unknown>>
  /** The field's current (local) value. */
  value: Accessor<unknown>
  /** The element focus moves to when the field is the first invalid one. */
  focusEl: () => HTMLElement | null | undefined
}

/** A field's validation state (live getters). */
export interface FieldState {
  /** The `error` message to SHOW now (at most one, round 9 §6.6). */
  readonly errors: string[]
  readonly invalid: boolean
  /** The one message shown now (any severity), null = none. */
  readonly shown: FieldChecks[`shown`]
  /** `<field id>.error` — put it in the control's aria-describedby. */
  readonly errorId: string
  readonly describedBy: string | undefined
  touch: () => void
  change: (value: unknown) => void
  submit: () => void
  readonly form: FormContextValue | null
}

/** React's `useField`: a named control's validation + its registration
 *  with the nearest Form. */
export function createField(options: FieldOptions): FieldState {
  const form = useForm()
  const ctx = useSurfaceContext()
  const checks = createMemo(() => resolvedFieldChecks(options.props().checks, ctx.t(`checkFailed`)))
  const all = () => checks().errors
  const [touched, setTouched] = createSignal(false)
  const [dirty, setDirty] = createSignal(false)
  const [submitted, setSubmitted] = createSignal(false)
  const show = createMemo(() => fieldErrorsVisible({ validateOn: str(options.props().validateOn, validateOnDefault(options.node().component) ?? `blur`), submitted: submitted(), attempts: form?.attempts ?? 0, dirty: dirty(), touched: touched() }))
  const name = createMemo(() => str(options.props().name))
  createEffect(() => {
    const n = name()
    const id = options.domId()
    if (!form || !n) return
    onCleanup(
      form.register({
        name: n,
        domId: id,
        value: () => untrack(options.value),
        errors: () => untrack(all),
        advisory: () => {
          const shown = untrack(checks).shown
          return shown !== null && shown.severity !== `error`
        },
        focus: () => options.focusEl()?.focus(),
        element: () => options.focusEl() ?? null,
      })
    )
  })
  const shown = createMemo(() => (show() ? checks().shown : null))
  const errors = createMemo(() => {
    const s = shown()
    return s?.severity === `error` ? [s.message] : []
  })
  const errorId = () => `${options.domId()}.error`
  return {
    get errors() {
      return errors()
    },
    get invalid() {
      return errors().length > 0
    },
    get shown() {
      return shown()
    },
    get errorId() {
      return errorId()
    },
    get describedBy() {
      return shown() ? errorId() : undefined
    },
    touch: () => setTouched(true),
    change: (v: unknown) => {
      setDirty(true)
      const n = untrack(name)
      if (form && n) form.changed(n, v)
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
export function FieldErrors(props: { part: PartFn; field: FieldState }): JSX.Element {
  return (
    <Show when={props.field.shown}>
      {(shown) => (
        <div {...props.part(`error`, shown().severity !== `error` && shown().severity)} id={props.field.errorId} role={shown().severity === `error` ? `alert` : `status`}>
          <span class="xui-field-error">{shown().message}</span>
        </div>
      )}
    </Show>
  )
}

export { joinIds }
