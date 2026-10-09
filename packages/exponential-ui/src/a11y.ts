// Round 1 (docs/round-1-contract.md §6): the accessibility contract as data
// (catalog/a11y.json): the machine-readable role vocabulary, per component
// its role, prose notes and keyboard expectations, the rules every component
// keeps, and the commands a host may send a surface. Macro parts carry their
// role, states and name on the expanded node (`accessibility`, from the
// template's `$a11y`, src/macros.ts).

import a11yJson from "../catalog/a11y.json" with { type: "json" }

export interface ComponentA11y {
  /** The role of the component's main accessible object: one of `A11Y_ROLES`. */
  role: string
  /** Prose: composite parts, states, which built-in strings name it. */
  notes: string
  /** Keyboard expectations, one line per key group. */
  keys: readonly string[]
}

/** The machine-readable roles: ARIA names, plus `text` (static text) and
 *  `hidden` (out of the accessibility tree). */
export const A11Y_ROLES: readonly string[] = a11yJson.roles
export const A11Y_RULES: readonly string[] = a11yJson.rules
export const A11Y_COMMANDS: Readonly<Record<string, { args: readonly string[]; description: string }>> = a11yJson.commands
export const COMPONENT_A11Y: Readonly<Record<string, ComponentA11y>> = a11yJson.components

/** A component's entry, if it has one (extensions declare their own). */
export function componentA11y(name: string): ComponentA11y | undefined {
  return COMPONENT_A11Y[name]
}

/** True for a role of the vocabulary. */
export function isA11yRole(value: unknown): boolean {
  return typeof value === `string` && A11Y_ROLES.includes(value)
}
