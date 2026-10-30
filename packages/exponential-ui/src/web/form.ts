// Round 1 (contract §3 Form; FEED-128: hoisted from the React renderer's
// form.tsx): the field REGISTRY a Form keeps and the rules around it,
// framework-free. A Form collects every named field beneath it; submit runs
// EVERY field's checks regardless of `validateOn` and, on any failure,
// yields every message (document order) and the first invalid field to
// focus; otherwise the values by name. A field shows its errors once it was
// submitted, the Form refused a submit, or its `validateOn` moment passed.

import { coreCatalog } from "@exponential_app/eui"

/** One registered field (a renderer's handle onto its control). */
export interface FormField {
  name: string
  domId: string
  value: () => unknown
  errors: () => string[]
  focus: () => void
  element: () => Element | null
}

export interface FormError {
  name: string
  message: string
}

export type FormCheck<F extends FormField = FormField> = { ok: false; errors: FormError[]; first: F | null } | { ok: true; values: Record<string, unknown> }

const VALIDATE_ON: Record<string, string> = Object.fromEntries(
  Object.entries(coreCatalog.components).flatMap(([name, def]) => {
    const d = (def.props as Record<string, { default?: unknown }>).validateOn?.default
    return d === undefined ? [] : [[name, String(d)]]
  })
)

/** A component's catalog default `validateOn`, or undefined. */
export function validateOnDefault(component: string): string | undefined {
  return VALIDATE_ON[component]
}

/** Whether a field shows its failing messages now. */
export function fieldErrorsVisible(state: { validateOn: string; submitted: boolean; attempts: number; dirty: boolean; touched: boolean }): boolean {
  return state.submitted || state.attempts > 0 || (state.validateOn === `change` ? state.dirty : state.validateOn === `blur` ? state.touched : false)
}

/** Fields in document order (unmounted ones keep their place). */
export function documentOrder<F extends FormField>(fields: Iterable<F>): F[] {
  return [...fields].sort((a, b) => {
    const ea = a.element()
    const eb = b.element()
    if (!ea || !eb) return 0
    return ea.compareDocumentPosition(eb) & 4 /* Node.DOCUMENT_POSITION_FOLLOWING */ ? -1 : 1
  })
}

export class FormRegistry<F extends FormField = FormField> {
  readonly fields = new Set<F>()

  /** Adds a field; the returned function removes it. */
  register(field: F): () => void {
    this.fields.add(field)
    return () => void this.fields.delete(field)
  }

  /** Every field's checks: the failures (document order) and the first
   *  invalid field, or the values by name. */
  check(): FormCheck<F> {
    const list = documentOrder(this.fields)
    const errors: FormError[] = []
    let first: F | null = null
    for (const f of list) {
      const errs = f.errors()
      if (errs.length && !first) first = f
      for (const message of errs) errors.push({ name: f.name, message })
    }
    if (errors.length) return { ok: false, errors, first }
    const values: Record<string, unknown> = {}
    for (const f of list) values[f.name] = f.value()
    return { ok: true, values }
  }

  /** The summary entries whose field still fails with that message (they
   *  clear as their fields become valid). */
  stillFailing(summary: readonly FormError[]): FormError[] {
    return summary.filter((e) => [...this.fields].some((f) => f.name === e.name && f.errors().includes(e.message)))
  }
}
