// Round 8 (docs/round-8-contract.md §2): what a Checkbox press does, the ONE
// rule every renderer applies (fixtures/interactions-round1.json `checkbox`;
// the Rust core: surface/events.rs). A mixed box resolves to checked; else
// the press toggles. `change` carries both values; `indeterminate` is
// written (to its binding) only when it was set, so an unbound or unseeded
// path stays untouched by an ordinary toggle.

export interface CheckboxPress {
  checked: boolean
  indeterminate: false
  /** Whether the press writes `indeterminate: false` (it was true). */
  writesIndeterminate: boolean
  /** The `change` event's payload. */
  change: { checked: boolean; indeterminate: false }
}

export function checkboxPress(checked: unknown, indeterminate: unknown): CheckboxPress {
  const mixed = indeterminate === true
  const next = mixed ? true : checked !== true
  return { checked: next, indeterminate: false, writesIndeterminate: mixed, change: { checked: next, indeterminate: false } }
}

/** The accessibility checked state: `mixed` wins over `checked`. */
export function checkboxAriaChecked(checked: unknown, indeterminate: unknown): `true` | `false` | `mixed` {
  return indeterminate === true ? `mixed` : checked === true ? `true` : `false`
}
