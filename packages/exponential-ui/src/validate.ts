// VAPP-85: props validation against a component definition — the catalog's
// own mini schema (types.ts PropSchema), so the reducer, the fixtures and the
// generator agree without a JSON Schema engine. core.schema.json is the same
// rules for external consumers.

import { FUNCTION_NAMES, ICON_NAMES } from "./catalog.generated"
import { isKnownToken, parseTokenRef, sharedCatalogView } from "./catalog"
import { isCall, isDynamic } from "./expr"
import { isIdentifier } from "./ids"
import { isDataSchema } from "./dynamic"
import { BREAKPOINTS, isResponsiveValue } from "./macros"
import { isStringRef, parseStringRef } from "./strings"
import { validateStyle } from "./style"
import type { CatalogView } from "./catalog"
import type { ComponentDef, ExtensionDef, PropSchema } from "./types"

const ICONS: ReadonlySet<string> = new Set(ICON_NAMES)
const viewOf = (extensions?: readonly ExtensionDef[]): CatalogView => sharedCatalogView(extensions)
const FUNCTIONS: ReadonlySet<string> = new Set(FUNCTION_NAMES)
const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/

export interface PropIssue {
  path: string
  message: string
}

export { isDynamic }

function checkScalar(
  schema: PropSchema,
  value: unknown,
  path: string,
  view: CatalogView,
  issues: PropIssue[]
): void {
  switch (schema.type) {
    case `string`:
    case `markdown`:
    case `url`:
      if (typeof value !== `string`) issues.push({ path, message: `expected a string` })
      else if (parseStringRef(value) !== null && !isStringRef(value))
        issues.push({ path, message: `unknown built-in string ${value} (catalog/strings.json)` })
      return
    case `componentId`:
      // Round 9 §5.3: a component id reference; the router's entry check
      // refuses a non-identifier, the reducer a missing id.
      if (typeof value !== `string`) issues.push({ path, message: `expected a string` })
      return
    case `dynamic`:
      // Round 9 §6.6: a binding or a call, never a literal (a check's condition).
      if (!isDynamic(value)) issues.push({ path, message: `expected a binding or a function call` })
      return
    case `date`:
      if (typeof value !== `string` || (value !== `` && !ISO_DATE.test(value)))
        issues.push({ path, message: `expected yyyy-mm-dd` })
      return
    case `number`:
      if (typeof value !== `number` || !Number.isFinite(value))
        issues.push({ path, message: `expected a number` })
      else if (schema.minimum !== undefined && value < schema.minimum) issues.push({ path, message: `expected at least ${schema.minimum}` })
      else if (schema.maximum !== undefined && value > schema.maximum) issues.push({ path, message: `expected at most ${schema.maximum}` })
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
      // Round 4 (VAPP-103): a node's colour is a TOKEN, never a literal, so
      // the theme's light and dark modes both apply (themes keep literals).
      if (isKnownToken(value) && parseTokenRef(value)?.group === `color`) return
      issues.push({ path, message: `expected $color.<name>` })
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
      if (!schema.shape) return
      const def = view.defs[schema.shape]
      if (!def) {
        issues.push({ path, message: `shape ${schema.shape} is not defined` })
        return
      }
      checkProps(def.properties, value as Record<string, unknown>, path, view, issues)
      return
    }
  }
}

function checkValue(
  schema: PropSchema,
  value: unknown,
  path: string,
  view: CatalogView,
  issues: PropIssue[]
): void {
  if (schema.bindable && isDynamic(value)) return
  if (schema.responsive && isResponsiveValue(value)) {
    for (const [bp, v] of Object.entries(value)) checkScalar(schema, v, `${path}.${bp}`, view, issues)
    return
  }
  if (schema.responsive && typeof value === `object` && value !== null && !Array.isArray(value) && !isDynamic(value)) {
    issues.push({ path, message: `a responsive value needs base and only ${BREAKPOINTS.join(`|`)} besides` })
    return
  }
  checkScalar(schema, value, path, view, issues)
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

/** Round 9 §6.3: the reserved host function prefixes and exact host names. */
export const HOST_FUNCTION_PREFIXES: readonly string[] = [`actions_`, `harness_`, `exponential_`, `tools_`, `upstream_`, `camera_`, `clipboard_`, `files_`, `http_`, `location_`, `notifications_`, `photos_`, `print_`, `share_`, `app_`]
export const HOST_FUNCTION_NAMES: readonly string[] = [`openUrl`, `bind`, `app_navigate`, `app_toast`]

/** A catalog or HOST function name (an exact host name or a reserved host
 *  prefix), or `@index`: what a card or a package may call statically. */
export function isKnownFunctionName(name: string): boolean {
  return FUNCTIONS.has(name) || name === `@index` || HOST_FUNCTION_NAMES.includes(name) || HOST_FUNCTION_PREFIXES.some((p) => name.startsWith(p) && name.length > p.length)
}

/** Round 9 §6.5: a name a SURFACE may call: any known name, or any other
 *  identifier (an agent function, routed when the surface has an agent). */
export function isCallableName(name: string): boolean {
  return isKnownFunctionName(name) || isIdentifier(name)
}

function checkCall(value: { "@call": string }, path: string, issues: PropIssue[]): void {
  if (!isCallableName(value[`@call`]))
    issues.push({ path, message: `unknown function "${value[`@call`]}"; known: the catalog functions, a host function (app_toast), or an agent function (an identifier)` })
}

/** Every `{call}` at any depth of an EXPRESSION (`visible`, a call's args,
 *  an event context): each resolves at any depth (`resolveDynamic`), so
 *  every `{call}` in it is a call. */
function checkCallsDeep(value: unknown, path: string, issues: PropIssue[]): void {
  if (Array.isArray(value)) {
    value.forEach((v, i) => checkCallsDeep(v, `${path}[${i}]`, issues))
    return
  }
  if (typeof value !== `object` || value === null) return
  if (isCall(value)) checkCall(value, path, issues)
  for (const [k, v] of Object.entries(value)) checkCallsDeep(v, `${path}.${k}`, issues)
}

/** The calls a PROP makes, walked along its schema exactly as the bind
 *  pass resolves it (`resolveProp`): a `{call}` AT a position is a call
 *  (its args are expressions); a DATA position (Table `rows`: shape-less
 *  objects) is the author's literal data, never descended (a row
 *  `{"@call": "+1 555 0100"}` is a row); arrays and shaped objects walk per
 *  item / property; a `style` prop resolves at any depth; anything else is
 *  a literal. */
function checkPropCalls(value: unknown, schema: PropSchema | undefined, path: string, view: CatalogView, issues: PropIssue[]): void {
  if (isCall(value)) {
    checkCall(value, path, issues)
    checkCallsDeep(value.args, `${path}.args`, issues)
    return
  }
  if (!schema || isDataSchema(schema)) return
  if (schema.type === `style`) return checkCallsDeep(value, path, issues)
  if (schema.type === `array` && schema.items && Array.isArray(value)) {
    value.forEach((item, i) => checkPropCalls(item, schema.items, `${path}[${i}]`, view, issues))
    return
  }
  const shape = schema.type === `object` && schema.shape ? view.defs[schema.shape] : undefined
  if (shape && typeof value === `object` && value !== null && !Array.isArray(value))
    for (const [k, v] of Object.entries(value)) checkPropCalls(v, shape.properties[k], `${path}.${k}`, view, issues)
}

/** The calls an ACTION makes: its `functionCall` (name + args) and its
 *  event context (an expression). Nothing else in it is evaluated. */
function checkActionCalls(action: unknown, path: string, issues: PropIssue[]): void {
  if (typeof action !== `object` || action === null) return
  const { functionCall, event } = action as { functionCall?: unknown; event?: unknown }
  if (isCall(functionCall)) {
    checkCall(functionCall, `${path}.functionCall`, issues)
    checkCallsDeep(functionCall.args, `${path}.functionCall.args`, issues)
  }
  if (typeof event === `object` && event !== null) checkCallsDeep((event as { context?: unknown }).context, `${path}.event.context`, issues)
}

/** What validateProps leaves out, round 4 (VAPP-103): the node's `style`
 *  (whitelisted keys, known tokens, `$color.*` colours), every function a
 *  prop (along its schema: never inside DATA), `visible` or an `on` action
 *  calls, and a Table's slot columns (each `type: slot` column names one of
 *  the node's slots). In this order; the Rust core
 *  (`validate::validate_node`) reports the same issues. */
export function validateNode(
  node: { component: string; props: Record<string, unknown>; style?: unknown; visible?: unknown; on?: Record<string, unknown>; slots?: Record<string, unknown> },
  options: { extensions?: readonly ExtensionDef[] } = {}
): PropIssue[] {
  const issues: PropIssue[] = []
  const view = viewOf(options.extensions)
  const def = view.components[node.component]
  if (node.style !== undefined) for (const issue of validateStyle(node.style, { path: `style` })) issues.push(issue)
  for (const [k, v] of Object.entries(node.props)) checkPropCalls(v, def?.props[k], `props.${k}`, view, issues)
  if (node.visible !== undefined) checkCallsDeep(node.visible, `visible`, issues)
  for (const [event, action] of Object.entries(node.on ?? {})) checkActionCalls(action, `on.${event}`, issues)
  if (node.component === `Table` && Array.isArray(node.props.columns)) {
    const slots = Object.keys(node.slots ?? {})
    node.props.columns.forEach((column, i) => {
      if (typeof column !== `object` || column === null || (column as { type?: unknown }).type !== `slot`) return
      const slot = (column as { slot?: unknown }).slot
      if (typeof slot !== `string`) issues.push({ path: `props.columns[${i}].slot`, message: `a slot column names one of the Table's slots` })
      else if (!slots.includes(slot)) issues.push({ path: `props.columns[${i}].slot`, message: `no slot "${slot}" on this Table; slots: ${slots.join(`|`) || `none`}` })
    })
  }
  return issues
}

/** Every issue in a node's props against its component definition. */
export function validateProps(
  def: ComponentDef,
  props: Record<string, unknown>,
  options: { path?: string; extensions?: readonly ExtensionDef[] } = {}
): PropIssue[] {
  const issues: PropIssue[] = []
  checkProps(def.props, props, options.path ?? `props`, viewOf(options.extensions), issues)
  return issues
}
