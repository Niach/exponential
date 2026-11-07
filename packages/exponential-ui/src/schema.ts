// VAPP-85, round 9 (docs/round-9-contract.md §11): the catalog files. ONE
// generator emits A2UI 1.0 catalog definitions (vendor/a2ui/v1_0/json/
// catalog_definition.json): the core (catalog/core.schema.json), the core-lite
// subset (catalog/core-lite.schema.json) and an extension's file
// (`extensionSchema`, also what `inlineCatalogs` carries). Components are
// FLAT (`component.const` + the declared props of §5.3), functions follow
// §6.1, `$ref`s only reach common_types.json, this file's components /
// functions and our own `$defs` leaves. `checkCatalogDefinition` is the
// hand-written structural checker (§11 Lint): generation runs it and throws
// on any issue, and src/schema.test.ts runs it over every file.

import styleJson from "../generated/style.runtime.json" with { type: "json" }
import vendoredBasic from "../vendor/a2ui/v1_0/catalogs/basic/v1/catalog.json" with { type: "json" }
import { catalogView, coreCatalog, TOKEN_GROUPS } from "./catalog"
import { ICON_NAMES } from "./catalog.generated"
import { isIdentifier } from "./ids"
import type { CatalogView } from "./catalog"
import type { ComponentDef, DefSchema, ExtensionDef, PropSchema } from "./types"

type Json = Record<string, unknown>

/** The 1.0 catalog definition's root keys (§11). */
export const CATALOG_ROOT_KEYS: readonly string[] = [`$schema`, `$id`, `protocolVersion`, `title`, `description`, `catalogId`, `metadata`, `instructions`, `components`, `functions`, `$defs`]
/** The common_types.json definitions a catalog file may `$ref` (§11). */
export const COMMON_TYPE_REFS: readonly string[] = [`ComponentId`, `Child`, `ChildList`, `DynamicString`, `DynamicNumber`, `DynamicBoolean`, `DynamicStringList`, `DynamicValue`, `AccessibilityAttributes`, `CheckRule`, `Checkable`, `Action`, `DataBinding`, `FunctionCall`]
/** §6.3: a catalog function never starts with one of these (they are host functions). */
export const RESERVED_FUNCTION_PREFIXES: readonly string[] = [`actions_`, `harness_`, `exponential_`, `tools_`, `upstream_`, `camera_`, `clipboard_`, `files_`, `http_`, `location_`, `notifications_`, `photos_`, `print_`, `share_`, `app_`]
/** §6.1 `returnType`. */
export const RETURN_TYPES: readonly string[] = [`string`, `number`, `boolean`, `array`, `object`, `validationResult`, `any`, `void`]
const CALLERS: readonly string[] = [`rendererOnly`, `agentOnly`, `rendererOrAgent`]
const FUNCTION_KEYS: readonly string[] = [`type`, `description`, `properties`, `required`, `returnType`, `allowedCallers`, `requiresUserActivation`]
/** §6.1: the catalog functions only the renderer calls; `openUrl` also needs a user gesture. */
const RENDERER_ONLY: ReadonlySet<string> = new Set([`set`, `bind`, `openUrl`])
const USER_ACTIVATION: ReadonlySet<string> = new Set([`openUrl`])
/** §5.1: the entry envelope: never a declared prop. */
const ENVELOPE: ReadonlySet<string> = new Set([`id`, `component`, `catalogId`, `accessibility`, `metadata`])

const SCHEMA_URI = `https://json-schema.org/draft/2020-12/schema`
const PROTOCOL_VERSION = `1.0`
const TOKEN_PATTERN = `^\\$[a-z][a-zA-Z0-9.]*\\.[a-zA-Z0-9]+$`
const common = (name: string): Json => ({ $ref: `common_types.json#/$defs/${name}` })
const leaf = (name: string): Json => ({ $ref: `#/$defs/${name}` })
const isPlain = (v: unknown): v is Json => typeof v === `object` && v !== null && !Array.isArray(v)
/** A value that is a binding or a call, never a literal. */
const DYNAMIC_ONLY = (): Json => ({ oneOf: [common(`DataBinding`), common(`FunctionCall`)] })
const orDynamic = (literal: Json): Json => ({ oneOf: [literal, common(`DataBinding`), common(`FunctionCall`)] })

interface Ctx {
  view: CatalogView
  iconNames: readonly string[]
  where: string
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

/** The literal schema of a prop (no binding, no breakpoints). */
function literalSchema(schema: PropSchema, ctx: Ctx): Json {
  switch (schema.type) {
    case `string`:
    case `markdown`:
      return { type: `string` }
    case `url`:
      return { type: `string`, format: `uri-reference` }
    case `date`:
      return { type: `string`, pattern: `^(\\d{4}-\\d{2}-\\d{2})?$` }
    case `number`:
      return { type: `number`, ...(schema.minimum !== undefined ? { minimum: schema.minimum } : {}), ...(schema.maximum !== undefined ? { maximum: schema.maximum } : {}) }
    case `boolean`:
      return { type: `boolean` }
    case `enum`:
      return { enum: [...(schema.values ?? ctx.view.enums[schema.enum ?? ``] ?? [])] }
    case `icon`:
      return { type: `string`, enum: [...ctx.iconNames] }
    case `color`:
      return { type: `string`, pattern: `^\\$color\\.[a-zA-Z0-9]+$` }
    case `style`:
      return leaf(`Style`)
    case `componentId`:
      return common(`ComponentId`)
    case `dynamic`:
      return DYNAMIC_ONLY()
    case `array`:
      return { type: `array`, items: schema.items ? propSchema(schema.items, ctx, false) : {} }
    case `object`:
      return schema.shape ? leaf(schema.shape) : { type: `object` }
  }
}

const STRINGISH = new Set([`string`, `markdown`, `url`, `date`, `color`])

/** §11 prop schemas: a bindable string / number / boolean is the matching
 *  Dynamic*, a bindable list of strings DynamicStringList, a shape-less
 *  object or array DynamicValue, anything else with a literal constraint
 *  (an enum, an icon, a shaped object, a typed list) the literal or a
 *  binding or a call. The literal's own constraints (format, pattern,
 *  bounds) ride along: they only constrain the literal. */
function dynamicSchema(schema: PropSchema, literal: Json): Json {
  const { type: _type, ...constraints } = literal
  if (STRINGISH.has(schema.type)) return { ...common(`DynamicString`), ...constraints }
  if (schema.type === `number`) return { ...common(`DynamicNumber`), ...constraints }
  if (schema.type === `boolean`) return common(`DynamicBoolean`)
  if (schema.type === `array` && schema.items && STRINGISH.has(schema.items.type) && !schema.items.bindable) return common(`DynamicStringList`)
  if (schema.type === `object` && !schema.shape) return common(`DynamicValue`)
  if (schema.type === `array` && (!schema.items || (schema.items.type === `object` && !schema.items.shape))) return common(`DynamicValue`)
  return orDynamic(literal)
}

function propSchema(schema: PropSchema, ctx: Ctx, withText = true): Json {
  const literal = literalSchema(schema, ctx)
  let out = schema.bindable && schema.type !== `dynamic` ? dynamicSchema(schema, literal) : literal
  if (schema.responsive) {
    // Round 1: `{base, sm?, md?, lg?, xl?}` keyed by the $breakpoint names.
    const responsive = { type: `object`, properties: Object.fromEntries([`base`, ...TOKEN_GROUPS.breakpoint].map((k) => [k, literal])), required: [`base`], additionalProperties: false }
    out = { oneOf: [out, responsive] }
  }
  out = { ...out }
  if (withText && schema.description) out.description = schema.description
  if (schema.default !== undefined) out.default = schema.default
  if (schema.status === `deprecated`) {
    out.deprecated = true
    out[`x-deprecated-reason`] = schema.deprecatedReason ?? schema.description
  }
  return out
}

function shapeSchema(def: DefSchema, ctx: Ctx): Json {
  const properties: Json = {}
  const required: string[] = []
  for (const [prop, schema] of Object.entries(def.properties)) {
    properties[prop] = propSchema(schema, ctx)
    if (schema.required) required.push(prop)
  }
  return { type: `object`, ...(def.description ? { description: def.description } : {}), properties, ...(required.length ? { required } : {}), additionalProperties: false }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

function componentSchema(name: string, def: ComponentDef, ctx: Ctx, defs: Json): Json {
  const where = `${ctx.where} component ${name}`
  const properties: Json = { component: { const: name } }
  const required = [`component`]
  let checkable = false
  for (const [prop, schema] of Object.entries(def.props)) {
    // The Unknown placeholder names the unmapped component in a prop called
    // `component` / `catalogId`: the envelope owns those keys on the wire.
    if (ENVELOPE.has(prop)) continue
    if (prop === `checks`) {
      checkable = true
      continue
    }
    if ((prop === `children` || prop === `on` || prop === `visible`) || (prop === `slots` && def.slots?.length))
      throw new Error(`${where}: prop ${prop} is a declared key (§5.3)`)
    properties[prop] = prop === `style` ? { ...leaf(`Style`), ...(schema.description ? { description: schema.description } : {}) } : propSchema(schema, ctx)
    if (schema.required) required.push(prop)
  }
  if (def.children !== `none`) properties.children = leaf(`ChildList`)
  if (def.slots?.length) {
    const slots: Json = {}
    for (const slot of def.slots) if (slot !== `*`) slots[slot] = common(`ComponentId`)
    // `*` = any slot name (Table's per-column cell templates).
    defs[`Slots_${name}`] = { type: `object`, properties: slots, additionalProperties: def.slots.includes(`*`) ? common(`ComponentId`) : false }
    properties.slots = leaf(`Slots_${name}`)
  }
  if (def.events?.length) {
    defs[`On_${name}`] = { type: `object`, properties: Object.fromEntries(def.events.map((event) => [event, leaf(`EuiAction`)])), additionalProperties: false }
    properties.on = leaf(`On_${name}`)
  }
  if (!(`style` in properties)) properties.style = leaf(`Style`)
  properties.visible = common(`DynamicBoolean`)
  const out: Json = { type: `object` }
  if (def.description) out.description = def.description
  if (checkable) out.allOf = [common(`Checkable`)]
  out.properties = properties
  out.required = required
  if (def.parents) out.allowedParents = [...def.parents]
  if (def.children === `none` && !def.slots?.length) out.allowedChildren = []
  const component: Json = { kind: def.kind, group: def.group, lite: def.lite, children: def.children }
  if (def.slots) component.slots = [...def.slots]
  if (def.slotScope) component.slotScope = def.slotScope
  if (def.events) component.events = [...def.events]
  if (def.hidden !== undefined) component.hidden = def.hidden
  if (def.status !== undefined) component.status = def.status
  out.metadata = { extensions: { at_exponential_component: component } }
  return out
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

interface FunctionSource {
  description?: string
  args: Record<string, string>
  returns: string
}

function functionDef(name: string, description: string | undefined, args: Json, argsRequired: readonly string[], returns: string): Json {
  if (!RETURN_TYPES.includes(returns)) throw new Error(`function ${name}: returns ${returns} is not a 1.0 returnType`)
  return {
    type: `object`,
    ...(description ? { description } : {}),
    properties: {
      "@call": { const: name },
      args: { type: `object`, properties: args, ...(argsRequired.length ? { required: [...argsRequired] } : {}), unevaluatedProperties: false },
    },
    required: argsRequired.length ? [`@call`, `args`] : [`@call`],
    returnType: returns,
    allowedCallers: RENDERER_ONLY.has(name) ? `rendererOnly` : `rendererOrAgent`,
    requiresUserActivation: USER_ACTIVATION.has(name),
  }
}

/** A core function's argument: dynamic by its type (args resolve at any
 *  depth, §3.2); `set`/`bind`'s `path` is a plain string (§3.1). */
function argSchema(fn: string, arg: string, type: string): Json {
  if ((fn === `set` || fn === `bind`) && arg === `path`) return { type: `string` }
  switch (type) {
    case `string`:
      return common(`DynamicString`)
    case `number`:
      return common(`DynamicNumber`)
    case `boolean`:
      return common(`DynamicBoolean`)
    case `array`:
      return orDynamic({ type: `array` })
    case `object`:
      return orDynamic({ type: `object` })
    default:
      return {}
  }
}

const vendored = vendoredBasic as unknown as { functions: Record<string, { description?: string; returnType: string; properties: { args: { properties: Json; required?: string[] } } }> }

/** The catalog function map (§6.1), in `functions.names` order: the 14
 *  basic functions keep upstream 1.0's argument schemas (§5.5: each is the
 *  core function of the same name), the core ones come from the source. */
function functionsSchema(): Json {
  const out: Json = {}
  const basic = coreCatalog.functions.basic as Record<string, FunctionSource>
  const core = coreCatalog.functions.core as Record<string, FunctionSource>
  for (const name of coreCatalog.functions.names) {
    if (name in basic) {
      const upstream = vendored.functions[name]
      if (!upstream) throw new Error(`function ${name}: not in the vendored 1.0 basic catalog`)
      if (upstream.returnType !== basic[name].returns) throw new Error(`function ${name}: returns ${basic[name].returns}, upstream 1.0 says ${upstream.returnType}`)
      out[name] = functionDef(name, upstream.description, upstream.properties.args.properties, upstream.properties.args.required ?? [], upstream.returnType)
    } else if (name in core) {
      const spec = core[name]
      const args = Object.fromEntries(Object.entries(spec.args).map(([arg, type]) => [arg, argSchema(name, arg, type)]))
      out[name] = functionDef(name, spec.description, args, [], spec.returns)
    } else throw new Error(`function ${name}: listed in functions.names, defined nowhere`)
  }
  return out
}

// ---------------------------------------------------------------------------
// Leaves
// ---------------------------------------------------------------------------

function styleSchema(): Json {
  const length = {
    oneOf: [{ type: `number` }, { type: `string`, pattern: `^(-?\\d+(\\.\\d+)?(px|%)|auto|\\$[a-z][a-zA-Z0-9.]*\\.[a-zA-Z0-9]+)$` }],
  }
  const number = { oneOf: [{ type: `number` }, { type: `string`, pattern: TOKEN_PATTERN }] }
  const color = { type: `string`, pattern: `^\\$color\\.[a-zA-Z0-9]+$` }
  const gradient = {
    type: `object`,
    properties: { angle: { type: `number` }, stops: { type: `array`, minItems: 2, items: { type: `object`, properties: { color, offset: { type: `number`, minimum: 0, maximum: 1 } }, required: [`color`, `offset`], additionalProperties: false } } },
    required: [`angle`, `stops`],
    additionalProperties: false,
  }
  const keys: Json = {}
  const groups = { ...styleJson.layout, ...styleJson.visual } as Record<string, { type?: string; enum?: unknown[]; group?: string }>
  for (const [key, spec] of Object.entries(groups)) {
    if (spec.enum) keys[key] = { enum: spec.enum }
    else if (spec.type === `length`) keys[key] = length
    else if (spec.type === `number`) keys[key] = number
    else if (spec.type === `color`) keys[key] = color
    else if (spec.type === `token`) keys[key] = { type: `string`, pattern: `^\\$${spec.group}\\.[a-zA-Z0-9]+$` }
    else if (spec.type === `ratio`) keys[key] = { oneOf: [{ type: `number` }, { type: `string`, pattern: `^\\d+(\\.\\d+)?/\\d+(\\.\\d+)?$` }] }
    else if (spec.type === `areas`) keys[key] = { type: `array`, items: { type: `string` } }
    else if (spec.type === `gradient`) keys[key] = gradient
    else if (spec.type === `transform`) keys[key] = { type: `string`, pattern: styleJson.conditions.transform }
    else keys[key] = { type: `string` }
  }
  const base: Json = { type: `object`, properties: keys, additionalProperties: false }
  return {
    type: `object`,
    description: `The Box style whitelist (VAPP-4, round 1): layout + visual keys, numbers in px, token references $<group>.<name>; conditions nest one level: @media (min-/max-width|height, orientation, hover, prefers-reduced-motion; px or $breakpoint.<name>) in source order, then the states ${styleJson.conditions.states.join(`, `)}.`,
    properties: {
      ...keys,
      ...Object.fromEntries(styleJson.conditions.states.map((state) => [state, base])),
    },
    patternProperties: { [styleJson.conditions.media]: base },
    additionalProperties: false,
  }
}

/** Our fixed leaves: Style, Tokens, ChildList (+ `key`), EuiAction. */
function fixedLeaves(): Json {
  return {
    Style: styleSchema(),
    Tokens: {
      type: `object`,
      description: `The token NAMES a theme must define, by group (values come from the theme).`,
      properties: Object.fromEntries(Object.entries(TOKEN_GROUPS).map(([group, names]) => [group, { type: `array`, items: { enum: [...names] } }])),
    },
    ChildList: {
      description: `Child component ids, or one template component per item of the array at \`path\`; \`key\` (ours) = a pointer relative to each item that identifies it, so reordering keeps the item's component state (the index when absent).`,
      oneOf: [
        { type: `array`, items: common(`ComponentId`) },
        {
          type: `object`,
          properties: { componentId: common(`ComponentId`), path: { type: `string` }, key: { type: `string` } },
          required: [`componentId`, `path`],
          additionalProperties: false,
        },
      ],
    },
    EuiAction: {
      description: `A core action: an agent event, a renderer function, or both (round 1: the function args and the event context are evaluated first, then the function runs, then the event is dispatched). \`functionCall\` may name a catalog function or a host function (\`app_toast\`; the host registers it and its policy gates it).`,
      type: `object`,
      properties: {
        event: {
          type: `object`,
          properties: { name: { type: `string` }, context: { type: `object`, additionalProperties: common(`DynamicValue`) }, userMessage: common(`DynamicString`) },
          required: [`name`],
          additionalProperties: false,
        },
        functionCall: {
          type: `object`,
          properties: { "@call": { type: `string`, minLength: 1 }, args: { type: `object` }, catalogId: { type: `string` } },
          required: [`@call`],
          additionalProperties: false,
        },
      },
      anyOf: [{ required: [`event`] }, { required: [`functionCall`] }],
      additionalProperties: false,
    },
  }
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

interface Build {
  catalogId: string
  title: string
  description: string
  instructions?: string
  extensions?: Json
  components: Record<string, ComponentDef>
  shapes: Record<string, DefSchema>
  ctx: Ctx
}

function checkNames(build: Build): void {
  const where = build.ctx.where
  for (const [name, def] of Object.entries(build.components)) {
    if (!isIdentifier(name)) throw new Error(`${where}: component ${JSON.stringify(name)} is not an identifier (UAX #31)`)
    if (name === `Surface`) throw new Error(`${where}: no catalog may define Surface (§5.4)`)
    for (const prop of Object.keys(def.props)) if (!isIdentifier(prop)) throw new Error(`${where}: ${name}.${prop} is not an identifier (UAX #31)`)
    for (const parent of def.parents ?? []) if (parent !== `Surface` && !(parent in build.components)) throw new Error(`${where}: ${name}.parents names ${parent}, not a component`)
  }
  for (const [name, def] of Object.entries(build.shapes))
    for (const prop of Object.keys(def.properties)) if (!isIdentifier(prop)) throw new Error(`${where}: shape ${name}.${prop} is not an identifier (UAX #31)`)
}

function catalogDefinition(build: Build): Json {
  checkNames(build)
  const defs: Json = {}
  const components: Json = {}
  for (const [name, def] of Object.entries(build.components)) components[name] = componentSchema(name, def, build.ctx, defs)
  const functions = functionsSchema()
  const shapes: Json = {}
  for (const [name, def] of Object.entries(build.shapes)) shapes[name] = shapeSchema(def, build.ctx)
  const fixed = fixedLeaves()
  for (const name of Object.keys(shapes)) if (name in fixed || name in defs) throw new Error(`${build.ctx.where}: shape ${name} collides with a generated leaf`)
  const doc: Json = {
    $schema: SCHEMA_URI,
    $id: build.catalogId,
    catalogId: build.catalogId,
    protocolVersion: PROTOCOL_VERSION,
    title: build.title,
    description: build.description,
    ...(build.instructions ? { instructions: build.instructions } : {}),
    ...(build.extensions ? { metadata: { extensions: build.extensions } } : {}),
    components,
    functions,
    $defs: {
      anyComponent: { oneOf: Object.keys(components).map((name) => ({ $ref: `#/components/${name}` })), discriminator: { propertyName: `component` } },
      anyFunction: { oneOf: Object.keys(functions).map((name) => ({ $ref: `#/functions/${name}` })) },
      ...fixed,
      ...shapes,
      ...defs,
    },
  }
  const issues = checkCatalogDefinition(doc)
  if (issues.length) throw new Error(`${build.ctx.where}: not a conforming 1.0 catalog definition:\n${issues.join(`\n`)}`)
  return doc
}

const describeCore = (count: number, lite: boolean): string =>
  `Exponential UI ${lite ? `core-lite` : `core`} catalog v${coreCatalog.version} (A2UI ${PROTOCOL_VERSION}): ${count} components${lite ? ` (the lite subset of the core)` : ` (natives + macros)`}, the A2UI basic catalog's 14 functions plus ${Object.keys(coreCatalog.functions.core).length} core functions, the Box style whitelist and the token names. Generated by packages/exponential-ui/scripts/generate.ts from catalog/core.catalog.json — do not edit.`

/** The core catalog as an A2UI 1.0 catalog definition (catalog/core.schema.json). */
export function coreSchema(iconNames: readonly string[] = ICON_NAMES): Json {
  const view = catalogView()
  return catalogDefinition({
    catalogId: coreCatalog.id,
    title: coreCatalog.name,
    description: describeCore(Object.keys(coreCatalog.components).length, false),
    instructions: coreCatalog.instructions,
    extensions: { at_exponential_lite_catalog_id: coreCatalog.liteId },
    components: coreCatalog.components,
    shapes: coreCatalog.defs,
    ctx: { view, iconNames, where: `core` },
  })
}

/** The core-lite subset (`lite: true` components, the core's functions;
 *  catalog/core-lite.schema.json). */
export function coreLiteSchema(iconNames: readonly string[] = ICON_NAMES): Json {
  const view = catalogView()
  const components = Object.fromEntries(Object.entries(coreCatalog.components).filter(([, def]) => def.lite))
  return catalogDefinition({
    catalogId: coreCatalog.liteId,
    title: `${coreCatalog.name} (lite)`,
    description: describeCore(Object.keys(components).length, true),
    instructions: coreCatalog.instructions,
    components,
    shapes: coreCatalog.defs,
    ctx: { view, iconNames, where: `core-lite` },
  })
}

/** An extension's 1.0 catalog definition: its own id, the core's
 *  components and functions plus its own (§5.5: an extension = its
 *  components ∪ the core's), self-contained so an agent can read it as an
 *  inline catalog. */
export function extensionSchema(ext: ExtensionDef, iconNames: readonly string[] = ICON_NAMES): Json {
  const view = catalogView([ext])
  for (const name of Object.keys(ext.components)) if (name in coreCatalog.components) throw new Error(`extension ${ext.id}: ${name} shadows a core component`)
  for (const name of Object.keys(ext.defs ?? {})) if (name in coreCatalog.defs) throw new Error(`extension ${ext.id}: shape ${name} shadows a core shape`)
  return catalogDefinition({
    catalogId: ext.id,
    title: ext.name,
    description: `${ext.name}: an Exponential UI extension catalog over ${ext.extends} (${Object.keys(ext.components).length} own components after the core's).`,
    instructions: ext.instructions,
    components: { ...coreCatalog.components, ...ext.components },
    shapes: { ...coreCatalog.defs, ...(ext.defs ?? {}) },
    ctx: { view, iconNames, where: `extension ${ext.id}` },
  })
}

// ---------------------------------------------------------------------------
// The structural checker (§11 Lint; no JSON Schema engine)
// ---------------------------------------------------------------------------

export interface CheckOptions {
  /** Default true: component props sit at the definition's top level (ours).
   *  false = upstream's style, props inside an `allOf` member (the vendored
   *  1.0 basic catalog). */
  flat?: boolean
}

const COMMON_REF = new RegExp(`^common_types\\.json#/\\$defs/(${COMMON_TYPE_REFS.join(`|`)})$`)

/** Every structural rule of §11 a 1.0 catalog definition must hold, as
 *  issues (`<pointer>: <message>`); empty = conforming. */
export function checkCatalogDefinition(doc: unknown, options: CheckOptions = {}): string[] {
  const flat = options.flat ?? true
  const issues: string[] = []
  const add = (at: string, message: string) => issues.push(`${at || `/`}: ${message}`)
  if (!isPlain(doc)) return [`/: a catalog definition is an object`]
  for (const key of Object.keys(doc)) if (!CATALOG_ROOT_KEYS.includes(key)) add(``, `root key ${key} is not allowed`)
  if (typeof doc.catalogId !== `string` || doc.catalogId === ``) add(`/catalogId`, `required, a non-empty string`)
  if (doc.$id !== undefined && doc.$id !== doc.catalogId) add(`/$id`, `must equal catalogId`)
  if (doc.protocolVersion !== PROTOCOL_VERSION) add(`/protocolVersion`, `must be "${PROTOCOL_VERSION}"`)
  for (const key of [`$schema`, `title`, `description`, `instructions`]) if (doc[key] !== undefined && typeof doc[key] !== `string`) add(`/${key}`, `a string`)
  if (doc.metadata !== undefined) checkMetadata(doc.metadata, `/metadata`, add)

  const components = isPlain(doc.components) ? doc.components : {}
  if (doc.components !== undefined && !isPlain(doc.components)) add(`/components`, `an object`)
  const functions = isPlain(doc.functions) ? doc.functions : {}
  if (doc.functions !== undefined && !isPlain(doc.functions)) add(`/functions`, `an object`)
  const defs = isPlain(doc.$defs) ? doc.$defs : {}
  if (!isPlain(doc.$defs)) add(`/$defs`, `an object with anyComponent and anyFunction`)

  for (const [name, def] of Object.entries(components)) checkComponent(name, def, components, flat, add)
  for (const [name, def] of Object.entries(functions)) checkFunction(name, def, add)

  for (const [key, list, kind] of [[`anyComponent`, components, `components`], [`anyFunction`, functions, `functions`]] as const) {
    const any = defs[key]
    if (!isPlain(any)) {
      add(`/$defs/${key}`, `required`)
      continue
    }
    const refs = Array.isArray(any.oneOf) ? any.oneOf.map((r) => (isPlain(r) ? r.$ref : undefined)) : []
    const expected = Object.keys(list).map((n) => `#/${kind}/${n}`)
    if (JSON.stringify(refs) !== JSON.stringify(expected)) add(`/$defs/${key}`, `oneOf must reference every entry of ${kind}, in order`)
  }
  for (const [name, def] of Object.entries(defs)) {
    if (name === `anyComponent` || name === `anyFunction`) continue
    if (definesComponent(def)) add(`/$defs/${name}`, `a leaf must not define a component (properties.component.const)`)
  }

  walkRefs(doc, ``, (ref, at) => {
    if (COMMON_REF.test(ref)) return
    const local = /^#\/(components|functions|\$defs)\/([^/]+)$/.exec(ref)
    if (!local) return add(at, `$ref ${ref} is not an allowed target`)
    const table = local[1] === `components` ? components : local[1] === `functions` ? functions : defs
    if (!(local[2] in table)) add(at, `$ref ${ref} names nothing`)
  })
  return issues
}

function checkMetadata(value: unknown, at: string, add: (at: string, message: string) => void): void {
  if (!isPlain(value)) return add(at, `an object`)
  for (const key of Object.keys(value)) if (key !== `extensions`) add(at, `key ${key} is not allowed (only extensions)`)
  if (value.extensions === undefined) return
  if (!isPlain(value.extensions)) return add(`${at}/extensions`, `an object`)
  for (const key of Object.keys(value.extensions)) if (!isIdentifier(key)) add(`${at}/extensions`, `key ${JSON.stringify(key)} is not an identifier (UAX #31)`)
}

function definesComponent(schema: unknown): boolean {
  if (!isPlain(schema)) return false
  const has = (s: unknown) => isPlain(s) && isPlain(s.properties) && isPlain(s.properties.component) && `const` in s.properties.component
  return has(schema) || (Array.isArray(schema.allOf) && schema.allOf.some(has))
}

function checkComponent(name: string, def: unknown, components: Json, flat: boolean, add: (at: string, message: string) => void): void {
  const at = `/components/${name}`
  if (!isIdentifier(name)) add(at, `component name is not an identifier (UAX #31)`)
  if (name === `Surface`) add(at, `Surface is reserved (§5.4)`)
  if (!isPlain(def)) return add(at, `a component definition is an object`)
  if (def.type !== `object`) add(`${at}/type`, `must be "object"`)
  if (`oneOf` in def || `anyOf` in def) add(at, `no root oneOf/anyOf (flat)`)
  let props: Json | undefined = isPlain(def.properties) ? def.properties : undefined
  let required: unknown = def.required
  if (def.allOf !== undefined) {
    const list = Array.isArray(def.allOf) ? def.allOf : []
    for (const [i, part] of list.entries()) {
      if (isPlain(part) && part.$ref === `common_types.json#/$defs/Checkable` && Object.keys(part).length === 1) continue
      if (!flat && isPlain(part) && isPlain(part.properties) && !props) {
        props = part.properties
        required = part.required
        continue
      }
      add(`${at}/allOf/${i}`, `the only allOf member allowed is common_types.json#/$defs/Checkable (flat)`)
    }
  }
  if (!props) return add(`${at}/properties`, `required (flat: the props sit on the definition)`)
  if (!isPlain(props.component) || props.component.const !== name) add(`${at}/properties/component`, `must be {"const": "${name}"}`)
  if (!Array.isArray(required) || !required.includes(`component`)) add(`${at}/required`, `must list component`)
  for (const prop of Object.keys(props)) if (!isIdentifier(prop)) add(`${at}/properties`, `prop ${JSON.stringify(prop)} is not an identifier (UAX #31)`)
  for (const key of [`allowedParents`, `allowedChildren`] as const) {
    const list = def[key]
    if (list === undefined) continue
    if (!Array.isArray(list) || list.some((v) => typeof v !== `string`) || new Set(list).size !== list.length) {
      add(`${at}/${key}`, `an array of unique names`)
      continue
    }
    for (const other of list as string[]) if (!(other in components) && !(key === `allowedParents` && other === `Surface`)) add(`${at}/${key}`, `${other} is not a component of this catalog`)
  }
  if (def.metadata !== undefined) checkMetadata(def.metadata, `${at}/metadata`, add)
}

function checkFunction(name: string, def: unknown, add: (at: string, message: string) => void): void {
  const at = `/functions/${name}`
  if (!isIdentifier(name)) add(at, `function name is not an identifier (UAX #31)`)
  if (RESERVED_FUNCTION_PREFIXES.some((p) => name.startsWith(p))) add(at, `function name has a reserved host prefix (§6.3)`)
  if (name.includes(`__`)) add(at, `function name contains __ (§6.3)`)
  if (!isPlain(def)) return add(at, `a function definition is an object`)
  for (const key of Object.keys(def)) if (!FUNCTION_KEYS.includes(key)) add(at, `key ${key} is not allowed in a function definition`)
  if (def.type !== `object`) add(`${at}/type`, `must be "object"`)
  if (!isPlain(def.properties)) add(`${at}/properties`, `required`)
  else {
    for (const key of Object.keys(def.properties)) if (key !== `@call` && key !== `args`) add(`${at}/properties`, `key ${key} is not allowed (only @call, args)`)
    const call = def.properties[`@call`]
    if (!isPlain(call) || call.const !== name) add(`${at}/properties/@call`, `must be {"const": "${name}"}`)
    const args = def.properties.args
    if (args !== undefined) {
      if (!isPlain(args) || args.type !== `object`) add(`${at}/properties/args`, `an object schema`)
      else if (isPlain(args.properties)) for (const arg of Object.keys(args.properties)) if (!isIdentifier(arg)) add(`${at}/properties/args`, `arg ${JSON.stringify(arg)} is not an identifier (UAX #31)`)
    }
  }
  if (!Array.isArray(def.required) || !def.required.includes(`@call`)) add(`${at}/required`, `must list @call`)
  if (!RETURN_TYPES.includes(def.returnType as string)) add(`${at}/returnType`, `one of ${RETURN_TYPES.join(`|`)}`)
  if (def.allowedCallers !== undefined && !CALLERS.includes(def.allowedCallers as string)) add(`${at}/allowedCallers`, `one of ${CALLERS.join(`|`)}`)
  if (def.requiresUserActivation !== undefined && typeof def.requiresUserActivation !== `boolean`) add(`${at}/requiresUserActivation`, `a boolean`)
  if (def.requiresUserActivation === true && def.allowedCallers !== undefined && def.allowedCallers !== `rendererOnly`) add(`${at}/allowedCallers`, `a function that requires user activation is rendererOnly`)
}

function walkRefs(value: unknown, at: string, visit: (ref: string, at: string) => void): void {
  if (Array.isArray(value)) return value.forEach((v, i) => walkRefs(v, `${at}/${i}`, visit))
  if (!isPlain(value)) return
  for (const [key, v] of Object.entries(value)) {
    const here = `${at}/${key.replace(/~/g, `~0`).replace(/\//g, `~1`)}`
    if (key === `$ref` && typeof v === `string`) visit(v, here)
    else walkRefs(v, here, visit)
  }
}
