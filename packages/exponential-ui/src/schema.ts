// VAPP-85: the catalog as JSON Schema, in A2UI's catalog shape (components +
// functions + $defs.anyComponent) so an A2UI client can validate a core
// surface, and the same emitter for an extension's schema (`extends` the
// core). Generated into catalog/core.schema.json by scripts/generate.ts.

import styleJson from "../catalog/style.json" with { type: "json" }
import vendoredBasic from "../vendor/a2ui/v0_9/basic_catalog.json" with { type: "json" }
import { catalogView, coreCatalog, TOKEN_GROUPS } from "./catalog"
import type { CatalogView } from "./catalog"
import type { ComponentDef, ExtensionDef, PropSchema } from "./types"

type Json = Record<string, unknown>

const TOKEN_PATTERN = `^\\$[a-z][a-zA-Z0-9.]*\\.[a-zA-Z0-9]+$`
const DYNAMIC = { oneOf: [{ $ref: `#/$defs/DataBinding` }, { $ref: `#/$defs/FunctionCall` }] }

function propToSchema(schema: PropSchema, view: CatalogView, iconNames: readonly string[]): Json {
  let base: Json
  switch (schema.type) {
    case `string`:
    case `markdown`:
      base = { type: `string` }
      break
    case `url`:
      base = { type: `string`, format: `uri-reference` }
      break
    case `date`:
      base = { type: `string`, pattern: `^(\\d{4}-\\d{2}-\\d{2})?$` }
      break
    case `number`:
      base = { type: `number`, ...(schema.minimum !== undefined ? { minimum: schema.minimum } : {}), ...(schema.maximum !== undefined ? { maximum: schema.maximum } : {}) }
      break
    case `boolean`:
      base = { type: `boolean` }
      break
    case `enum`:
      base = { enum: [...(schema.values ?? view.enums[schema.enum ?? ``] ?? [])] }
      break
    case `icon`:
      base = { type: `string`, enum: [...iconNames] }
      break
    case `color`:
      base = { type: `string`, pattern: `^\\$color\\.[a-zA-Z0-9]+$` }
      break
    case `style`:
      base = { $ref: `#/$defs/Style` }
      break
    case `array`:
      base = { type: `array`, items: schema.items ? propToSchema(schema.items, view, iconNames) : {} }
      break
    case `object`:
      base = schema.shape ? { $ref: `#/$defs/${schema.shape}` } : { type: `object` }
      break
  }
  const alternatives: Json[] = [base]
  if (schema.responsive) {
    // Round 1: `{base, sm?, md?, lg?, xl?}` keyed by the $breakpoint names.
    const value = { ...base }
    alternatives.push({ type: `object`, properties: Object.fromEntries([`base`, ...TOKEN_GROUPS.breakpoint].map((k) => [k, value])), required: [`base`], additionalProperties: false })
  }
  if (schema.bindable) alternatives.push(...DYNAMIC.oneOf)
  const out: Json = alternatives.length === 1 ? base : { oneOf: alternatives }
  if (schema.description) out.description = schema.description
  if (schema.default !== undefined) out.default = schema.default
  return out
}

function componentSchema(name: string, def: ComponentDef, view: CatalogView, iconNames: readonly string[]): Json {
  const properties: Json = { component: { const: name } }
  const required = [`component`]
  for (const [prop, schema] of Object.entries(def.props)) {
    properties[prop] = propToSchema(schema, view, iconNames)
    if (schema.required) required.push(prop)
  }
  if (def.children !== `none`) properties.children = { $ref: `#/$defs/ChildList` }
  if (def.slots?.length) {
    const slots: Json = {}
    for (const slot of def.slots) if (slot !== `*`) slots[slot] = { $ref: `#/$defs/ComponentId` }
    // `*` = any slot name (Table's per-column cell templates).
    properties.slots = { type: `object`, properties: slots, additionalProperties: def.slots.includes(`*`) ? { $ref: `#/$defs/ComponentId` } : false }
  }
  if (def.events?.length) {
    const on: Json = {}
    for (const event of def.events) on[event] = { $ref: `#/$defs/Action` }
    properties.on = { type: `object`, properties: on, additionalProperties: false }
  }
  properties.style = { $ref: `#/$defs/Style` }
  properties.visible = { $ref: `#/$defs/Visible` }
  properties.accessibility = { $ref: `#/$defs/AccessibilityAttributes` }
  return {
    type: `object`,
    description: def.description,
    allOf: [
      { $ref: `#/$defs/ComponentCommon` },
      { type: `object`, properties, required },
    ],
    unevaluatedProperties: false,
  }
}

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

function defsSchema(view: CatalogView, iconNames: readonly string[]): Json {
  const defs: Json = {
    ComponentId: { type: `string`, description: `The unique identifier for a component within a surface.` },
    ComponentCommon: {
      type: `object`,
      properties: { id: { $ref: `#/$defs/ComponentId` }, accessibility: { $ref: `#/$defs/AccessibilityAttributes` } },
      required: [`id`],
    },
    AccessibilityAttributes: {
      type: `object`,
      description: `A2UI AccessibilityAttributes: label and description are DynamicStrings (a literal, a binding or a call, resolved at bind time).`,
      properties: {
        label: { oneOf: [{ type: `string` }, { $ref: `#/$defs/DataBinding` }, { $ref: `#/$defs/FunctionCall` }] },
        description: { oneOf: [{ type: `string` }, { $ref: `#/$defs/DataBinding` }, { $ref: `#/$defs/FunctionCall` }] },
      },
      additionalProperties: false,
    },
    ChildList: {
      oneOf: [
        { type: `array`, items: { $ref: `#/$defs/ComponentId` } },
        {
          type: `object`,
          properties: {
            componentId: { $ref: `#/$defs/ComponentId` },
            path: { type: `string` },
            key: { type: `string`, description: `A pointer relative to each item that identifies it, so reordering keeps the item's component state (the index when absent).` },
          },
          required: [`componentId`, `path`],
          additionalProperties: false,
        },
      ],
    },
    Visible: {
      description: `Round 1: false (or a binding/call resolving falsy) = the node is not rendered and takes no space; absent = visible.`,
      oneOf: [{ type: `boolean` }, { $ref: `#/$defs/DataBinding` }, { $ref: `#/$defs/FunctionCall` }],
    },
    DataBinding: { type: `object`, properties: { path: { type: `string` } }, required: [`path`], additionalProperties: false },
    FunctionCall: {
      type: `object`,
      properties: { call: { type: `string`, enum: [...coreCatalog.functions.names] }, args: { type: `object` }, returnType: { type: `string` } },
      required: [`call`],
    },
    ActionFunctionCall: {
      description: `A function an action runs: a catalog function (built in) or a host function (namespaced, \`app.toast\`; the host registers it and its policy gates it, catalog/host.json).`,
      type: `object`,
      properties: { call: { type: `string`, minLength: 1, examples: [...coreCatalog.functions.names] }, args: { type: `object` }, returnType: { type: `string` } },
      required: [`call`],
    },
    Action: {
      description: `A server event, a client function, or (round 1) both: the function args and the event context are evaluated first, then the function runs, then the event is dispatched. \`functionCall\` = A2UI's key.`,
      type: `object`,
      properties: {
        event: { type: `object`, properties: { name: { type: `string` }, context: { type: `object` } }, required: [`name`] },
        functionCall: { $ref: `#/$defs/ActionFunctionCall` },
      },
      anyOf: [{ required: [`event`] }, { required: [`functionCall`] }],
      additionalProperties: false,
    },
    Style: styleSchema(),
    Tokens: {
      type: `object`,
      description: `The token NAMES a theme must define, by group (values come from the theme).`,
      properties: Object.fromEntries(Object.entries(TOKEN_GROUPS).map(([group, names]) => [group, { type: `array`, items: { enum: [...names] } }])),
    },
  }
  for (const [name, def] of Object.entries(view.defs)) {
    const properties: Json = {}
    const required: string[] = []
    for (const [prop, schema] of Object.entries(def.properties)) {
      properties[prop] = propToSchema(schema, view, iconNames)
      if (schema.required) required.push(prop)
    }
    defs[name] = { type: `object`, description: def.description, properties, required, additionalProperties: false }
  }
  return defs
}

interface CoreFunctionSpec {
  description: string
  args: Record<string, string>
  returns: string
}
const CORE_FUNCTION_SPECS: Record<string, CoreFunctionSpec> = coreCatalog.functions.core

/** The round-1 core functions in the basic catalog's function-schema shape. */
function coreFunctionSchemas(): Json {
  const argSchema = (type: string): Json =>
    type === `any` ? {} : type === `array` ? { type: `array` } : type === `object` ? { type: `object` } : { oneOf: [{ type }, { $ref: `#/$defs/DataBinding` }, { $ref: `#/$defs/FunctionCall` }] }
  const out: Json = {}
  for (const [name, spec] of Object.entries(CORE_FUNCTION_SPECS)) {
    out[name] = {
      type: `object`,
      description: spec.description,
      properties: {
        call: { const: name },
        args: { type: `object`, properties: Object.fromEntries(Object.entries(spec.args).map(([arg, type]) => [arg, argSchema(type)])), additionalProperties: false },
        returnType: { const: spec.returns },
      },
      required: [`call`, `args`],
      unevaluatedProperties: false,
    }
  }
  return out
}

/** The core catalog's JSON Schema. */
export function coreSchema(iconNames: readonly string[]): Json {
  const view = catalogView()
  const components: Json = {}
  for (const [name, def] of Object.entries(coreCatalog.components)) components[name] = componentSchema(name, def, view, iconNames)
  const basic = vendoredBasic as unknown as { functions: Json }
  return {
    $schema: `https://json-schema.org/draft/2020-12/schema`,
    $id: coreCatalog.id,
    title: coreCatalog.name,
    description: `Exponential UI core catalog v${coreCatalog.version}: ${Object.keys(coreCatalog.components).length} components (natives + macros), the A2UI basic catalog's 14 functions plus ${Object.keys(CORE_FUNCTION_SPECS).length} core functions, the Box style whitelist and the token names. Generated by packages/exponential-ui/scripts/generate.ts from catalog/core.catalog.json — do not edit.`,
    catalogId: coreCatalog.id,
    liteCatalogId: coreCatalog.liteId,
    components,
    functions: { ...basic.functions, ...coreFunctionSchemas() },
    $defs: {
      ...defsSchema(view, iconNames),
      anyComponent: { oneOf: Object.keys(coreCatalog.components).map((name) => ({ $ref: `#/components/${name}` })) },
      anyFunction: { oneOf: [...Object.keys(basic.functions), ...Object.keys(CORE_FUNCTION_SPECS)].map((name) => ({ $ref: `#/functions/${name}` })) },
    },
  }
}

/** An extension's JSON Schema: its own id, `extends` the core, its components. */
export function extensionSchema(ext: ExtensionDef, iconNames: readonly string[]): Json {
  const view = catalogView([ext])
  const components: Json = {}
  for (const [name, def] of Object.entries(ext.components)) components[name] = componentSchema(name, def, view, iconNames)
  const defs: Json = {}
  for (const [name, def] of Object.entries(ext.defs ?? {})) {
    const properties: Json = {}
    const required: string[] = []
    for (const [prop, schema] of Object.entries(def.properties)) {
      properties[prop] = propToSchema(schema, view, iconNames)
      if (schema.required) required.push(prop)
    }
    defs[name] = { type: `object`, description: def.description, properties, required, additionalProperties: false }
  }
  return {
    $schema: `https://json-schema.org/draft/2020-12/schema`,
    $id: ext.id,
    title: ext.name,
    catalogId: ext.id,
    extends: ext.extends,
    components,
    $defs: {
      ...defs,
      anyComponent: {
        oneOf: [
          { $ref: `${ext.extends}#/$defs/anyComponent` },
          ...Object.keys(ext.components).map((name) => ({ $ref: `#/components/${name}` })),
        ],
      },
    },
  }
}
