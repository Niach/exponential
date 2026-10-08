// VAPP-85: props validation against a component definition — the catalog's
// own mini schema (types.ts PropSchema), so the reducer, the fixtures and the
// generator agree without a JSON Schema engine. core.schema.json is the same
// rules for external consumers.

import { ICON_NAMES } from "./catalog.generated"
import { catalogView, isKnownToken, parseTokenRef } from "./catalog"
import { validateStyle } from "./style"
import type { CatalogView } from "./catalog"
import type { ComponentDef, ExtensionDef, PropSchema } from "./types"

const ICONS: ReadonlySet<string> = new Set(ICON_NAMES)
const HEX = /^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8}|[0-9a-fA-F]{3,4})$/
const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/

export interface PropIssue {
  path: string
  message: string
}

/** An A2UI dynamic value: a data binding or a function call. */
export function isDynamic(value: unknown): boolean {
  if (typeof value !== `object` || value === null || Array.isArray(value)) return false
  const obj = value as Record<string, unknown>
  if (typeof obj.path === `string` && Object.keys(obj).length === 1) return true
  return typeof obj.call === `string`
}

function checkValue(
  schema: PropSchema,
  value: unknown,
  path: string,
  view: CatalogView,
  issues: PropIssue[]
): void {
  if (schema.bindable && isDynamic(value)) return
  switch (schema.type) {
    case `string`:
    case `markdown`:
    case `url`:
      if (typeof value !== `string`) issues.push({ path, message: `expected a string` })
      return
    case `date`:
      if (typeof value !== `string` || (value !== `` && !ISO_DATE.test(value)))
        issues.push({ path, message: `expected yyyy-mm-dd` })
      return
    case `number`:
      if (typeof value !== `number` || !Number.isFinite(value))
        issues.push({ path, message: `expected a number` })
      return
    case `boolean`:
      if (typeof value !== `boolean`) issues.push({ path, message: `expected a boolean` })
      return
    case `enum`: {
      const values = schema.values ?? (schema.enum ? view.enums[schema.enum] : undefined)
      if (!values) issues.push({ path, message: `enum ${schema.enum} is not defined` })
      else if (!values.includes(value as string))
        issues.push({ path, message: `expected one of ${values.join(`|`)}` })
      return
    }
    case `icon`:
      if (typeof value !== `string` || !ICONS.has(value))
        issues.push({ path, message: `not an icons.json name` })
      return
    case `color`:
      if (typeof value === `string` && HEX.test(value)) return
      if (isKnownToken(value) && parseTokenRef(value)?.group === `color`) return
      issues.push({ path, message: `expected #hex or $color.<name>` })
      return
    case `style`:
      for (const issue of validateStyle(value, { path, root: false }))
        issues.push(issue)
      return
    case `array`:
      if (!Array.isArray(value)) {
        issues.push({ path, message: `expected an array` })
        return
      }
      if (schema.items)
        value.forEach((item, i) => checkValue(schema.items!, item, `${path}[${i}]`, view, issues))
      return
    case `object`: {
      if (typeof value !== `object` || value === null || Array.isArray(value)) {
        issues.push({ path, message: `expected an object` })
        return
      }
      const def = schema.shape ? view.defs[schema.shape] : undefined
      if (!def) {
        issues.push({ path, message: `shape ${schema.shape} is not defined` })
        return
      }
      checkProps(def.properties, value as Record<string, unknown>, path, view, issues)
      return
    }
  }
}

function checkProps(
  schemas: Record<string, PropSchema>,
  props: Record<string, unknown>,
  path: string,
  view: CatalogView,
  issues: PropIssue[]
): void {
  for (const [name, schema] of Object.entries(schemas)) {
    const value = props[name]
    if (value === undefined || value === null) {
      if (schema.required) issues.push({ path: `${path}.${name}`, message: `required` })
      continue
    }
    checkValue(schema, value, `${path}.${name}`, view, issues)
  }
  for (const name of Object.keys(props)) {
    if (!(name in schemas)) issues.push({ path: `${path}.${name}`, message: `unknown prop` })
  }
}

/** Every issue in a node's props against its component definition. */
export function validateProps(
  def: ComponentDef,
  props: Record<string, unknown>,
  options: { path?: string; extensions?: readonly ExtensionDef[] } = {}
): PropIssue[] {
  const issues: PropIssue[] = []
  checkProps(def.props, props, options.path ?? `props`, catalogView(options.extensions), issues)
  return issues
}
