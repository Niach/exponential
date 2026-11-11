#!/usr/bin/env bun
// VAPP-85: emits everything derived from catalog/*.json. Run from the repo
// root with `bun run --filter @exponential_app/eui generate`; generate.test.ts
// re-renders in memory and fails on any committed drift.
//
// Outputs (all committed):
//   catalog/core.schema.json              the catalog as an A2UI 1.0 catalog definition
//   catalog/core-lite.schema.json         the core-lite subset, the same shape
//   src/catalog.generated.ts              names, enums, token names, icon names, basic map
//   generated/ExponentialUICatalog.generated.swift   the same for the SwiftUI painter (VAPP-88)
//   generated/ExponentialUICatalog.generated.kt      the same for the Compose painter (VAPP-89)
//   generated/catalog.generated.rs                   the same for the Rust core (VAPP-86)
//   docs/components.generated.json        one doc page per component, as data (VAPP-93 renders it)
//   fixtures/catalog-components.json      every component × every enum/boolean value + state cases
//   fixtures/catalog-macros.json          every macro case → the expanded tree
//   fixtures/catalog-basic-map.json       the hand-written basic cases → expected trees
//   fixtures/catalog-extension.json       the example extension's cases → expected trees
//   fixtures/kitchen-sink.expanded.json   fixtures/kitchen-sink.json, reduced + expanded
//   fixtures/prompt-budget.json           the prompt's size on record
//   fixtures/bind-time.json               round 1: bound macro cases × data → the BOUND tree + presses
//   fixtures/style-conditions.json        round 1: style × surface/state context → the flattened style
//   fixtures/code-tokens.json             round 1: source × language → CodeBlock tokens per line
//   fixtures/text-shimmer.json            EXP-1324: text colour × mode → the Text shimmer colours
//   fixtures/voice-wave.json              NEW-77: size × activity × level × bands × time → the VoiceWave rings (VAPP-162: + the bars)
//   fixtures/qr-code.json                 VAPP-162: value × level → the QrCode matrix
//   fixtures/zoom.json                    EXP-1586: picture × viewport × input → the ZoomableImage view
//   fixtures/specimens.json               one surface per component + the demo (VAPP-93: the site's shots)
//   + the theme outputs of scripts/generate-themes.ts (VAPP-92)
//   + the round-4 fixture of scripts/generate-round4.ts (round4-contract.json)
//   + the round-2 fixtures of scripts/generate-round2.ts (format, template items,
//     text direction, Resizable, virtual lists, animations, the list bench)
//
// The icon name list is read from packages/icons/icons.json at GENERATE time
// (the registry is the Icon prop's vocabulary); the package has no runtime
// dependency on it.

import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, TOKEN_GROUPS, coreCatalog } from "../src/catalog"
import { splitDescriptions } from "../src/catalog.described"
import { basicMap } from "../src/basic-map"
import { mangle } from "../src/ids"
import { catalogPrompt, componentPrompt, estimateTokens } from "../src/prompt"
import { reduceNested, reduceSurface } from "../src/reducer"
import { coreLiteSchema, coreSchema } from "../src/schema"
import { STYLE_KEYS, STYLE_STATES, resolveConditions, type ConditionContext } from "../src/style"
import { BIND_FUNCTION_NAMES, bindRowSlot, bindTree, hasRowSlots, isDataSchema, runAction } from "../src/dynamic"
import { tokenizeCode, CODE_LANGUAGE_NAMES } from "../src/code"
import { textShimmer } from "../src/text-shimmer"
import { voiceWave, voiceWaveBars, type VoiceWaveBarsInput, type VoiceWaveInput } from "../src/voice-wave"
import * as zoom from "../src/zoom"
import { qrMatrix } from "../src/qr-code"
import { A11Y_ROLES, COMPONENT_A11Y } from "../src/a11y"
import { STRING_IDS, DEFAULT_STRINGS } from "../src/strings"
import { renderThemes } from "./generate-themes"
import { renderA2ui10 } from "./generate-a2ui10"
import { renderHost } from "./generate-host"
import { renderXid } from "./generate-xid"
import { renderRound2 } from "./generate-round2"
import { renderRound4 } from "./generate-round4"
import { ANIMATION_NAMES } from "../src/animation"
import { FORMAT_FUNCTION_NAMES } from "../src/format"
import { LAYOUT_CONSTANTS } from "../src/layout"
import { LIMITS } from "../src/limits"
import { treeGuides } from "../src/tree-guides"
import { treeKey, treeRows, type TreeKeyRow } from "../src/tree"
import { conformanceManifest } from "../src/conformance"
import styleJson from "../catalog/style.json" with { type: "json" }
import localeJson from "../catalog/locale.json" with { type: "json" }
import type { CatalogSource, ComponentDef, ExtensionDef, FlatComponent, NestedNode, PropSchema, UiNode } from "../src/types"

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), `..`)
const HOST_JSON = JSON.parse(readFileSync(join(pkgRoot, `catalog/host.json`), `utf8`)) as { a2uiVersion: string; transition: { acceptV09: boolean } }
const repoRoot = join(pkgRoot, `..`, `..`)

// VAPP-106: the generator works on the SOURCE catalog (descriptions, examples,
// comments and all), not on the runtime JSON it generates: the source is
// synced INTO the shared `coreCatalog` object in place, nested objects kept
// by identity (a module that captured `coreCatalog.functions.core` at import
// sees the text too), so every module the generator calls (the view, the
// reducer, the prompt, the schema) works on the source.
const sourceCatalog = JSON.parse(readFileSync(join(pkgRoot, `catalog/core.catalog.json`), `utf8`)) as CatalogSource
const isPlainObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
function syncInPlace(target: Record<string, unknown>, source: Record<string, unknown>): void {
  for (const key of Object.keys(target)) if (!(key in source)) delete target[key]
  for (const [key, value] of Object.entries(source)) {
    const current = target[key]
    if (isPlainObject(current) && isPlainObject(value)) syncInPlace(current, value)
    else target[key] = structuredClone(value)
  }
  // VAPP-126: a key the source added lands at the END of the target; re-insert
  // in the source's order (same values, nested identity kept) so ONE run emits
  // what the next run would (a new component sits where the source puts it).
  const order = Object.keys(source)
  if (Object.keys(target).some((k, i) => k !== order[i])) {
    const values = order.map((k) => target[k])
    for (const key of order) delete target[key]
    order.forEach((k, i) => (target[k] = values[i]))
  }
}
syncInPlace(coreCatalog as unknown as Record<string, unknown>, sourceCatalog as unknown as Record<string, unknown>)
/** The runtime catalog + its text, split from the source (src/catalog.described.ts). */
const catalogSplit = splitDescriptions(sourceCatalog)

const HEADER = `AUTO-GENERATED by packages/exponential-ui/scripts/generate.ts — do not edit.
Source of truth: packages/exponential-ui/catalog/core.catalog.json (+ tokens.json, style.json, basic-map.json, strings.json, locale.json, code.json, a11y.json, layout.json).`

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

interface IconRegistry {
  pickable: string[]
  devicePickable: string[]
  semantic: Record<string, string>
  custom?: Record<string, unknown>
  imported?: Record<string, { fill?: string }>
}

/** Every name the Icon prop accepts: the concept ids plus every registry
 *  glyph name (pickable, device, the Lucide targets, custom, imported). */
export function iconNames(): string[] {
  const registry = JSON.parse(readFileSync(join(repoRoot, `packages/icons/icons.json`), `utf8`)) as IconRegistry
  const names = new Set<string>()
  for (const name of Object.keys(registry.semantic)) names.add(name)
  for (const name of Object.values(registry.semantic)) names.add(name)
  for (const name of registry.pickable) names.add(name)
  for (const name of registry.devicePickable) names.add(name)
  for (const name of Object.keys(registry.custom ?? {})) names.add(name)
  for (const name of Object.keys(registry.imported ?? {})) names.add(name)
  return [...names].sort()
}

/** #EXP-1764: the icons with their OWN colour (icons.json `imported[].fill`,
 *  `brand-claude` = Anthropic's orange): every renderer paints them in it,
 *  never in the text colour. Sorted by name. */
export function iconFills(): [string, string][] {
  const registry = JSON.parse(readFileSync(join(repoRoot, `packages/icons/icons.json`), `utf8`)) as IconRegistry
  return Object.entries(registry.imported ?? {})
    .flatMap(([name, entry]) => (entry.fill ? [[name, entry.fill] as [string, string]] : []))
    .sort(([a], [b]) => a.localeCompare(b))
}

const json = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`

/** VAPP-160: `value` without any `$comment` key, at every depth. */
/** catalog/host.json reduced to what src/host/contract.ts and exposes.ts read at runtime. */
function hostRuntime() {
  const host = readJson<any>(`catalog/host.json`)
  return {
    version: host.version,
    a2uiVersion: host.a2uiVersion,
    messages: { a2ui: host.messages.a2ui, host: host.messages.host },
    clientMessages: { kinds: host.clientMessages.kinds, errorCodes: host.clientMessages.errorCodes },
    urls: { defaultSchemes: host.urls.defaultSchemes },
    media: { defaultSchemes: host.media.defaultSchemes, limits: host.media.limits },
    paint: { hook: host.paint.hook },
    transport: { mcpMimeTypes: host.transport.mcpMimeTypes, mcpActionTool: host.transport.mcpActionTool, sseEvents: host.transport.sseEvents },
    surface: { settings: host.surface.settings, formatter: host.surface.formatter, commands: host.surface.commands },
    markdown: { hooks: host.markdown.hooks },
    package: { exposes: host.package.exposes },
  }
}

function stripComments(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stripComments)
  if (value && typeof value === `object`) return Object.fromEntries(Object.entries(value).filter(([k]) => k !== `$comment`).map(([k, v]) => [k, stripComments(v)]))
  return value
}
const kebab = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, `$1-$2`).toLowerCase()
/** VAPP-289: a generated component id (round 9 §4: UAX #31, so `_`, never `-`). */
const snake = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, `$1_$2`).toLowerCase()
const screaming = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, `$1_$2`).replace(/[^A-Za-z0-9]+/g, `_`).toUpperCase()
const capFirst = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

/** Round 1: the icon each renderer-owned glyph uses (`<Component>.<part>` →
 *  an icons.json name), so a CodeBlock's copy button or a FileUpload's drop
 *  zone shows the same glyph on every platform. */
function builtinIcons(): Record<string, string> {
  const table = coreCatalog.builtinIcons
  return Object.fromEntries(Object.entries(table).filter(([k]) => !k.startsWith(`$`)))
}

const components = Object.entries(coreCatalog.components)
/** What a model is offered: everything but the hidden placeholder. */
const visible = components.filter(([, def]) => !def.hidden)
const names = components.map(([name]) => name)
const natives = components.filter(([, d]) => d.kind === `native`).map(([n]) => n)
const macros = components.filter(([, d]) => d.kind === `macro`).map(([n]) => n)
const lite = visible.filter(([, d]) => d.lite).map(([n]) => n)
const specimenId = (name: string) => `exponential-ui-${kebab(name)}`
const basicTargets = Object.entries(basicMap.components).map(([basic, rule]) => [basic, rule.to] as const)
const basicIcons = Object.entries(basicMap.icons).map(([basic, target]) => [basic, target ?? basicMap.placeholderIcon] as const)

// ---------------------------------------------------------------------------
// TypeScript
// ---------------------------------------------------------------------------

/** VAPP-108: a prop schema → the TypeScript type `defineSurface` offers. */
function tsPropType(schema: PropSchema): string {
  let base: string
  switch (schema.type) {
    case `enum`:
      base = schema.values ? schema.values.map((v) => JSON.stringify(v)).join(` | `) : `${capFirst(schema.enum ?? ``)}Value`
      break
    case `number`:
      base = `number`
      break
    case `boolean`:
      base = `boolean`
      break
    case `icon`:
      base = `IconName`
      break
    case `style`:
      base = `Style`
      break
    case `dynamic`:
      base = `Dynamic`
      break
    case `array`:
      base = schema.items ? `${schema.items.type === `object` ? (schema.items.shape ? `Shapes[${JSON.stringify(schema.items.shape)}]` : `Record<string, unknown>`) : tsPropType(schema.items)}[]` : `unknown[]`
      break
    case `object`:
      base = schema.shape ? `Shapes[${JSON.stringify(schema.shape)}]` : `Record<string, unknown>`
      break
    default:
      base = `string`
  }
  if (schema.responsive) base = `${base} | Responsive<${base}>`
  if (schema.bindable) base = `${base} | Dynamic`
  return base
}

function renderTs(icons: string[]): string {
  const shapeTypes = Object.entries(coreCatalog.defs)
    .map(([name, def]) => `  ${name}: { ${Object.entries(def.properties).map(([p, s]) => `${p}${s.required ? `` : `?`}: ${tsPropType(s)}`).join(`; `)} }`)
    .join(`\n`)
  const propTypes = components
    .map(([name, def]) => `  ${name}: { ${Object.entries(def.props).map(([p, s]) => `${p}${s.required ? `` : `?`}: ${tsPropType(s)}`).join(`; `)} }`)
    .join(`\n`)
  const enumConsts = Object.entries(coreCatalog.enums)
    .map(([name, values]) => `  ${name}: ${JSON.stringify(values)},`)
    .join(`\n`)
  const enumTypes = Object.entries(coreCatalog.enums)
    .map(([name]) => `export type ${capFirst(name)}Value = (typeof ENUMS)[${JSON.stringify(name)}][number]`)
    .join(`\n`)
  const tokenConsts = Object.entries(TOKEN_GROUPS)
    .map(([group, values]) => `  ${JSON.stringify(group)}: ${JSON.stringify(values)},`)
    .join(`\n`)
  return `// ${HEADER.split(`\n`).join(`\n// `)}

import type { Style } from "./style"
import type { DataBinding, FunctionCall } from "./types"

export const CATALOG_ID = ${JSON.stringify(CORE_CATALOG_ID)}
export const LITE_CATALOG_ID = ${JSON.stringify(CORE_LITE_CATALOG_ID)}
export const BASIC_CATALOG_ID = ${JSON.stringify(A2UI_BASIC_CATALOG_ID)}
/** VAPP-289: the A2UI wire version every core entry accepts and emits (catalog/host.json \`a2uiVersion\`). */
export const A2UI_VERSION = ${JSON.stringify(HOST_JSON.a2uiVersion)}
/** VAPP-289 (round 9 §14): while true, every core entry upgrades 0.9 input through upgrade09 (catalog/host.json \`transition.acceptV09\`). */
export const ACCEPT_V09: boolean = ${JSON.stringify(HOST_JSON.transition.acceptV09)}
export const CATALOG_VERSION = ${JSON.stringify(coreCatalog.version)}
export const UNKNOWN_COMPONENT_NAME = ${JSON.stringify(coreCatalog.unknownComponent)}

/** Every component in source order (the hidden placeholder included). */
export const COMPONENT_NAMES = ${JSON.stringify(names)} as const
export type ComponentName = (typeof COMPONENT_NAMES)[number]
export const NATIVE_COMPONENTS = ${JSON.stringify(natives)} as const
export const MACRO_COMPONENTS = ${JSON.stringify(macros)} as const
/** The core-lite subset (no overlays, media or Chart). */
export const LITE_COMPONENTS = ${JSON.stringify(lite)} as const
export const COMPONENT_GROUPS: Record<ComponentName, string> = ${JSON.stringify(Object.fromEntries(components.map(([n, d]) => [n, d.group])))}
/** The specimen id each component has on ui.exponential.at (\`exponential-ui-<kebab name>\`, derived so the bundles carry no table, VAPP-289). */
export const SPECIMEN_IDS = Object.fromEntries(COMPONENT_NAMES.map((n) => [n, \`exponential-ui-\${n.replace(/([a-z0-9])([A-Z])/g, \`$1-$2\`).toLowerCase()}\`])) as Record<ComponentName, string>

export const ENUMS = {
${enumConsts}
} as const
${enumTypes}

/** Token NAMES by group (values come from a theme). */
export const TOKEN_NAMES = {
${tokenConsts}
} as const
export type TokenGroup = keyof typeof TOKEN_NAMES

export const STYLE_KEY_NAMES = ${JSON.stringify(STYLE_KEYS)} as const
export const STYLE_STATE_NAMES = ${JSON.stringify(STYLE_STATES)} as const

/** The Icon prop's vocabulary: packages/icons/icons.json at generate time. */
export const ICON_NAMES = ${JSON.stringify(icons)} as const

/** #EXP-1764: the icons painted in their OWN colour (\`#RRGGBB\`), never the text colour. */
export const ICON_FILLS: Record<string, string> = ${JSON.stringify(Object.fromEntries(iconFills()))}

export const FUNCTION_NAMES = ${JSON.stringify(coreCatalog.functions.names)} as const

/** VAPP-108: the typed authoring surface (src/define.ts). A ~ prop also
 *  takes a binding or a call, a ^ prop a per-breakpoint object. */
export type Dynamic = DataBinding | FunctionCall
export type Responsive<T> = { base: T; sm?: T; md?: T; lg?: T; xl?: T }
export type IconName = (typeof ICON_NAMES)[number]
/** The shared shapes (catalog \`defs\`). */
export interface Shapes {
${shapeTypes}
}
/** Every component's props, by name. */
export interface ComponentProps {
${propTypes}
}

/** A2UI basic component → core component. */
export const BASIC_COMPONENT_MAP: Record<string, string> = ${JSON.stringify(Object.fromEntries(basicTargets))}
/** A2UI basic icon name → icons.json name (the placeholder where none exists). */
export const BASIC_ICON_MAP: Record<string, string> = ${JSON.stringify(Object.fromEntries(basicIcons))}
export const BASIC_PLACEHOLDER_ICON = ${JSON.stringify(basicMap.placeholderIcon)}
`
}

// ---------------------------------------------------------------------------
// Swift / Kotlin / Rust: parallel arrays, like packages/domain-contract
// ---------------------------------------------------------------------------

const propNames = (def: ComponentDef) => Object.keys(def.props).join(`,`)
const eventNames = (def: ComponentDef) => (def.events ?? []).join(`,`)
const rows = {
  componentNames: names,
  componentKinds: components.map(([, d]) => d.kind),
  componentGroups: components.map(([, d]) => d.group),
  componentChildren: components.map(([, d]) => d.children),
  componentProps: components.map(([, d]) => propNames(d)),
  componentEvents: components.map(([, d]) => eventNames(d)),
  componentSlots: components.map(([, d]) => (d.slots ?? []).join(`,`)),
  componentSpecimenIds: names.map(specimenId),
  liteComponents: lite,
  nativeComponents: natives,
  macroComponents: macros,
  enumNames: Object.keys(coreCatalog.enums),
  enumValues: Object.values(coreCatalog.enums).map((v) => v.join(`|`)),
  tokenGroups: Object.keys(TOKEN_GROUPS),
  tokenNames: Object.values(TOKEN_GROUPS).map((v) => v.join(`,`)),
  styleKeys: [...STYLE_KEYS],
  styleLayoutKeys: Object.keys(styleJson.layout),
  styleStates: [...STYLE_STATES],
  styleLayoutEffectKeys: Object.entries(styleJson.visual).filter(([, spec]) => (spec as { layoutEffect?: boolean }).layoutEffect).map(([k]) => k),
  functionNames: [...coreCatalog.functions.names],
  coreFunctionNames: Object.keys(coreCatalog.functions.core),
  bindFunctionNames: [...BIND_FUNCTION_NAMES],
  stringIds: [...STRING_IDS],
  stringDefaults: STRING_IDS.map((id) => DEFAULT_STRINGS[id]),
  codeLanguageNames: [...CODE_LANGUAGE_NAMES],
  builtinIconSlots: Object.keys(builtinIcons()),
  builtinIconNames: Object.values(builtinIcons()),
  iconFillNames: iconFills().map(([name]) => name),
  iconFillColors: iconFills().map(([, fill]) => fill),
  weekStartRegions: Object.values(localeJson.weekStart.regions).flat(),
  weekStartDays: Object.entries(localeJson.weekStart.regions).flatMap(([day, regions]) => regions.map(() => day)),
  likelyRegionLanguages: Object.keys(localeJson.likelyRegion),
  likelyRegions: Object.values(localeJson.likelyRegion),
  rtlLanguages: [...localeJson.rtl],
  languageAliases: Object.keys(localeJson.languageAliases),
  languageAliasTargets: Object.values(localeJson.languageAliases),
  rtlMirroredIcons: [...localeJson.rtlMirroredIcons],
  a11yRoles: [...A11Y_ROLES],
  rowSlotComponents: components.filter(([, d]) => hasRowSlots(d)).map(([n]) => n),
  dataProps: components.flatMap(([n, d]) => Object.entries(d.props).filter(([, schema]) => isDataSchema(schema)).map(([p]) => `${n}.${p}`)),
  basicComponentNames: basicTargets.map(([b]) => b),
  basicComponentTargets: basicTargets.map(([, t]) => t),
  basicIconNames: basicIcons.map(([b]) => b),
  basicIconTargets: basicIcons.map(([, t]) => t),
  // Round 2 (docs/round-2-contract.md).
  animationNames: [...ANIMATION_NAMES],
  formatFunctionNames: [...FORMAT_FUNCTION_NAMES],
  layoutConstantNames: Object.keys(LAYOUT_CONSTANTS),
  layoutConstantValues: Object.values(LAYOUT_CONSTANTS).map(String),
  // VAPP-103: the input limits (catalog/limits.json).
  limitNames: Object.keys(LIMITS),
  limitValues: Object.values(LIMITS).map(String),
}
const liteFlags = components.map(([, d]) => d.lite && !d.hidden)
const scalars: [string, string][] = [
  [`catalogId`, CORE_CATALOG_ID],
  [`liteCatalogId`, CORE_LITE_CATALOG_ID],
  [`basicCatalogId`, A2UI_BASIC_CATALOG_ID],
  [`a2uiVersion`, HOST_JSON.a2uiVersion],
  // VAPP-289: the round-8 name of a2uiVersion, kept until the natives read a2uiVersion (phase B).
  [`basicVersion`, HOST_JSON.a2uiVersion],
  [`catalogVersion`, coreCatalog.version],
  [`unknownComponent`, coreCatalog.unknownComponent],
  [`basicPlaceholderIcon`, basicMap.placeholderIcon],
  [`styleMediaPattern`, styleJson.conditions.media],
  [`styleTransformPattern`, styleJson.conditions.transform],
  [`defaultLocale`, localeJson.default],
  [`weekStartDefault`, String(localeJson.weekStart.default)],
]

function renderSwift(): string {
  const arr = (name: string, values: readonly string[]) => `    public static let ${name}: [String] = [${values.map((v) => JSON.stringify(v)).join(`, `)}]`
  return `// ${HEADER.split(`\n`).join(`\n// `)}

import Foundation

public enum ExponentialUICatalog {
${scalars.map(([k, v]) => `    public static let ${k}: String = ${JSON.stringify(v)}`).join(`\n`)}
${Object.entries(rows).map(([k, v]) => arr(k, v)).join(`\n`)}
    public static let componentLite: [Bool] = [${liteFlags.join(`, `)}]
    public static let acceptV09: Bool = ${HOST_JSON.transition.acceptV09}
}
`
}

/** A Kotlin string literal: JSON's escapes plus `\$` (a bare `$` starts a
 *  string template in Kotlin: `$breakpoint` would read a variable). */
const ktString = (v: string) => JSON.stringify(v).replace(/\$/g, `\\$`)

function renderKotlin(): string {
  const arr = (name: string, values: readonly string[]) => `    val ${name}: List<String> = listOf(${values.map(ktString).join(`, `)})`
  return `// ${HEADER.split(`\n`).join(`\n// `)}

package at.exponential.ui

object ExponentialUICatalog {
${scalars.map(([k, v]) => `    const val ${k}: String = ${ktString(v)}`).join(`\n`)}
${Object.entries(rows).map(([k, v]) => arr(k, v)).join(`\n`)}
    val componentLite: List<Boolean> = listOf(${liteFlags.join(`, `)})
    const val acceptV09: Boolean = ${HOST_JSON.transition.acceptV09}
}
`
}

/** A JSON document as a Rust raw string: minified, no `$comment`s, and a
 *  hash fence long enough that no JSON string can close it early. */
function rustJson(value: unknown): string {
  const text = JSON.stringify(value)
  if (text.includes(`"####`)) throw new Error(`rustJson: the document contains the raw-string fence`)
  return `r####"${text}"####`
}

/** The catalog source files the Rust core (VAPP-86) EMBEDS so the crate
 *  packages standalone: the same JSON the TS reference reads, minus comments. */
function embeddedCatalogJson(): [string, string][] {
  const strip = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(strip)
    if (typeof v === `object` && v !== null) return Object.fromEntries(Object.entries(v).filter(([k]) => k !== `$comment`).map(([k, x]) => [k, strip(x)]))
    return v
  }
  const read = (rel: string) => strip(JSON.parse(readFileSync(join(pkgRoot, rel), `utf8`)))
  return [
    // VAPP-106: the RUNTIME catalog (no descriptions or examples); the text is
    // `catalog.descriptions.generated.rs`, behind the crate's `descriptions` feature.
    [`CORE_CATALOG_JSON`, rustJson(catalogSplit.runtime)],
    [`MACROS_JSON`, rustJson(read(`catalog/macros.json`))],
    [`BASIC_MAP_JSON`, rustJson(read(`catalog/basic-map.json`))],
    [`RECIPES_JSON`, rustJson(read(`catalog/recipes.json`))],
    [`STYLE_JSON`, rustJson(read(`catalog/style.json`))],
    [`TOKENS_JSON`, rustJson(read(`catalog/tokens.json`))],
    [`STRINGS_JSON`, rustJson(read(`catalog/strings.json`))],
    [`LOCALE_JSON`, rustJson(read(`catalog/locale.json`))],
    [`CODE_JSON`, rustJson(read(`catalog/code.json`))],
    [`A11Y_JSON`, rustJson(read(`catalog/a11y.json`))],
    [`LAYOUT_JSON`, rustJson(read(`catalog/layout.json`))],
    [`LIMITS_JSON`, rustJson(read(`catalog/limits.json`))],
  ]
}

/** VAPP-106: the catalog's text for a Rust host that builds its own prompt,
 *  behind the crate's `descriptions` feature (off by default: nothing in the
 *  core reads it). */
function renderRustDescriptions(): string {
  return `// ${HEADER.split(`\n`).join(`\n// `)}
// VAPP-106: the model-facing descriptions and examples of the catalog (generated/core.descriptions.json), keyed like the
// catalog; the runtime catalog (CORE_CATALOG_JSON) carries none. Embedded only with the \`descriptions\` feature.

pub const CORE_CATALOG_DESCRIPTIONS_JSON: &str = ${rustJson(catalogSplit.descriptions)};
`
}

function renderRust(icons: string[]): string {
  const arr = (name: string, values: readonly string[]) => `pub const ${screaming(name)}: &[&str] = &[${values.map((v) => JSON.stringify(v)).join(`, `)}];`
  return `// ${HEADER.split(`\n`).join(`\n// `)}
// Consumed by apps/desktop/crates/eui-core (VAPP-86): the generator copies this file to
// apps/desktop/crates/eui-core/src/generated/catalog.rs (drift-gated there) so the crate packages standalone.

${scalars.map(([k, v]) => `pub const ${screaming(k)}: &str = ${JSON.stringify(v)};`).join(`\n`)}
${Object.entries(rows).map(([k, v]) => arr(k, v)).join(`\n`)}
pub const COMPONENT_LITE: &[bool] = &[${liteFlags.join(`, `)}];
/// VAPP-289 (round 9 §14): while true, every core entry upgrades 0.9 input through upgrade09.
pub const ACCEPT_V09: bool = ${HOST_JSON.transition.acceptV09};
/// The Icon prop's vocabulary (packages/icons/icons.json at generate time).
${arr(`iconNames`, icons)}
/// The catalog source files, embedded (minified, \`$comment\`s dropped); parse once at startup.
${embeddedCatalogJson().map(([name, raw]) => `pub const ${name}: &str = ${raw};`).join(`\n`)}
`
}

// ---------------------------------------------------------------------------
// Docs: one page per component, as data
// ---------------------------------------------------------------------------

function docProps(def: ComponentDef) {
  const typeOf = (s: PropSchema): string =>
    s.type === `enum`
      ? (s.values ?? coreCatalog.enums[s.enum ?? ``] ?? []).join(` | `)
      : s.type === `array`
        ? `[${s.items ? (s.items.type === `object` ? (s.items.shape ?? `object`) : typeOf(s.items)) : ``}]`
        : s.type === `object`
          ? (s.shape ?? `object`)
          : s.type
  return Object.entries(def.props).map(([name, s]) => ({
    name,
    type: typeOf(s),
    required: s.required ?? false,
    bindable: s.bindable ?? false,
    responsive: s.responsive ?? false,
    ...(s.default !== undefined ? { default: s.default } : {}),
    description: s.description,
  }))
}

function renderDocs() {
  const basicAliases: Record<string, string[]> = {}
  for (const [basic, rule] of Object.entries(basicMap.components)) (basicAliases[rule.to] ??= []).push(basic)
  basicAliases.Heading = [`Text (h1–h4)`]
  basicAliases.Avatar = [`Image (avatar)`]
  basicAliases.Textarea = [`TextField (longText)`]
  basicAliases.Segmented = [`ChoicePicker (chips)`]
  basicAliases.Select = [`ChoicePicker (filterable, multipleSelection)`]
  return {
    $comment: `${HEADER} One entry per component: what ui.exponential.at renders as the component's page (VAPP-93). specimenId = the id of its specimen there, painted once each renderer lands.`,
    catalogId: CORE_CATALOG_ID,
    liteCatalogId: CORE_LITE_CATALOG_ID,
    groups: [...new Set(components.map(([, d]) => d.group))],
    components: visible.map(([name, def]) => ({
      name,
      kind: def.kind,
      group: def.group,
      lite: def.lite,
      ...(def.status ? { status: def.status } : {}),
      specimenId: specimenId(name),
      description: def.description,
      children: def.children,
      slots: def.slots ?? [],
      events: def.events ?? [],
      basic: basicAliases[name] ?? [],
      props: docProps(def),
      accessibility: COMPONENT_A11Y[name] ?? null,
      example: { id: `example`, component: name, ...(def.example ?? {}) },
      macro: def.kind === `macro` ? `Expands to natives (see catalog/macros.json); the theme styles its parts by recipe ${name}/<part>.` : null,
    })),
  }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

interface ComponentCase {
  name: string
  node: NestedNode
}

function childFor(name: string): NestedNode {
  return { id: `${snake(name)}_child`, component: `Text`, props: { text: `Child` } }
}

/** Macros whose child switches the look (a ColorPicker's child = its own
 *  trigger): the sweep keeps them childless and ONE `<name>/child` case
 *  shows the child. */
const CHILD_SWITCHES = new Set([`ColorPicker`])

function makeCase(name: string, def: ComponentDef, suffix: string, props: Record<string, unknown>): ComponentCase {
  const node: NestedNode = { id: snake(name), component: name, props }
  // #VAPP-284: children make a Row a foldable parent; the sweep keeps the plain row (foldCases cover the parent).
  if (def.children !== `none` && name !== `Row` && !CHILD_SWITCHES.has(name)) node.children = [childFor(name)]
  return { name: `${name}/${suffix}`, node }
}

/** A responsive value worth expanding: the default (else the first value)
 *  at `base`, a different value from md (lg too for numbers). */
function responsiveSample(schema: PropSchema): Record<string, unknown> {
  if (schema.type === `number`) return { base: 1, md: 2, lg: 3 }
  if (schema.type === `boolean`) return { base: false, md: true }
  const values = schema.values ?? coreCatalog.enums[schema.enum ?? ``] ?? []
  const base = schema.default ?? values[0]
  return { base, md: values.find((v) => v !== base) ?? base }
}

/** Round 1: every BINDABLE prop of every macro bound to `/<prop>` and every
 *  RESPONSIVE prop of every macro given a breakpoint object: the cases where
 *  the expander must emit instead of evaluate (docs/round-1-contract.md §1). */
function boundMacroCases(): ComponentCase[] {
  const cases: ComponentCase[] = []
  for (const [name, def] of visible) {
    if (def.kind !== `macro`) continue
    const base = { ...(def.example ?? {}) }
    for (const [prop, schema] of Object.entries(def.props)) {
      if (schema.bindable) cases.push(makeCase(name, def, `bound:${prop}`, { ...base, [prop]: { "@path": `/${prop}` } }))
      if (schema.responsive) cases.push(makeCase(name, def, `responsive:${prop}`, { ...base, [prop]: responsiveSample(schema) }))
    }
  }
  return cases
}

/** FEED-205: the Autotax dashboard's chart. */
const INCOME_EXPENSES = {
  kind: `bar`,
  title: `Income vs expenses`,
  categories: [`Jan`, `Feb`, `Mar`, `Apr`],
  series: [
    { name: `Income`, values: [4200, 3900, 4600, 5100], tone: `success` },
    { name: `Expenses`, values: [3100, 3400, 2900, 3800], tone: `danger` },
  ],
}

/** States the sweep cannot reach: a prop that only shows in a state, set
 *  together with it, with no custom child so the prop itself paints
 *  (EXP-1259: the pending Button paints `pendingLabel`). */
const STATE_CASES: Record<string, Record<string, Record<string, unknown>>> = {
  // #EXP-1629: a `button` trigger leads with the picked option's own glyph, in its tint.
  Select: { "triggerVariant=button+icon": { triggerVariant: `button`, value: `review`, options: [{ label: `In Review`, value: `review`, icon: `status-in-review`, iconColor: `$color.success` }, { label: `Do nothing`, value: `none` }] } },
  // #EXP-1674: an href press target.
  Row: { href: { title: `Docs`, href: `https://ui.exponential.at` } },
  // #EXP-1736: an incoming bubble that opens its sender (a cross-session sender).
  MessageBubble: { href: { side: `in`, href: `https://ui.exponential.at` } },
  Button: { href: { label: `Docs`, href: `https://ui.exponential.at` }, "loading=true+pendingLabel": { loading: true, pendingLabel: `Submitting` } },
  // EXP-1548: wider than its box: the table scrolls sideways, cells wrap,
  // the `truncate` column keeps one line with an ellipsis.
  Table: { wide: wideTableProps() },
  // Round 8: the label row with only its trailing text, the caption under the bar.
  Progress: { "valueText-only": { label: ``, valueText: `41 MB/s · 12 min` }, detail: { detail: `3 of 8 files` } },
  // Round 8: the collapsed line on the first step, and with no step current.
  Stepper: { "collapse=always+current=0": { collapse: `always`, current: 0 }, "collapse=always+current=9": { collapse: `always`, current: 9 } },
  // Round 8: mixed wins over checked in the look.
  Checkbox: { "indeterminate=true+checked=true": { indeterminate: true, checked: true } },
  // FEED-205: income vs expenses per month in EUR (ticks, tooltip, values),
  // and the same over a query's rows in compact euros.
  Chart: {
    "valueFormat=currency": { ...INCOME_EXPENSES, showValues: true, valueFormat: { type: `currency`, currency: `EUR`, decimals: 0 } },
    "rows+valueFormat=compact": {
      categories: undefined,
      rows: INCOME_EXPENSES.categories.map((month, i) => ({ month, income: INCOME_EXPENSES.series[0].values[i], expenses: INCOME_EXPENSES.series[1].values[i] })),
      categoryField: `month`,
      series: [{ name: `Income`, field: `income`, tone: `success` }, { name: `Expenses`, field: `expenses`, tone: `danger` }],
      valueFormat: { type: `compact`, currency: `EUR` },
    },
  },
}


/** EXP-1548: a table whose columns need more than a phone's width. */
function wideTableProps(): Record<string, unknown> {
  return {
    columns: [
      { key: `issue`, label: `Issue`, width: 96 },
      { key: `title`, label: `Title` },
      { key: `board`, label: `Board` },
      { key: `assignee`, label: `Assignee` },
      { key: `status`, label: `Status`, type: `badge` },
      { key: `summary`, label: `Summary`, truncate: true },
      { key: `estimate`, label: `Estimate`, type: `number`, align: `end` },
    ],
    rows: [
      { id: `a`, issue: `APP-9`, title: `Fix crash when uploading HEIC photos from the share sheet`, board: `Mobile`, assignee: `Alice Lindqvist`, status: `In progress`, summary: `The share extension hands over a file URL the uploader cannot read once the sheet closes`, estimate: 3 },
      { id: `b`, issue: `APP-21`, title: `Offline queue for issue edits`, board: `Sync`, assignee: `Bob Okafor`, status: `Todo`, summary: `Edits made without a connection are lost on relaunch`, estimate: 8 },
      { id: `c`, issue: `APP-18`, title: `Reduce cold start below 800 ms`, board: `Performance`, assignee: `Chen Wei`, status: `Done`, summary: `Defer the sync bootstrap until after the first frame`, estimate: 5 },
    ],
  }
}

/** FEED-202: the Autotax Expenses screen's ledger: dd.MM.yy dates, EUR
 *  amounts, an EÜR category and the receipt status. */
const EXPENSES = [
  { id: `e1`, date: `2026-11-03`, vendor: `Hetzner Online`, euer_line: `software`, receipt_status: `booked`, net: 38.66, vat: 7.73, gross: 46.39, updatedAt: `2026-11-04T09:12:00Z` },
  { id: `e2`, date: `2026-10-28`, vendor: `ÖBB Ticket`, euer_line: `travel`, receipt_status: `received`, net: 74.17, vat: 7.42, gross: 81.59, updatedAt: `2026-10-29T17:40:00Z` },
  { id: `e3`, date: `2026-10-21`, vendor: `Libro`, euer_line: `office`, receipt_status: `missing`, net: 20.75, vat: 4.15, gross: 24.9, updatedAt: `2026-10-21T11:05:00Z` },
]

/** FEED-202: a Table with its sum row (`totals`). */
function totalsTableProps(): Record<string, unknown> {
  return {
    columns: [
      { key: `vendor`, label: `Vendor` },
      { key: `date`, label: `Date`, type: `date`, format: `dd.MM.yy`, width: 104 },
      { key: `net`, label: `Net`, type: `currency`, currency: `EUR`, decimals: 2, align: `end`, width: 96 },
      { key: `gross`, label: `Gross`, type: `currency`, currency: `EUR`, decimals: 2, align: `end`, width: 96 },
    ],
    rows: EXPENSES,
    totals: true,
  }
}

/** FEED-202: the host-filled RecordTable of an `expense` kind (columns =
 *  date dd.MM.yy, vendor, euer_line as Category, receipt_status as Receipt,
 *  net / vat / gross in EUR; search, the receipt_status filter, totals). */
function expensesRecordTableProps(): Record<string, unknown> {
  const eur = { type: `currency`, align: `end`, currency: `EUR`, decimals: 2, width: 96, sortable: true }
  return {
    kind: `expense`,
    title: `Expenses`,
    search: true,
    searchValue: ``,
    filterLabel: `Receipt status`,
    filterOptions: [{ label: `$string.all`, value: `` }, ...[`missing`, `received`, `booked`].map((v) => ({ label: v, value: v }))],
    filterValue: ``,
    totals: true,
    cols: [
      { key: `date`, label: `Date`, sortable: true, type: `date`, format: `dd.MM.yy`, width: 104 },
      { key: `vendor`, label: `Vendor`, sortable: true, truncate: true },
      { key: `euer_line`, label: `Category`, sortable: true, truncate: true },
      { key: `receipt_status`, label: `Receipt`, sortable: true, truncate: true },
      { key: `net`, label: `Net`, ...eur },
      { key: `vat`, label: `VAT`, ...eur },
      { key: `gross`, label: `Gross`, ...eur },
      { key: `updatedAt`, label: `Updated`, type: `relativeTime`, width: 136, sortable: true },
      { key: `menu`, label: ``, type: `slot`, slot: `menu`, width: 80 },
    ],
    rows: EXPENSES,
    count: `3 records`,
    fields: [],
    path: `/recordTable/expenses/editor/draft`,
    editorOpen: false,
    deleteOpen: false,
  }
}

/** Every visible component: its example, then every enum value of every enum
 *  prop and both values of every boolean prop, over the example, then its
 *  STATE_CASES. */
function componentCases(): ComponentCase[] {
  const cases: ComponentCase[] = []
  for (const [name, def] of visible) {
    const base = { ...(def.example ?? {}) }
    const make = (suffix: string, props: Record<string, unknown>) => makeCase(name, def, suffix, props)
    cases.push(make(`example`, base))
    for (const [prop, schema] of Object.entries(def.props)) {
      if (schema.type === `enum`) {
        const values = schema.values ?? coreCatalog.enums[schema.enum ?? ``] ?? []
        for (const value of values) cases.push(make(`${prop}=${value}`, { ...base, [prop]: value }))
      } else if (schema.type === `boolean`) {
        for (const value of [true, false]) cases.push(make(`${prop}=${value}`, { ...base, [prop]: value }))
      }
    }
    for (const [suffix, props] of Object.entries(STATE_CASES[name] ?? {})) cases.push({ name: `${name}/${suffix}`, node: { id: snake(name), component: name, props: { ...base, ...props } } })
    if (CHILD_SWITCHES.has(name)) cases.push({ name: `${name}/child`, node: { id: snake(name), component: name, props: base, children: [childFor(name)] } })
  }
  return cases
}

// ---------------------------------------------------------------------------
// Specimens (VAPP-93): one surface per component, the page shot of
// ui.exponential.at, painted by every renderer from the same JSON
// ---------------------------------------------------------------------------

/** Components that size to their content: their cases sit at the start of the
 *  column instead of stretching across it. */
const INLINE_SPECIMENS = new Set([
  `Avatar`, `Badge`, `Button`, `Checkbox`, `Chip`, `Icon`, `Link`, `Menu`,
  `Radio`, `Ring`, `Spinner`, `Switch`, `Toggle`, `Segmented`, `Tooltip`, `Popover`, `Pagination`, `QrCode`,
])
/** The enum props a specimen shows, in priority order (at most two of them). */
const SPECIMEN_ENUMS = [`variant`, `shape`, `surface`, `openOn`, `type`, `kind`, `level`, `size`, `tone`, `tint`, `chevron`, `density`, `orientation`, `direction`, `fit`]
/** The boolean props a specimen shows `true` for, in priority order (at most two). */
const SPECIMEN_BOOLEANS = [`checked`, `pressed`, `selected`, `disabled`, `loading`, `busy`, `removable`, `attachments`, `collapsible`, `tree`, `padded`, `divided`, `rounded`]
const SPECIMEN_MAX_CASES = 8

const text = (id: string, value: string, variant = `body`): NestedNode => ({ id, component: `Text`, props: { text: value, variant } })
const button = (id: string, label: string, variant = `default`): NestedNode => ({ id, component: `Button`, props: { label, variant } })
const row = (id: string, title: string, meta: string, depth = 0): NestedNode => ({ id, component: `Row`, props: { title, meta, pressable: true, ...(depth ? { depth } : {}) } })
const card = (id: string, title: string, description: string): NestedNode => ({ id, component: `Card`, props: { title, description } })
/** The one child a Window specimen frames: a padded column that fills the body. */
const windowBody = (id: string): NestedNode => ({
  id,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: `$spacing.sm`, padding: `$spacing.xl`, flexGrow: 1, alignItems: `flex-start` },
  children: [text(`${id}_title`, `Release 0.18`, `label`), text(`${id}_text`, `4 steps · 2 done`, `muted`), button(`${id}_open`, `Open board`, `outline`)],
})

/** What a specimen puts inside a container instead of the cases' generic
 *  `Child` (a component missing here keeps the case's children as they are). */
const SPECIMEN_CHILDREN: Record<string, NestedNode[]> = {
  Box: [text(`t1`, `A Box lays its children out by style.`), text(`t2`, `Flex, grid, spacing and colours come from the whitelist.`, `muted`)],
  Stack: [
    { id: `a`, component: `Avatar`, props: { name: `Ada Lovelace`, seed: `demo-ada`, size: `sm` } },
    text(`t`, `Ada Lovelace`),
    { id: `b`, component: `Badge`, props: { text: `Owner`, variant: `outline` } },
  ],
  Grid: [1, 2, 3, 4, 5, 6].map((n) => card(`g${n}`, `Cell ${n}`, `Grid item`)),
  Card: [text(`t`, `3 open issues · 2 in review`), button(`b`, `Open board`, `outline`)],
  Window: [windowBody(`body`)],
  List: [text(`l1`, `Inbox`), text(`l2`, `Drafts`), text(`l3`, `Archive`)],
  // Round 3: a Section of nested rows shows the elbow, the tee and a pass-through.
  Section: [row(`r1`, `Release 0.18`, `4 steps`), row(`r2`, `Build the macOS app`, `done`, 1), row(`r3`, `Notarize`, `running`, 2), row(`r4`, `Upload the dmg`, `queued`, 2), row(`r5`, `Publish the release notes`, `queued`, 1), row(`r6`, `Release 0.17`, `3 steps`)],
  Group: [row(`r1`, `Mentions`, `Push + email`), row(`r2`, `Assigned to me`, `Push`)],
  Carousel: [card(`p1`, `Page one`, `Swipe for more`), card(`p2`, `Page two`, ``), card(`p3`, `Page three`, ``)],
  Tabs: [text(`p1`, `The issue panel.`), text(`p2`, `The session panel.`)],
  Accordion: [text(`p`, `Created 2 days ago by Ada; 3 comments.`)],
  Collapsible: [text(`p`, `Retries, timeouts and the proxy live here.`, `muted`)],
  Dialog: [text(`p`, `The board and its 42 issues move to the trash for 48 hours.`)],
  Drawer: [text(`p`, `Status, assignee and labels.`)],
  DockCard: [text(`p`, `Move the waiting cards above the composer and open each one as a bottom sheet.`)],
  Popover: [text(`p`, `Popover content.`)],
  Tooltip: [button(`b`, `Hover me`, `outline`)],
  Button: [],
  // Round 2: real content for the round-1 containers and Resizable.
  Resizable: [text(`p1`, `Boards`, `label`), text(`p2`, `Drag the handle or use the arrow keys.`, `muted`)],
  ScrollArea: [1, 2, 3, 4, 5, 6, 7, 8].map((n) => row(`r${n}`, `Issue ${n}`, `${n}h`)),
  Sidebar: [text(`main`, `Pick a board on the left.`)],
  AlertDialog: [],
  Menu: [button(`trigger`, `More`, `outline`)],
  // VAPP-126: the layout macros, each with the content the eval's corpus gave it.
  Page: [text(`p1`, `General`, `label`), card(`c1`, `Team name`, `Acme`), card(`c2`, `Timezone`, `Europe/Vienna`)],
  Toolbar: [
    { id: `search`, component: `Input`, props: { label: `Search`, name: `q`, type: `search`, placeholder: `Search issues` }, style: { width: 240 } },
    button(`filter`, `Filter`, `outline`),
  ],
  SplitView: [{ id: `detail`, component: `Page`, props: { title: `APP-9`, subtitle: `Fix crash when uploading HEIC photos` }, children: [text(`body`, `Uploading a HEIC photo from the iOS share sheet crashes the app.`), text(`meta`, `Opened 2 days ago by Ada.`, `muted`)] }],
  FieldGroup: [
    { id: `starts`, component: `Input`, props: { label: `Starts`, name: `starts`, value: `09:00` } },
    { id: `ends`, component: `Input`, props: { label: `Ends`, name: `ends`, value: `10:30` } },
  ],
  FormActions: [],
  Form: [
    { id: `title`, component: `Input`, props: { label: `Title`, name: `title`, placeholder: `What changed?` } },
    { id: `notify`, component: `Switch`, props: { label: `Notify watchers`, name: `notify`, checked: true } },
    { id: `save`, component: `Button`, props: { label: `Save`, submit: true } },
  ],
}

/** EXP-1324: a Trace as the transcript paints one (reasoning, a failed
 *  command with its output, a running edit, a pending step). */
function traceLines(size: string): NestedNode[] {
  const item = (id: string, props: Record<string, unknown>, children?: NestedNode[]): NestedNode => ({ id, component: `TraceItem`, props: { size, ...props }, ...(children ? { children } : {}) })
  return [
    item(`think`, { title: `Thought for 5s`, icon: `trace-reasoning` }),
    item(`run`, { title: `Ran bun test`, icon: `trace-terminal`, status: `error`, collapsible: true, open: true }, [
      { id: `out`, component: `CodeBlock`, props: { code: `\u001b[32m(pass)\u001b[0m Store > put [0.2ms]\n\u001b[31m(fail)\u001b[0m Store > get [1.2ms]\n\n 1 pass\n 1 fail`, language: `ansi`, title: `Output`, size: `sm`, variant: `destructive` } },
    ]),
    item(`edit`, { title: `Editing src/store.ts`, icon: `trace-edit`, status: `running` }),
    item(`subagent`, { title: `Ran the Explore agent`, icon: `trace-subagent`, description: `1m 12s`, collapsible: true }),
    item(`next`, { title: `Run the suite again`, icon: `trace-terminal`, status: `pending` }),
  ]
}

/** VAPP-280: the Select row of the SettingsRow specimen. */
function settingsSelectRow(stackBelow?: string): NestedNode {
  return {
    id: `digest`,
    component: `SettingsRow`,
    props: { label: `Digest`, description: `How often the summary email goes out.`, ...(stackBelow ? { stackBelow } : {}) },
    children: [{ id: `digest_select`, component: `Select`, props: { name: `digest`, value: `daily`, options: [{ label: `Hourly`, value: `hourly` }, { label: `Daily`, value: `daily` }, { label: `Off`, value: `off` }] } }],
  }
}

/** VAPP-280: a settings page's group: Group (the card) over a bare Section
 *  (the hairline-divided rows). */
function settingsGroup(): NestedNode {
  return {
    id: `group`,
    component: `Group`,
    props: { title: `Notifications` },
    children: [{
      id: `rows`,
      component: `Section`,
      children: [
        { id: `email`, component: `SettingsRow`, props: { label: `Email notifications`, stackBelow: `never` }, children: [{ id: `email_switch`, component: `Switch`, props: { label: ``, name: `email`, checked: true } }] },
        settingsSelectRow(),
        { id: `volume`, component: `SettingsRow`, props: { label: `Ring volume` }, children: [{ id: `volume_slider`, component: `Slider`, props: { min: 0, max: 100, step: 5, value: 60, icon: `media-volume-up`, endLabel: `60%`, showValue: false } }] },
      ],
    }],
  }
}

/** #VAPP-285: a repository as a Tree (folders open and closed, a selected
 *  file, a lazy folder): the specimen, the macro cases and fixtures/tree.json. */
const FILE_TREE: Record<string, unknown>[] = [
  { id: `apps`, title: `apps`, children: [
    { id: `apps/web`, title: `web`, children: [
      { id: `apps/web/main.ts`, title: `main.ts`, meta: `M` },
      { id: `apps/web/router.ts`, title: `router.ts` },
    ] },
    { id: `apps/desktop`, title: `desktop`, hasChildren: true },
  ] },
  { id: `packages`, title: `packages`, children: [{ id: `packages/ui`, title: `ui`, children: [] }] },
  { id: `README.md`, title: `README.md` },
  { id: `package.json`, title: `package.json`, meta: `A` },
]
const TREE_ON = { select: { event: { name: `open` } }, toggle: { event: { name: `fold` } }, load: { event: { name: `fetch` } } }
const fileTree = (props: Record<string, unknown> = {}, extra: Partial<NestedNode> = {}): NestedNode => ({ id: `files`, component: `Tree`, props: { label: `Files`, items: FILE_TREE, expanded: [`apps`, `apps/web`], value: `apps/web/main.ts`, ...props }, ...extra })
function treeCases(): ComponentCase[] {
  return [
    { name: `Tree/files`, node: fileTree({}, { on: TREE_ON }) },
    { name: `Tree/lazy-open`, node: fileTree({ expanded: [`apps`, `apps/desktop`] }, { on: TREE_ON }) },
    { name: `Tree/nothing-open`, node: fileTree({ expanded: [] }) },
    { name: `Tree/bound`, node: fileTree({ expanded: { "@path": `/open` }, value: { "@path": `/picked` } }, { on: TREE_ON }) },
  ]
}

/** EXP-1324: components whose specimen is hand-written (TraceItem's `open`
 *  would otherwise read as an overlay's). */
const SPECIMEN_OVERRIDES: Record<string, { label: string; node: NestedNode }[]> = {
  // #EXP-1760: the identity pickers of boards, agents and labels: a pick
  // (tinted), none (the dashed placeholder) and the open grid of the board
  // set (with No icon) / the label palette, in a box tall enough for it.
  IconPicker: [
    { label: `value · color`, node: { id: `icon`, component: `IconPicker`, props: { value: `rocket`, color: `#3b82f6` } } },
    { label: `empty`, node: { id: `icon-none`, component: `IconPicker`, props: {} } },
  ],
  ColorPicker: [
    { label: `value`, node: { id: `color`, component: `ColorPicker`, props: { value: `#3b82f6` } } },
    { label: `empty`, node: { id: `color-none`, component: `ColorPicker`, props: {} } },
    // The IDE's Statuses / Labels: a status glyph as the trigger (a ghost
    // button around the child) and the create form's inline grid.
    { label: `child · a status glyph`, node: { id: `color-glyph`, component: `ColorPicker`, props: { value: `#22c55e` }, children: [{ id: `color-glyph-icon`, component: `Icon`, props: { name: `circle-dashed`, size: `sm` } }] } },
    { label: `inline`, node: { id: `color-inline`, component: `ColorPicker`, props: { value: `#22c55e`, inline: true } } },
  ],
  // #VAPP-285: a repository: folders open and closed, a selected file, an
  // empty folder; then a lazy folder opened (its loading row).
  Tree: [
    { label: `files`, node: { ...fileTree({ expanded: [`apps`, `apps/web`, `packages`] }), style: { maxWidth: 360 } } },
    { label: `hasChildren · loading`, node: { ...fileTree({ expanded: [`apps`, `apps/desktop`], value: undefined }), style: { maxWidth: 360 } } },
  ],
  // VAPP-126: the layout macros show their example and the one variant that
  // changes the picture (SplitView's `open` is no overlay: on a wide surface
  // both panes show; StatTile's tones differ only in the change's colour).
  Page: [
    { label: `example`, node: { id: `page`, component: `Page`, props: { title: `Settings`, subtitle: `Acme`, back: true } } },
    { label: `padded · false`, node: { id: `page`, component: `Page`, props: { title: `Members`, padded: false } } },
  ],
  // EXP-1556: the vertical Separator is an extra (SPECIMEN_EXTRA: its row
  // keeps its own children); the horizontal List gets a gap.
  Separator: [{ label: `example`, node: { id: `separator`, component: `Separator`, props: { orientation: `horizontal` } } }],
  List: [
    { label: `example`, node: { id: `list`, component: `List`, props: { direction: `vertical`, divided: true }, children: SPECIMEN_CHILDREN.List } },
    { label: `direction · horizontal`, node: { id: `list`, component: `List`, props: { direction: `horizontal`, divided: true, gap: `lg` }, children: SPECIMEN_CHILDREN.List } },
  ],
  // EXP-1653: the column stays at every width, so a phone frame shows the
  // shell and not only the pane (`md` hides it there with no way back).
  Sidebar: [{ label: `collapseBelow · never`, node: { id: `sidebar`, component: `Sidebar`, props: { side: `left`, width: 240, collapseBelow: `never` } } }],
  SplitView: [{ label: `example`, node: { id: `split_view`, component: `SplitView`, props: { width: 280 } } }],
  FormActions: [
    { label: `example`, node: { id: `form_actions`, component: `FormActions`, props: { submit: `Save`, cancel: true } } },
    { label: `destructive · true`, node: { id: `form_actions`, component: `FormActions`, props: { submit: `Delete team`, cancel: true, destructive: true } } },
  ],
  // VAPP-280: no generated cases; its cases are SPECIMEN_EXTRA (a Group
  // root must keep its own rows, not the Group demo children).
  SettingsRow: [],
  StatTile: [
    { label: `example`, node: { id: `stat_tile`, component: `StatTile`, props: { label: `Active users`, value: `1,284`, delta: `+12 %`, tone: `success`, trend: [3, 4, 3, 5, 6, 8] } } },
    { label: `tone · danger`, node: { id: `stat_tile`, component: `StatTile`, props: { label: `Error rate`, value: `0.42 %`, delta: `+0.1 %`, tone: `danger`, trend: [2, 2, 3, 2, 4, 5] } } },
    { label: `no trend`, node: { id: `stat_tile`, component: `StatTile`, props: { label: `Open issues`, value: `37` } } },
  ],
  Trace: [`default`, `sm`].map((size) => ({ label: size === `sm` ? `size · sm` : `example`, node: { id: `trace`, component: `Trace`, props: { size }, children: traceLines(size) } })),
  TraceItem: [
    { label: `example`, node: { id: `item`, component: `TraceItem`, props: { title: `Ran bun test`, icon: `trace-terminal`, description: `4s`, collapsible: true, open: true }, children: [{ id: `out`, component: `CodeBlock`, props: { code: `\u001b[32m✓\u001b[0m 12 tests passed\nDone in 4.1s`, language: `ansi`, title: `Output`, size: `sm` } }] } },
    { label: `status · running`, node: { id: `item`, component: `TraceItem`, props: { title: `Reading src/startup.kt`, icon: `trace-read`, status: `running` } } },
    { label: `status · error`, node: { id: `item`, component: `TraceItem`, props: { title: `The general-purpose agent failed`, icon: `trace-subagent`, description: `9s`, status: `error` } } },
    { label: `status · pending`, node: { id: `item`, component: `TraceItem`, props: { title: `Waiting on the web captures`, icon: `trace-background`, status: `pending` } } },
    { label: `size · sm`, node: { id: `item`, component: `TraceItem`, props: { title: `Searched persistOnStop`, icon: `trace-search`, size: `sm`, collapsible: true } } },
    // VAPP-162: the two Plan step states.
    { label: `status · done`, node: { id: `item`, component: `TraceItem`, props: { title: `Opened augustiner-keller.de`, icon: `globe`, status: `done` } } },
    { label: `status · skipped`, node: { id: `item`, component: `TraceItem`, props: { title: `Mail the confirmation to Lisa`, icon: `mail`, status: `skipped` } } },
  ],
  // VAPP-162: the new low-level pieces, each with the states the drafts show.
  MessageBubble: [
    { label: `example`, node: { id: `bubble`, component: `MessageBubble`, props: { text: `Is the printer fixed?`, side: `out`, time: `18:00`, status: `read` }, style: { width: 360 } } },
    { label: `side · in · author + attachment`, node: { id: `bubble`, component: `MessageBubble`, props: { text: `There you go.`, author: `spezl`, time: `20:21` }, style: { width: 360 }, slots: { attachment: { id: `file`, component: `Chip`, props: { label: `IMG_2041.jpg`, icon: `ui-file`, shape: `rect` } } } } },
  ],
  CallSurface: [
    { label: `state · ringing`, node: { id: `call`, component: `CallSurface`, props: { name: `Assistant`, seed: `agent-assistant`, state: `ringing`, status: `Calling…` }, style: { minHeight: 480 } } },
    { label: `state · talking`, node: { id: `call`, component: `CallSurface`, props: { name: `Assistant`, seed: `agent-assistant`, state: `talking`, activity: `speaking`, level: 0.6, bands: [0.9, 0.5, 1, 0.6], status: `01:24`, caption: `Sure, Friday at eight works. Shall I book it?`, speaker: true }, style: { minHeight: 480 } } },
    { label: `muted · true`, node: { id: `call`, component: `CallSurface`, props: { name: `Assistant`, seed: `agent-assistant`, state: `talking`, status: `03:10`, muted: true }, style: { minHeight: 480 } } },
  ],
  Plan: [
    { label: `state · proposed`, node: { id: `plan`, component: `Plan`, props: { title: `Here's my plan`, description: `I'll use your mouse and keyboard for about two minutes.`, state: `proposed`, approveLabel: `Yes, do it`, steps: [{ title: `Open augustiner-keller.de in Safari`, icon: `globe`, status: `pending` }, { title: `Reserve Fri 20:00 for 4`, icon: `pencil`, status: `pending` }, { title: `Mail the confirmation to Lisa`, icon: `mail`, status: `pending` }] }, style: { maxWidth: 480 } } },
    { label: `state · running`, node: { id: `plan`, component: `Plan`, props: { title: `Working on it · step 2 of 4`, description: `Hands off the mouse for a moment. Esc stops me.`, state: `running`, progress: 0.4, steps: [{ title: `Open augustiner-keller.de in Safari`, icon: `globe`, status: `done` }, { title: `Reserve Fri 20:00 for 4`, icon: `pencil`, status: `running` }, { title: `Mail the confirmation to Lisa`, icon: `mail`, status: `pending` }, { title: `Add it to your calendar`, icon: `calendar`, status: `skipped` }] }, style: { maxWidth: 480 } } },
  ],
  // EXP-1493: the Carousel shows its round-6 counter + thumbnail strip (one thumbnail per page).
  Carousel: [
    { label: `counter · thumbnails`, node: { id: `carousel`, component: `Carousel`, props: { page: 0, indicators: true, counter: true, thumbnails: [`https://example.com/1.jpg`, `https://example.com/2.jpg`, `https://example.com/3.jpg`] } } },
  ],
  QrCode: [
    { label: `example`, node: { id: `qr`, component: `QrCode`, props: { value: `spezl://pair?host=macbook.local&key=Zm9vYmFy`, label: `Scan to pair` } } },
    { label: `level · H · size 120`, node: { id: `qr`, component: `QrCode`, props: { value: `HELLO`, level: `H`, size: 120 } } },
  ],
  QrScanner: [
    { label: `example`, node: { id: `scan`, component: `QrScanner`, props: { hint: `Point at the code on your computer` }, style: { width: 320 } } },
    { label: `disabled · true`, node: { id: `scan`, component: `QrScanner`, props: { hint: `Paused`, disabled: true }, style: { width: 320 } } },
  ],
}

/** #VAPP-284: a Row with children is a foldable parent (the settings nav's
 *  Issues › Labels, Statuses): closed and open, a pressable head (its own
 *  fold button) and a plain one (the head folds), a bound `open`. */
const foldChildren = (selected?: string): NestedNode[] => [
  { id: `labels`, component: `Row`, props: { title: `Labels`, icon: `settings-labels`, density: `compact`, pressable: true, ...(selected === `labels` ? { selected: true } : {}) } },
  { id: `statuses`, component: `Row`, props: { title: `Statuses`, icon: `settings-statuses`, density: `compact`, pressable: true, ...(selected === `statuses` ? { selected: true } : {}) } },
]
const FOLD_ON = { press: { event: { name: `select`, context: { page: `issues` } } }, change: { event: { name: `fold` } } }
const foldRow = (open: unknown, extra: Partial<NestedNode> = {}): NestedNode => ({ id: `issues`, component: `Row`, props: { title: `Issues`, icon: `settings-issues`, density: `compact`, open }, on: FOLD_ON, children: foldChildren(`statuses`), ...extra })
/** #EXP-1629: the Avatar specimen's picture (a 64 px person mark, PNG). */
const AVATAR_PICTURE = `data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAACu0lEQVR42tXY11pTQRSG4XVDGHpUOkRFETuKKOrNgVIUG0UR0kggnQQsWFABu2gAsR25dmZTTsye7DXsyXqe/wLe7yDJmsC1nr9Xe/7grvT+xnX3/uq+jvt5GXdj8xKu7weuq2+jq3/jYv+6sYH1zoG1zsG1C4NZYze/nzf2reMWbrVjaPXc0Ffc2dtfxM7c+Yw7ffdTbh9P3cN9OIm7/x53Yvgdrn14pX1k5fjIsrHR5bbRpbaxpWNjb409eHPU2OvWh7jF1vHFI+OvcIcfvcQBa70ZwFd/aOIFsNabAXz1nonnwFrvmcQAznrP5AKw1rd4F4C1vsX7DFjrmzGAtb7Z9xRY680Avvom3xNgrW/yYwBnfaP/MbDWNwZEgGp9Nrv5v6nV5wKU6vPQd0+VviEwD87rxZToG4IYoEMvRtfXB+dAl16MqN8K2MtPrUyAbX39VAY06sUo+joMoH/f0wNs6+tCaaD/WhEDKPpaDKD/1lIDCPra0CzQLwVygH19TRgDyHcOMYCirwmngH6lkQPs680A4o1JDKDoD04ngX4hUwMI+p0A4n2vS39gJgFKXieUAIreCFD1ttKiNwNUvQyd1++PxEHtu9Zh/XaA4lf5nn5qd+vdkRjo/U+BqHdHY8Ba745GgbW+OhYF1vrqWARY66tEAF99VXwGlOstfwQU6s0AJXobpwRdXxmfBrqeeE5T9JUJDNCqF7Otr0iEQbtezJ6+IikCdOvFbOhzAcWh324oSF+eDEHx6MUK0penMKCY9GLy+rLUFDjwfV94gKx+K6CY9GKS+rLZIBR0KTgWIKkvxYAi1JsNEvrSdAAUXmlqJ6N3YYD8hex0gITelfZDcerFLPWuDAbIvU50BeTX78v4QPJtpSXAUm8GyLwM9QRY6UvmvCD5rtUSYKnfCbB8lWsLyKsvmff+A9vY5mTot7phAAAAAElFTkSuQmCC`
/** VAPP-163: cases a specimen shows AFTER its generated ones (past the cap),
 *  each pinning a cross-renderer drift the spezl drafts found: a styled
 *  Heading on a band (the author's style beats the Heading recipe), the
 *  non-Lucide glyphs (no host mapping on web), a bar of icon tabs (labels
 *  centred under their icons), a label-less search field (the placeholder
 *  centred in the box). */
const SPECIMEN_EXTRA: Record<string, { label: string; node: NestedNode }[]> = {
  // #EXP-1760: the open grids, in a box tall enough for them (an extra:
  // the wrapper keeps its own child).
  IconPicker: [
    { label: `open · the board set, allowNone`, node: { id: `icon-open-box`, component: `Box`, style: { height: 400 }, children: [{ id: `icon-open`, component: `IconPicker`, props: { value: `rocket`, color: `#3b82f6`, allowNone: true, open: true } }] } },
  ],
  ColorPicker: [
    { label: `open · the label palette`, node: { id: `color-open-box`, component: `Box`, style: { height: 180 }, children: [{ id: `color-open`, component: `ColorPicker`, props: { value: `#3b82f6`, open: true } }] } },
  ],
  // #EXP-1629: a status picker's rows: each glyph in its own tint
  // (`iconColor`), the chosen one checked.
  Menu: [
    { label: `items · icon + iconColor`, node: { id: `status_menu`, component: `Menu`, props: { align: `end`, items: [
      { label: `Backlog`, value: `backlog`, kind: `checkbox`, icon: `status-backlog`, iconColor: `$color.mutedForeground` },
      { label: `In Progress`, value: `in_progress`, kind: `checkbox`, icon: `status-in-progress`, iconColor: `$color.warning`, checked: true },
      { label: `Done`, value: `done`, kind: `checkbox`, icon: `status-done`, iconColor: `$color.success` },
      { label: `Cancelled`, value: `cancelled`, kind: `checkbox`, icon: `status-cancelled`, iconColor: `$color.destructive` },
    ] }, children: [{ id: `status_trigger`, component: `Button`, props: { label: `In Progress`, icon: `status-in-progress`, variant: `outline` } }] } },
    // #EXP-1629: the IDE session composer's "+" menu: Model › with its
    // current value (`detail`), the Account row's health badge and usage card
    // (`preview`, beside the row while hovered or highlighted), Issue › rows
    // led by a status glyph and their mono `identifier`.
    {
      label: `items · detail + identifier + preview`,
      node: {
        id: `composer_menu`,
        component: `Menu`,
        props: {
          label: `Options`,
          items: [
            { label: `Model`, kind: `submenu`, detail: `Opus`, items: [{ label: `Opus`, value: `opus`, kind: `checkbox`, checked: true }, { label: `Sonnet`, value: `sonnet`, kind: `checkbox` }] },
            {
              label: `ada@example.com`,
              value: `account`,
              icon: `brand-claude`,
              detail: `Healthy`,
              preview: { root: { id: `usage`, component: `Stack`, props: { gap: `xs` }, children: [{ id: `usage_title`, component: `Text`, props: { text: `5-hour limit`, variant: `caption` } }, { id: `usage_bar`, component: `Progress`, props: { value: 42, label: `42%` } }] } },
            },
            { label: `Issue`, kind: `submenu`, items: [{ label: `Fix the login redirect`, value: `EXP-12`, identifier: `EXP-12`, icon: `status-in-progress` }, { label: `Ship the composer menu`, value: `EXP-14`, identifier: `EXP-14`, icon: `status-done` }] },
          ],
        },
        children: [{ id: `composer_plus`, component: `Button`, props: { icon: `ui-add`, label: `Options`, variant: `ghost`, size: `icon` } }],
      },
    },
  ],
  // #EXP-1629 / #EXP-1754 (fixtures/avatar.json): no seed = the name keys the
  // hue, nothing at all = the muted fallback, an
  // email's initials (its local part), a picture in its hairline ring.
  Avatar: [
    { label: `seed · none`, node: { id: `by_name`, component: `Avatar`, props: { name: `Jane Reporter`, size: `default` } } },
    { label: `name · empty`, node: { id: `muted`, component: `Avatar`, props: { name: ``, size: `default` } } },
    { label: `name · an email`, node: { id: `email`, component: `Avatar`, props: { name: `alex.smith@x.dev`, seed: `demo-jonas`, size: `default` } } },
    { label: `src · a picture`, node: { id: `picture`, component: `Avatar`, props: { name: `Jane Doe`, seed: `demo-mira`, size: `xl`, src: AVATAR_PICTURE } } },
  ],
  // #EDE-65 R: the IDE's composer row: the "+" options Menu in `leading`,
  // the context Ring in `trailing` (before the send disc).
  Composer: [{
    label: `slots · leading + trailing`,
    node: {
      id: `slotted`,
      component: `Composer`,
      props: { placeholder: `Ask anything` },
      slots: {
        leading: { id: `options`, component: `Menu`, props: { items: [{ label: `Attach a file`, value: `attach`, icon: `ui-attach` }, { label: `Plan first`, value: `plan` }] }, children: [{ id: `plus`, component: `Button`, props: { icon: `ui-add`, label: `Options`, variant: `ghost`, size: `icon` } }] },
        trailing: { id: `context`, component: `Ring`, props: { value: 0.42, size: `sm` } },
      },
    },
  }, {
    // #EXP-1762: THE session composer, flat: "+" left, the field growing
    // upward, the context Ring + send right, all on one bottom-aligned row.
    label: `layout · flat`,
    node: {
      id: `flat`,
      component: `Composer`,
      props: { placeholder: `Reply…`, layout: `flat` },
      slots: {
        leading: { id: `flat_options`, component: `Menu`, props: { items: [{ label: `Attach a file`, value: `attach`, icon: `ui-attach` }, { label: `Plan first`, value: `plan` }] }, children: [{ id: `flat_plus`, component: `Button`, props: { icon: `ui-add`, label: `Options`, variant: `ghost`, size: `icon` } }] },
        trailing: { id: `flat_context`, component: `Ring`, props: { value: 0.42, size: `sm` } },
      },
    },
  }, {
    // #EXP-1762: the IDE's attachment strip INSIDE the card, over the field
    // (a wrapping row of chips); it sizes the composer.
    label: `slots · above`,
    node: {
      id: `above`,
      component: `Composer`,
      props: { placeholder: `Reply…`, layout: `flat` },
      slots: {
        above: {
          id: `above_strip`,
          component: `Stack`,
          props: { direction: `horizontal`, wrap: true, gap: `xs` },
          children: [
            { id: `above_file`, component: `Chip`, props: { label: `screenshot.png`, icon: `ui-attach`, removable: true } },
            { id: `above_team`, component: `Chip`, props: { label: `Design`, removable: true } },
          ],
        },
        leading: { id: `above_plus`, component: `Button`, props: { icon: `ui-add`, label: `Options`, variant: `ghost`, size: `icon` } },
      },
    },
  }],
  // FEED-205: income vs expenses per month in EUR (ticks, values, tooltip),
  // then the same rows of a query in compact euros.
  Chart: [
    { label: `valueFormat · currency EUR`, node: { id: `income`, component: `Chart`, props: { ...INCOME_EXPENSES, showValues: true, valueFormat: { type: `currency`, currency: `EUR`, decimals: 0 } } } },
    {
      label: `rows · valueFormat compact`,
      node: {
        id: `income_rows`,
        component: `Chart`,
        props: {
          kind: `line`,
          rows: INCOME_EXPENSES.categories.map((month, i) => ({ month, income: INCOME_EXPENSES.series[0].values[i] * 10, expenses: INCOME_EXPENSES.series[1].values[i] * 10 })),
          categoryField: `month`,
          series: [{ name: `Income`, field: `income`, tone: `success` }, { name: `Expenses`, field: `expenses`, tone: `danger` }],
          valueFormat: { type: `compact`, currency: `EUR` },
        },
      },
    },
  ],
  // #EXP-1629: the settings pane's picker rows (the IDE's glass picker row):
  // a searchable timezone, a short list and a multiple pick (the IDE's
  // multi picker), inside a Group.
  Select: [{
    label: `triggerVariant · row (in a Group)`,
    node: {
      id: `prefs`,
      component: `Group`,
      children: [
        { id: `timezone`, component: `Select`, props: { label: `Timezone`, name: `timezone`, triggerVariant: `row`, searchable: true, value: `Europe/Vienna`, options: [`Europe/Berlin`, `Europe/London`, `Europe/Vienna`, `UTC`].map((zone) => ({ label: zone, value: zone })) } },
        { id: `week`, component: `Select`, props: { label: `Week starts on`, name: `week`, triggerVariant: `row`, value: `monday`, options: [{ label: `Monday`, value: `monday` }, { label: `Sunday`, value: `sunday` }] } },
        { id: `apps`, component: `Select`, props: { label: `Apps`, name: `apps`, triggerVariant: `row`, multiple: true, searchable: true, value: `zoom`, options: [{ label: `Slack`, value: `slack` }, { label: `Zoom`, value: `zoom` }] } },
      ],
    },
  }, {
    // #EXP-1629: a picker row with its hint under the label (the IDE's
    // `glass_picker_row(label, Some(description), …)`: Settings → Voice).
    label: `triggerVariant · row + description`,
    node: {
      id: `voice_prefs`,
      component: `Group`,
      children: [
        { id: `language`, component: `Select`, props: { label: `Language`, description: `Spoken replies and dictation use it`, name: `language`, triggerVariant: `row`, value: `en`, options: [{ label: `English`, value: `en` }, { label: `Deutsch`, value: `de` }] } },
      ],
    },
  }, {
    // #EXP-1629: the IDE's Branch picker row: the repo's default reads
    // `main · default` in the popup (option `trailing`), the trigger only
    // `main`; opening fires `open` (the lazy branch fetch).
    label: `triggerVariant · row + option trailing`,
    node: {
      id: `board_prefs`,
      component: `Group`,
      children: [
        { id: `branch`, component: `Select`, props: { label: `Branch`, name: `branch`, triggerVariant: `row`, value: `main`, options: [{ label: `main`, value: `main`, trailing: `· default` }, { label: `develop`, value: `develop` }] }, on: { open: { event: { name: `loadBranches` } } } },
      ],
    },
  }, {
    // #EXP-1764: the IDE's Account row: the chosen login's agent mark
    // (`brand-claude` in its own orange) before the email (`triggerIcon`).
    label: `triggerVariant · row + triggerIcon`,
    node: {
      id: `agent_prefs`,
      component: `Group`,
      children: [
        { id: `account`, component: `Select`, props: { label: `Account`, name: `account`, triggerVariant: `row`, triggerIcon: true, value: `claude`, options: [{ label: `ada@example.com`, value: `claude`, icon: `brand-claude` }, { label: `ada@example.org`, value: `codex`, icon: `brand-codex` }] } },
      ],
    },
  }, {
    // #EXP-1765: the IDE's per-repository branch picker: a `pill` holding
    // the branch in mono; the popup searches ("Search branches..."), lists
    // the branches in mono with the repo default's muted `default`, and
    // shows the Skeleton rows while `loading` (the lazy fetch on `open`).
    label: `triggerVariant · pill + mono options + loading`,
    node: {
      id: `repo_branches`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, align: `center` },
      children: [
        { id: `repo_branch`, component: `Select`, props: { label: `Default branch`, name: `repoBranch`, triggerVariant: `pill`, searchable: true, searchPlaceholder: `Search branches...`, value: `main`, options: [{ label: `main`, value: `main`, mono: true, trailing: `default` }, { label: `develop`, value: `develop`, mono: true }] }, on: { open: { event: { name: `loadBranches` } } } },
        { id: `repo_branch_loading`, component: `Select`, props: { label: `Default branch (loading)`, name: `repoBranchLoading`, triggerVariant: `pill`, searchable: true, searchPlaceholder: `Search branches...`, value: `develop`, loading: true, options: [{ label: `develop`, value: `develop`, mono: true }] } },
        { id: `repo_branch_busy`, component: `Select`, props: { label: `Default branch (busy)`, name: `repoBranchBusy`, triggerVariant: `pill`, value: `release/2026-11`, disabled: true, options: [{ label: `release/2026-11`, value: `release/2026-11`, mono: true }] } },
      ],
    },
  }, {
    // #EXP-1629: the IDE's device picker: each option's glyph, an offline
    // one's muted `description`; the popup fits its options, at least 170.
    label: `triggerVariant · row + option icon, description`,
    node: {
      id: `agent_prefs`,
      component: `Group`,
      children: [
        { id: `device`, component: `Select`, props: { label: `Device`, name: `device`, triggerVariant: `row`, popupMinWidth: 170, value: `mint`, options: [{ label: `mint`, value: `mint`, icon: `monitor` }, { label: `build-server`, value: `build`, icon: `server` }, { label: `macbook`, value: `macbook`, icon: `laptop`, description: `Offline` }] } },
      ],
    },
  }, {
    // #EXP-1629: web's PR trigger: the picked option's glyph leads, in its
    // tint; an option with no glyph leaves the value alone.
    label: `triggerVariant · button + the picked option's icon`,
    node: {
      id: `pr_targets`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, align: `center` },
      children: [
        { id: `pr_opened_button`, component: `Select`, props: { label: `When a PR opens`, name: `prOpenedButton`, triggerVariant: `button`, value: `in_review`, options: [{ label: `In Review`, value: `in_review`, icon: `status-in-review`, iconColor: `$color.success` }, { label: `Done`, value: `done`, icon: `status-done`, iconColor: `$color.info` }, { label: `Do nothing`, value: `none` }] }, style: { width: 176 } },
        { id: `pr_merged_button`, component: `Select`, props: { label: `When a PR merges`, name: `prMergedButton`, triggerVariant: `button`, value: `none`, options: [{ label: `Done`, value: `done`, icon: `status-done`, iconColor: `$color.info` }, { label: `Do nothing`, value: `none` }] }, style: { width: 176 } },
      ],
    },
  }, {
    // #EXP-1778: App detail's Accounts picker: an outline small button (the
    // label names it) opening the pointer menu, the shown option checked.
    label: `triggerVariant · button`,
    node: {
      id: `accounts`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `sm`, align: `center` },
      children: [
        { id: `upstream`, component: `Select`, props: { label: `github`, name: `upstream`, triggerVariant: `button`, value: `s1`, options: [{ label: `Not set up`, value: `none` }, { label: `GitHub`, value: `s1` }, { label: `GitHub (work account)`, value: `s2` }] }, style: { width: 144 } },
        { id: `upstream_busy`, component: `Select`, props: { label: `linear`, name: `upstreamBusy`, triggerVariant: `button`, value: `none`, disabled: true, options: [{ label: `Not set up`, value: `none` }] }, style: { width: 144 } },
      ],
    },
  }, {
    // #EXP-1629: a locked row (the IDE's Effort row under ultracode): the
    // value and the chevron at 0.35 (the 0.7 value tone halved).
    label: `triggerVariant · row + description, disabled`,
    node: {
      id: `agent_effort`,
      component: `Group`,
      children: [
        { id: `effort_locked`, component: `Select`, props: { label: `Effort`, name: `effort`, triggerVariant: `row`, description: `ultracode sets effort`, disabled: true, value: `max`, options: [{ label: `High`, value: `high` }, { label: `Max`, value: `max` }] } },
      ],
    },
  }, {
    // #EXP-1629: a status picker: each option's glyph in its own tint
    // (`iconColor`), the chosen one's leading the row's value.
    label: `option iconColor + triggerIcon`,
    node: {
      id: `pr_automation`,
      component: `Group`,
      children: [
        { id: `pr_opened`, component: `Select`, props: { label: `When a PR opens`, name: `prOpened`, triggerVariant: `row`, triggerIcon: true, popupMinWidth: 170, value: `in_review`, options: [
          { label: `Backlog`, value: `backlog`, icon: `status-backlog`, iconColor: `$color.mutedForeground` },
          { label: `In Progress`, value: `in_progress`, icon: `status-in-progress`, iconColor: `$color.warning` },
          { label: `In Review`, value: `in_review`, icon: `status-in-review`, iconColor: `$color.success` },
          { label: `Cancelled`, value: `cancelled`, icon: `status-cancelled`, iconColor: `$color.destructive` },
        ] } },
      ],
    },
  }, {
    // #EXP-1629: the IDE's skills multi picker: two adjacent picks of three,
    // one bordered block in the open popup (`joinTop` / `joinBottom`).
    label: `multiple · searchable · row + option icon, description`,
    node: {
      id: `agent_skills`,
      component: `Group`,
      children: [
        { id: `skills`, component: `Select`, props: { label: `Skills`, name: `skills`, triggerVariant: `row`, multiple: true, searchable: true, value: `deploy,review`, options: [{ label: `deploy`, value: `deploy`, icon: `ui-docs`, description: `Ship a build to staging` }, { label: `review`, value: `review`, icon: `ui-docs`, description: `Review the open pull request` }, { label: `triage`, value: `triage`, icon: `ui-docs`, description: `Sort the inbox by priority` }] } },
      ],
    },
  }],
  // #FEED-201: a dashboard tile and a card that open a detail screen (a
  // button with the Row hover fill; at rest they look like the others).
  StatTile: [{ label: `pressable · true`, node: { id: `tile_press`, component: `StatTile`, props: { label: `Income`, value: `4,200`, delta: `+8 %`, tone: `success`, pressable: true } } }],
  Card: [{ label: `pressable · true`, node: { id: `card_press`, component: `Card`, props: { title: `Expenses`, description: `12 this month`, pressable: true }, children: [{ id: `card_press_t`, component: `Text`, props: { text: `Rent, travel, software`, variant: `body` } }] } }],
  Window: [{ label: `no title`, node: { id: `untitled`, component: `Window`, props: {}, style: { height: 160 }, children: [windowBody(`untitled_body`)] } }],
  // Round 8 (docs/round-8-contract.md): the download row, a bar with only
  // its trailing text, the collapsed Stepper, the mixed Checkbox.
  Progress: [
    { label: `valueText · detail`, node: { id: `download`, component: `Progress`, props: { value: 41, label: `Downloading`, valueText: `41 MB/s · 12 min`, detail: `3 of 8 files` } } },
    { label: `valueText only`, node: { id: `sync`, component: `Progress`, props: { value: 80, valueText: `80%` } } },
  ],
  Stepper: [{ label: `collapse · always`, node: { id: `wizard`, component: `Stepper`, props: { steps: [{ label: `Account` }, { label: `Team` }, { label: `Done` }], current: 1, collapse: `always` } } }],
  // The mixed parent of two packs, one checked and one not.
  Checkbox: [{
    label: `indeterminate · a parent of two`,
    node: {
      id: `packs`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: `start` },
      children: [
        { id: `all`, component: `Checkbox`, props: { label: `All packs`, name: `all`, indeterminate: true } },
        {
          id: `pack_list`,
          component: `Stack`,
          props: { direction: `vertical`, gap: `xs`, align: `start` },
          style: { paddingInlineStart: `$spacing.lg` },
          children: [
            { id: `icons`, component: `Checkbox`, props: { label: `Icons`, name: `icons`, checked: true } },
            { id: `fonts`, component: `Checkbox`, props: { label: `Fonts`, name: `fonts`, checked: false } },
          ],
        },
      ],
    },
  }],
  // VAPP-280: a settings page's group (a Switch that stays inline on phones,
  // a Select with a description, a Slider), then the stacked phone layout at
  // any width (`stackBelow: xl` is above the specimen's 720).
  SettingsRow: [
    { label: `example · a settings group`, node: settingsGroup() },
    { label: `stackBelow · xl (stacked)`, node: { id: `group`, component: `Group`, children: [settingsSelectRow(`xl`)] } },
  ],
  // #EXP-1629: the shortcut recorder in its five states (the IDE's Push to
  // talk row: a binding, none, recording, a held modifier, an inline error).
  // #EDE-65: Embed in the chat's three uses: an agent's result card (its own
  // data), a question card that replies, and a bad surface's error box.
  Embed: [
    { label: `result card · own data`, node: { id: `result`, component: `Embed`, props: { surface: { root: { id: `stack`, component: `Stack`, props: { gap: `sm` }, children: [{ id: `row`, component: `Row`, props: { title: `Tests`, subtitle: { "@path": `/suite` } }, slots: { trailing: { id: `badge`, component: `Badge`, props: { text: `Green`, variant: `secondary` } } } }, { id: `stats`, component: `Grid`, props: { columns: 2, gap: `sm` }, children: [{ id: `passed`, component: `StatTile`, props: { label: `Passed`, value: { "@path": `/passed` } } }, { id: `took`, component: `StatTile`, props: { label: `Took`, value: `4m 12s` } }] }] }, data: { suite: `web unit ×4 shards`, passed: `2,730` } } } } },
    { label: `question card · reply`, node: { id: `question`, component: `Embed`, props: { surface: { root: { id: `stack`, component: `Stack`, props: { gap: `sm` }, children: [{ id: `q`, component: `Text`, props: { text: `Ship the Embed PR now?` } }, { id: `actions`, component: `Toolbar`, children: [{ id: `no`, component: `Button`, props: { label: `Not yet`, variant: `outline` }, on: { press: { event: { name: `reply`, context: { text: `Not yet` } } } } }, { id: `yes`, component: `Button`, props: { label: `Ship it` }, on: { press: { event: { name: `reply`, context: { text: `Ship it` } } } } }] }] } } }, on: { action: { event: { name: `card` } } } } },
    { label: `bad surface · error box`, node: { id: `broken`, component: `Embed`, props: { surface: { root: { id: `x` } } } } },
  ],
  KeyRecorder: [
    { label: `value`, node: { id: `ptt`, component: `KeyRecorder`, props: { label: `Push to talk`, value: [`Ctrl`, `Space`] } } },
    { label: `empty`, node: { id: `ptt-none`, component: `KeyRecorder`, props: { label: `Push to talk`, value: [] } } },
    { label: `recording`, node: { id: `ptt-recording`, component: `KeyRecorder`, props: { label: `Push to talk`, value: [`Ctrl`, `Space`], recording: true } } },
    { label: `recording · held modifier`, node: { id: `ptt-held`, component: `KeyRecorder`, props: { label: `Push to talk`, value: [`Ctrl`, `Space`], recording: true, pending: [`Ctrl`] } } },
    { label: `error`, node: { id: `ptt-error`, component: `KeyRecorder`, props: { label: `Push to talk`, recording: true, error: `Hold Ctrl, Alt, Shift or Super with that key` } } },
  ],
  // #VAPP-284: a foldable parent (the settings nav's groups), closed and open.
  Row: [
    { label: `children · closed`, node: { id: `device`, component: `Row`, props: { title: `This device`, icon: `ui-device`, density: `compact`, pressable: true, open: false }, children: [{ id: `tools`, component: `Row`, props: { title: `Tools`, icon: `settings-tools`, density: `compact`, pressable: true } }] } },
    { label: `children · open`, node: { id: `issues`, component: `Row`, props: { title: `Issues`, icon: `settings-issues`, density: `compact`, pressable: true, open: true }, children: foldChildren(`statuses`) } },
  ],
  // VAPP-276: a bottom panel's buttons open popovers ABOVE them by `anchor`
  // (no trigger of their own): top-start at the start button, top-end at
  // the clock at the panel's end (the eDE start menu and calendar applet).
  Popover: [{
    label: `anchor · top-start + top-end (a bottom panel)`,
    node: {
      id: `desk`,
      component: `Box`,
      style: { position: `relative`, width: `100%`, height: 300, borderRadius: `$radius.lg`, backgroundColor: `$color.muted`, overflow: `hidden` },
      children: [
        {
          id: `panel`,
          component: `Box`,
          style: { position: `absolute`, left: 0, right: 0, bottom: 0, display: `flex`, flexDirection: `row`, justifyContent: `space-between`, alignItems: `center`, padding: `$spacing.xs`, backgroundColor: `$color.card` },
          children: [button(`start`, `Start`, `ghost`), button(`clock`, `12:00`, `ghost`)],
        },
        { id: `start_menu`, component: `Popover`, props: { anchor: `start`, side: `top`, align: `start`, open: true }, children: [text(`files`, `Files`), text(`terminal`, `Terminal`), text(`settings`, `Settings`)] },
        { id: `calendar`, component: `Popover`, props: { anchor: `clock`, side: `top`, align: `end`, open: true }, children: [{ id: `month`, component: `Heading`, props: { text: `November 2026`, level: `h4` } }, text(`today`, `Friday, 6`)] },
      ],
    },
  }, {
    // `matchAnchorWidth`: the IDE's `/` typeahead floats above the composer
    // card at the card's full width (anchor = the card, top-start).
    label: `matchAnchorWidth`,
    node: {
      id: `chat`,
      component: `Box`,
      style: { position: `relative`, width: `100%`, height: 260, borderRadius: `$radius.lg`, backgroundColor: `$color.muted`, overflow: `hidden` },
      children: [
        {
          id: `composer_card`,
          component: `Box`,
          style: { position: `absolute`, left: 16, right: 16, bottom: 16, height: 72, padding: `$spacing.md`, borderRadius: `$radius.lg`, borderWidth: `$border.hairline`, borderColor: `$color.border`, backgroundColor: `$color.card` },
          children: [text(`draft`, `/rev`)],
        },
        { id: `commands`, component: `Popover`, props: { anchor: `composer_card`, side: `top`, align: `start`, matchAnchorWidth: true, open: true }, children: [text(`review`, `/review`), text(`revert`, `/revert`)] },
      ],
    },
  }],
  // EXP-1556: a vertical Separator between two items of a row (alone in a
  // column it has no height to draw).
  Separator: [{
    label: `orientation · vertical`,
    node: { id: `bar`, component: `Stack`, props: { direction: `horizontal`, gap: `md`, align: `center` }, children: [text(`a`, `Inbox`), { id: `separator`, component: `Separator`, props: { orientation: `vertical` }, style: { height: 20 } }, text(`b`, `Drafts`)] },
  }],
  // #VAPP-277: the rail Sidebar, expanded and collapsed (icon tiles,
  // tooltips, the counts as badges, the Section's header → a rule).
  Sidebar: [false, true].map((collapsed) => ({
    label: collapsed ? `rail · collapsed` : `rail · expanded`,
    node: {
      id: `sidebar`,
      component: `Sidebar`,
      props: { rail: true, collapsed, collapseBelow: `never` },
      slots: {
        sidebar: { id: `nav`, component: `Stack`, props: { gap: `xs` }, children: [
          { id: `n1`, component: `Row`, props: { title: `Inbox`, icon: `nav-inbox`, meta: `3`, selected: true, pressable: true } },
          { id: `n2`, component: `Row`, props: { title: `Issues`, icon: `nav-issues`, pressable: true } },
          { id: `more`, component: `Section`, props: { title: `More` }, children: [
            { id: `n3`, component: `Row`, props: { title: `Search`, icon: `nav-search`, meta: `12`, pressable: true } },
          ] },
        ] },
      },
      children: [text(`main`, `Pick a board on the left.`)],
    },
  })),
  // #VAPP-286: the volume row (icon, track, end label, no value above).
  Slider: [{
    label: `icon · endLabel · showValue false`,
    node: { id: `volume`, component: `Slider`, props: { min: 0, max: 100, value: 34, icon: `media-volume-up`, endLabel: `34%`, showValue: false }, style: { width: 320 } },
  }],
  // #VAPP-282: the month grid in place (the calendar applet): today ringed,
  // the selected day, event dots, Monday first.
  DatePicker: [{
    label: `inline · today · marks · firstDayOfWeek 1`,
    node: { id: `month`, component: `DatePicker`, props: { name: `day`, inline: true, value: `2026-10-14`, today: `2026-10-08`, firstDayOfWeek: 1, marks: [{ date: `2026-10-02`, tone: `primary` }, { date: `2026-10-14`, tone: `success` }, { date: `2026-10-20`, tone: `warning` }, { date: `2026-10-29`, tone: `danger` }] } },
  }],
  Heading: [{
    label: `style · on a band`,
    node: { id: `band`, component: `Box`, style: { backgroundColor: `$color.primary`, padding: `$spacing.lg`, borderRadius: `$radius.lg` }, children: [
      { id: `heading`, component: `Heading`, props: { text: `Welcome`, level: `h4` }, style: { color: `$color.primaryForeground`, fontSize: `$type.size.xl3`, lineHeight: `$type.lineHeight.xl3` } },
    ] },
  }],
  Icon: [{
    label: `name · non-Lucide`,
    node: { id: `marks`, component: `Stack`, props: { direction: `horizontal`, gap: `md` }, children: [`os-apple`, `os-windows`, `os-linux`, `progress-2-4`, `brand-exponential`].map((name) => ({ id: name, component: `Icon`, props: { name, size: `lg` } })) },
  }],
  // VAPP-162: the navigation rail (a vertical bar) with counts.
  Segmented: [{
    label: `variant · bar · icons`,
    node: { id: `tabs`, component: `Segmented`, props: { variant: `bar`, value: `chats`, items: [{ label: `Chats`, value: `chats`, icon: `message-circle` }, { label: `Contacts`, value: `contacts`, icon: `users` }, { label: `Settings`, value: `settings`, icon: `settings` }] }, style: { width: 360 } },
  }, {
    label: `orientation · vertical · bar (the rail) · counts`,
    node: { id: `rail`, component: `Segmented`, props: { variant: `bar`, orientation: `vertical`, value: `chats`, items: [{ label: `Chats`, value: `chats`, icon: `message-circle`, count: 3 }, { label: `Voices`, value: `voices`, icon: `mic` }, { label: `Library`, value: `library`, icon: `layers`, count: 12 }, { label: `Friends`, value: `friends`, icon: `users` }] } },
  }],
  VoiceWave: [`sm`, `md`, `lg`, `xl`].map((size) => ({
    label: `variant · bars · ${size}`,
    node: { id: `bars_${size}`, component: `VoiceWave`, props: { variant: `bars`, size, activity: `speaking`, level: 0.7, bands: [0.9, 0.5, 1, 0.6], label: `Voice` } },
  })),
  Button: [{
    label: `hold · true`,
    node: { id: `talk`, component: `Button`, props: { label: `Hold to talk`, icon: `call-mute`, hold: true, holdingLabel: `Holding…` } },
  }, {
    // #EXP-1629: the IDE's composer "+" tool (a muted ghost web_icon_xs)
    // next to the quick replies' ✕.
    label: `size · icon-xs, tone · muted`,
    node: { id: `icon-xs`, component: `Stack`, props: { direction: `horizontal`, gap: `sm`, align: `center` }, children: [
      { id: `tool`, component: `Button`, props: { label: `Add`, icon: `ui-add`, variant: `ghost`, size: `icon-xs`, tone: `muted` } },
      { id: `dismiss`, component: `Button`, props: { label: `Dismiss`, icon: `ui-close`, variant: `ghost`, size: `icon-xs` } },
    ] },
  }],
  // VAPP-162: a conversation (in, out, the three delivery states).
  MessageBubble: [{
    label: `a conversation`,
    node: { id: `chat`, component: `Stack`, props: { gap: `sm` }, style: { width: 360 }, children: [
      { id: `m1`, component: `MessageBubble`, props: { text: `Is the printer fixed?`, side: `out`, time: `18:00`, status: `read` } },
      { id: `m2`, component: `MessageBubble`, props: { text: `Yes, driver's updated. Try again.`, time: `18:02` } },
      { id: `m3`, component: `MessageBubble`, props: { text: `Send me the photo from the Wiesn I put on the desktop.`, side: `out`, time: `20:20`, status: `sent` } },
      { id: `m4`, component: `MessageBubble`, props: { text: `On its way…`, side: `out`, time: `20:21`, status: `sending` } },
    ] },
  }],
  // VAPP-162: the serif family token on a node style.
  Text: [{
    label: `fontFamily · serif`,
    node: { id: `serif`, component: `Text`, props: { text: `The quick brown fox jumps over the lazy dog.`, variant: `lead` }, style: { fontFamily: `$type.family.serif` } },
  }, {
    // #EXP-1778: App detail's webhook URL and shown-once secret: one mono
    // line each, the middle cut (start…end) at the box's width.
    label: `truncate · middle`,
    node: {
      id: `hook`,
      component: `Stack`,
      props: { gap: `xs` },
      style: { maxWidth: 240 },
      children: [
        { id: `hook_url`, component: `Text`, props: { text: `https://app.exponential.at/api/apps/hooks/0f6a3c1e-7d2b-4a8e-9c51-2be4d7a1f093/github-push`, variant: `caption`, truncate: `middle` }, style: { fontFamily: `$type.family.mono` } },
        { id: `hook_secret`, component: `Text`, props: { text: `whsec_9f8e7d6c5b4a39281706f5e4d3c2b1a0ffeeddccbbaa`, truncate: `middle` }, style: { fontFamily: `$type.family.mono` } },
        { id: `hook_end`, component: `Text`, props: { text: `truncate · end keeps the start of a long line and cuts its end`, truncate: `end` } },
      ],
    },
  }],
  // EXP-1548: a table wider than the page scrolls sideways; cells wrap.
  Table: [{
    label: `wide · scrolls`,
    node: { id: `wide`, component: `Table`, props: wideTableProps() },
  }, {
    label: `totals · dd.MM.yy · EUR`,
    node: { id: `totals`, component: `Table`, props: totalsTableProps() },
  }],
  // FEED-202: the Autotax Expenses screen: column objects, search, the
  // receipt_status filter and the totals row.
  RecordTable: [{
    label: `Autotax Expenses · search · filter · totals`,
    node: { id: `expenses`, component: `RecordTable`, props: expensesRecordTableProps() },
  }],
  // VAPP-279: the strip (the IDE's work tabs): icons, the active one, the
  // close glyph on hover / always, the +, and a narrow strip folding into `+N`.
  Tabs: [{
    label: `variant · strip · icons · closable hover · addable`,
    node: { id: `strip`, component: `Tabs`, props: { variant: `strip`, value: `notes`, closable: `hover`, addable: true, tabs: [{ label: `Inbox`, value: `inbox`, icon: `nav-inbox` }, { label: `Release notes`, value: `notes`, icon: `nav-changelog` }, { label: `Board`, value: `board`, icon: `nav-boards` }] } },
  }, {
    label: `variant · strip · closable always`,
    node: { id: `strip_close`, component: `Tabs`, props: { variant: `strip`, value: `a`, closable: `always`, tabs: [{ label: `ExponentialOS`, value: `a`, icon: `status-in-progress` }, { label: `google keywords`, value: `b`, icon: `action-chat` }] } },
  }, {
    label: `variant · strip · overflow (+N)`,
    node: { id: `strip_narrow`, component: `Tabs`, props: { variant: `strip`, value: `t4`, closable: `hover`, addable: true, tabs: [`Inbox`, `Release notes`, `Calendar`, `Settings`, `Search`, `Drafts`].map((label, i) => ({ label, value: `t${i}` })) }, style: { width: 320 } },
  }, {
    // FEED-200: panels that page by swipe on touch (the thumb at rest).
    label: `swipeable · true · fill`,
    node: { id: `swipe`, component: `Tabs`, props: { swipeable: true, fill: true, value: `activity`, tabs: [`Dashboard`, `Activity`, `Income`, `Expenses`].map((label) => ({ label, value: label.toLowerCase() })) }, style: { maxWidth: 360 }, children: [`Dashboard`, `Activity`, `Income`, `Expenses`].map((label) => ({ id: `swipe_${label.toLowerCase()}`, component: `Text`, props: { text: `The ${label.toLowerCase()} panel.`, variant: `body` } })) },
  }],
  Input: [{
    label: `label · none`,
    node: { id: `search`, component: `Input`, props: { name: `q`, label: ``, type: `search`, placeholder: `Search` } },
  }, {
    // #EXP-1629: the settings pane's text rows (the IDE's glass input row):
    // an empty one showing its placeholder and one with a value, in a Group.
    label: `variant · row (in a Group)`,
    node: {
      id: `voice`,
      component: `Group`,
      children: [
        { id: `wake`, component: `Input`, props: { label: `Wake phrase`, name: `wake`, variant: `row`, placeholder: `Hey Exponential` } },
        { id: `cli`, component: `Input`, props: { label: `CLI path`, name: `cli`, variant: `row`, value: `/usr/local/bin/claude` } },
      ],
    },
  }, {
    // #EXP-1629: a name typed inline in a Group row (the agent and skill
    // pages): `bare`, and `code` after the row's own glyph.
    label: `variant · bare, code (in a Group)`,
    node: {
      id: `identity`,
      component: `Group`,
      children: [
        { id: `agent`, component: `Box`, style: { display: `flex`, flexDirection: `row`, alignItems: `center`, gap: `$spacing.sm`, paddingHorizontal: `$spacing.lg`, paddingVertical: `$spacing.md` }, children: [
          { id: `agent-name`, component: `Input`, props: { label: `Name`, name: `agent`, variant: `bare`, value: `Triage` }, style: { flexGrow: 1, minWidth: 0 } },
        ] },
        { id: `skill`, component: `Box`, style: { display: `flex`, flexDirection: `row`, alignItems: `center`, gap: `$spacing.md`, paddingHorizontal: `$spacing.lg`, paddingVertical: `$spacing.md` }, children: [
          { id: `skill-icon`, component: `Icon`, props: { name: `book-open` } },
          { id: `skill-name`, component: `Input`, props: { label: `Name`, name: `skill`, variant: `code`, placeholder: `skill-name` }, style: { flexGrow: 1, minWidth: 0 } },
        ] },
      ],
    },
  }, {
    // #EXP-1629: the IDE's boxed field (the invite form): `box`, the label
    // unshown, at `size` sm beside the default.
    label: `variant · box, size sm`,
    node: {
      id: `invite`,
      component: `Box`,
      style: { display: `flex`, flexDirection: `column`, gap: `$spacing.sm` },
      children: [
        { id: `invite-sm`, component: `Box`, style: { display: `flex`, flexDirection: `row`, alignItems: `center`, gap: `$spacing.sm` }, children: [
          { id: `invite-name`, component: `Input`, props: { label: `Name`, name: `name`, variant: `box`, size: `sm`, placeholder: `Name` }, style: { width: 176, flexShrink: 0 } },
          { id: `invite-email`, component: `Input`, props: { label: `Email`, name: `email`, variant: `box`, size: `sm`, value: `ada@example.com` }, style: { flexGrow: 1, minWidth: 0 } },
        ] },
        { id: `invite-md`, component: `Input`, props: { label: `Email`, name: `email-md`, variant: `box`, placeholder: `teammate@example.com` } },
      ],
    },
  }],
  Textarea: [{
    // #EXP-1629: the settings pages' text bodies inside a Group: `row`
    // (an agent's instructions) and `code` (a SKILL.md).
    label: `variant · row, code (in a Group)`,
    node: {
      id: `bodies`,
      component: `Group`,
      children: [
        { id: `instructions`, component: `Textarea`, props: { label: `Instructions`, name: `instructions`, variant: `row`, rows: 3, value: `Triage new issues every morning.\nLabel them and ping the board's owner.` } },
        { id: `body`, component: `Textarea`, props: { label: `SKILL.md`, name: `body`, variant: `code`, rows: 4, value: `---\nname: review\n---\nRun the checks, then summarise the diff.` } },
      ],
    },
  }],
}

/** FEED-202: specimens wider than 720 (the Expenses ledger's columns). */
const SPECIMEN_MAX_WIDTH: Record<string, number> = { RecordTable: 1040 }

/** VAPP-162: specimens whose cases sit side by side and wrap: the root's
 *  style and each case's (a phone-like CallSurface column, 360 at most, the
 *  full width on a phone). */
const SPECIMEN_WRAP: Record<string, Record<string, unknown>> = {
  CallSurface: { width: `100%`, maxWidth: 360 },
}

/** Styles a specimen case needs to show anything (a fixed box). */
const SPECIMEN_STYLES: Record<string, Record<string, unknown>> = {
  Resizable: { height: 160 },
  ScrollArea: { height: 160 },
  Window: { height: 200 },
  SplitView: { height: 160 },
}

/** Slots a specimen fills (triggers and footers of the overlays). */
const SPECIMEN_SLOTS: Record<string, Record<string, NestedNode>> = {
  Dialog: {
    trigger: button(`trigger`, `Delete board`, `destructive`),
    footer: { id: `footer`, component: `Stack`, props: { direction: `horizontal`, gap: `sm`, justify: `end` }, children: [button(`cancel`, `Cancel`, `outline`), button(`ok`, `Delete`, `destructive`)] },
  },
  Drawer: { trigger: button(`trigger`, `Filters`, `outline`), footer: button(`apply`, `Apply`) },
  DockCard: { footer: { id: `footer`, component: `Stack`, props: { direction: `horizontal`, gap: `sm`, justify: `end` }, children: [button(`no`, `No, keep planning`, `secondary`), button(`yes`, `Yes`)] } },
  Popover: { trigger: button(`trigger`, `Open popover`, `outline`) },
  Sidebar: { sidebar: { id: `nav`, component: `Stack`, props: { gap: `xs` }, children: [{ id: `n1`, component: `Row`, props: { title: `Inbox`, icon: `nav-inbox`, selected: true, pressable: true }, slots: { trailing: { id: `n1_count`, component: `Badge`, props: { text: `3`, variant: `secondary` } } } }, { id: `n2`, component: `Row`, props: { title: `Issues`, icon: `nav-issues`, pressable: true } }] } },
  AppBar: { trailing: { id: `more`, component: `Button`, props: { icon: `ui-more`, label: `More`, variant: `ghost`, size: `icon` } } },
  AlertDialog: { trigger: button(`trigger`, `Delete board`, `destructive`) },
  Page: { trailing: { id: `more`, component: `Button`, props: { icon: `ui-more`, label: `More`, variant: `ghost`, size: `icon` } } },
  Toolbar: { trailing: button(`new`, `New issue`) },
  SplitView: { list: { id: `rows`, component: `List`, props: { divided: true, gap: `none` }, children: [{ id: `r1`, component: `Row`, props: { title: `Fix crash when uploading HEIC photos`, identifier: `APP-9`, selected: true, pressable: true } }, { id: `r2`, component: `Row`, props: { title: `Offline queue for issue edits`, identifier: `APP-21`, pressable: true } }, { id: `r3`, component: `Row`, props: { title: `Reduce cold start below 800 ms`, identifier: `APP-18`, pressable: true } }] } },
}

/** A case node with the specimen's children + slots in place of `Child`. */
function specimenContent(node: NestedNode): NestedNode {
  const children = SPECIMEN_CHILDREN[node.component]
  const slots = SPECIMEN_SLOTS[node.component]
  const out: NestedNode = { ...node }
  if (children) {
    if (children.length > 0) out.children = children
    else delete out.children
  }
  if (slots) out.slots = slots
  // EXP-1556: a Box's example carries its layout as a `style` prop; every
  // renderer lays a node out by its node style, so the specimen hoists it.
  if (node.component === `Box` && out.props?.style) {
    const { style: hoisted, ...props } = out.props
    out.props = props
    out.style = { ...(hoisted as Record<string, unknown>), ...(out.style ?? {}) }
  }
  const style = SPECIMEN_STYLES[node.component]
  if (style) out.style = { ...(node.style ?? {}), ...style }
  return out
}

/** Re-ids a nested tree so several cases of one component share a surface
 *  (VAPP-289: through `mangle`, so every specimen id is an identifier). */
function prefixIds(node: NestedNode, prefix: string): NestedNode {
  const out: NestedNode = { ...node, id: mangle(`${prefix}${node.id}`) }
  // VAPP-276: an `anchor` names a node id of the same case.
  if (typeof node.props?.anchor === `string`) out.props = { ...node.props, anchor: mangle(`${prefix}${node.props.anchor}`) }
  if (node.children) out.children = node.children.map((c) => prefixIds(c, prefix))
  if (node.slots) out.slots = Object.fromEntries(Object.entries(node.slots).map(([k, v]) => [k, prefixIds(v, prefix)]))
  return out
}

/** The cases a component's specimen shows: an overlay (an `open` prop) once,
 *  open; anything else its example, then up to two enum props × every value,
 *  then `true` for up to two booleans, deduplicated, at most SPECIMEN_MAX_CASES. */
function specimenCases(name: string, all: ComponentCase[]): { label: string; node: NestedNode }[] {
  // The extras are authored whole: their wrapper Box/Stack must keep its own
  // children, not the component's demo children (specimenContent).
  const generated = generatedCases(name, all).map((c) => ({ ...c, node: specimenContent(c.node) }))
  return [...generated, ...(SPECIMEN_EXTRA[name] ?? [])]
}

function generatedCases(name: string, all: ComponentCase[]): { label: string; node: NestedNode }[] {
  if (SPECIMEN_OVERRIDES[name]) return SPECIMEN_OVERRIDES[name]
  const own = all.filter((c) => c.node.component === name)
  const suffix = (c: ComponentCase) => c.name.slice(name.length + 1)
  const def = coreCatalog.components[name]
  // #VAPP-284: a Row's `open` folds its children; it is no overlay.
  if (def.props.open && name !== `Row`) {
    const open = own.find((c) => suffix(c) === `open=true`)!
    // EXP-1653: a timed overlay (Toast) stays up for the shot on every
    // platform instead of dismissing itself before the capture settles.
    const node = def.props.duration ? { ...open.node, props: { ...open.node.props, duration: 0 } } : open.node
    return [{ label: `open`, node }]
  }
  const picked: ComponentCase[] = [own.find((c) => suffix(c) === `example`)!]
  const enums = SPECIMEN_ENUMS.filter((p) => def.props[p]?.type === `enum`).slice(0, 2)
  const bools = SPECIMEN_BOOLEANS.filter((p) => def.props[p]?.type === `boolean`).slice(0, 2)
  for (const prop of enums) picked.push(...own.filter((c) => suffix(c).startsWith(`${prop}=`)))
  for (const prop of bools) picked.push(...own.filter((c) => suffix(c) === `${prop}=true`))
  const seen = new Set<string>()
  const out: { label: string; node: NestedNode }[] = []
  for (const c of picked) {
    const key = JSON.stringify(c.node.props ?? {}, Object.keys(c.node.props ?? {}).sort())
    if (seen.has(key)) continue
    seen.add(key)
    const label = suffix(c) === `example` ? `example` : suffix(c).replace(`=`, ` · `)
    out.push({ label, node: c.node })
    if (out.length === SPECIMEN_MAX_CASES) break
  }
  return out
}

/** Specimens whose WHOLE tree is a shared source file (`{data, cases}`), so
 *  every platform's picture and the styleguide entry paint the same thing.
 *  Round 8: the Canvas (a bar chart, a line plot, a pointer pad), also read
 *  by the styleguide's `canvas` entry and its desktop twin. */
const SPECIMEN_SOURCES: Record<string, string> = { Canvas: `fixtures/canvas-specimen.json` }

/** `{path}` bindings replaced by their value in `data` (a specimen carries no data model). */
function inlineBindings(value: unknown, data: Record<string, unknown>): unknown {
  if (Array.isArray(value)) return value.map((v) => inlineBindings(v, data))
  if (!value || typeof value !== `object`) return value
  const o = value as Record<string, unknown>
  if (Object.keys(o).length === 1 && typeof o[`@path`] === `string`) {
    const got = (o[`@path`] as string).split(`/`).filter(Boolean).reduce<unknown>((at, k) => (at && typeof at === `object` ? (at as Record<string, unknown>)[k] : undefined), data)
    if (got === undefined) throw new Error(`specimen source: ${o.path} has no data`)
    return got
  }
  return Object.fromEntries(Object.entries(o).map(([k, v]) => [k, inlineBindings(v, data)]))
}

/** One component's specimen: a padded column of captioned cases. */
function specimenNode(name: string, all: ComponentCase[]): NestedNode {
  const inline = INLINE_SPECIMENS.has(name)
  const root = `specimen_${snake(name)}`
  const wrap = SPECIMEN_WRAP[name]
  const source = SPECIMEN_SOURCES[name]
  if (source) {
    const { data, cases } = readJson<{ data?: Record<string, unknown>; cases: NestedNode }>(source)
    return {
      id: root,
      component: `Box`,
      style: { display: `flex`, flexDirection: `column`, gap: `$spacing.lg`, padding: `$spacing.xl`, maxWidth: 720 },
      children: [inlineBindings(cases, data ?? {}) as NestedNode],
    }
  }
  return {
    id: root,
    component: `Box`,
    style: wrap
      ? { display: `flex`, flexDirection: `row`, flexWrap: `wrap`, alignItems: `flex-start`, gap: `$spacing.lg`, padding: `$spacing.xl` }
      : { display: `flex`, flexDirection: `column`, gap: `$spacing.lg`, padding: `$spacing.xl`, maxWidth: SPECIMEN_MAX_WIDTH[name] ?? 720 },
    children: specimenCases(name, all).map((c, i) => ({
      id: `${root}_case_${i}`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: inline ? `start` : `stretch` },
      ...(wrap ? { style: wrap } : {}),
      children: [
        { id: `${root}_case_${i}_label`, component: `Text`, props: { text: c.label, variant: `caption` } },
        prefixIds(c.node, `c${i}_`),
      ],
    })),
  }
}

interface SpecimenEntry {
  id: string
  component: string | null
  title: string
  node: NestedNode
}

/** fixtures/specimens.json: every visible component's specimen (view id =
 *  its specimenId) plus the home page's demo surface (fixtures/demo-surface.json). */
function renderSpecimens(all: ComponentCase[], demo: NestedNode) {
  const specimens: SpecimenEntry[] = visible.map(([name]) => ({
    id: specimenId(name),
    component: name,
    title: name,
    node: specimenNode(name, all),
  }))
  const { $comment: _, ...demoNode } = demo as NestedNode & { $comment?: string }
  specimens.push({ id: `exponential-ui-demo`, component: null, title: `Demo`, node: demoNode })
  for (const s of specimens) {
    const result = reduceNested(s.node, { catalogId: CORE_CATALOG_ID })
    if (result.issues.length > 0) throw new Error(`specimen ${s.id}: ${result.issues.map((i) => `${i.id}: ${i.message}`).join(`; `)}`)
  }
  return {
    $comment: `${HEADER} One surface per visible component (id = its docs specimenId = its view-catalog id) plus the home page demo (exponential-ui-demo, from fixtures/demo-surface.json). ui.exponential.at shows the four platform shots of each; every renderer's example host paints an entry by id (web harness ?view=specimen&id=, the IDE's EXP_DEV_SCREEN, the Swift example's -shot, the Compose example's shot extra).`,
    catalogId: CORE_CATALOG_ID,
    specimens,
  }
}

/** Round 3: the depth runs the guides pass must get right (the ×4 vectors of
 *  the app's tree-guides tests), as Row siblings under a Section (any non-Row sibling ends every subtree). */
const TREE_GUIDE_DEPTHS: Record<string, number[]> = {
  flat: [0, 0, 0],
  chain: [0, 1, 2],
  siblings: [0, 1, 1, 1],
  "pass-through": [0, 1, 2, 2, 1],
  "subtree-ends": [0, 1, 2, 3, 2],
  "two-roots": [0, 1, 0, 1],
  orphans: [1, 2, 1],
}

function treeGuideCases(): ComponentCase[] {
  const rows = (depths: number[]) => depths.map((depth, i) => ({ id: `r${i + 1}`, component: `Row`, props: { title: `Row ${i + 1}`, ...(depth ? { depth } : {}) } }))
  const cases: ComponentCase[] = Object.entries(TREE_GUIDE_DEPTHS).map(([name, depths]) => ({
    name: `Row/tree:${name}`,
    node: { id: `tree`, component: `Section`, props: { title: `Tree`, tree: true }, children: rows(depths) },
  }))
  cases.push({ name: `Row/tree:text-breaks-the-subtree`, node: { id: `tree`, component: `Section`, children: [...rows([0, 1]), { id: `t`, component: `Text`, props: { text: `Between` } }, { id: `r3`, component: `Row`, props: { title: `Row 3`, depth: 1 } }] } })
  return cases
}

function treeGuidesFixture() {
  return {
    $comment: `${HEADER} Round 3 (docs/round-3-contract.md §2): the tree connector rule the core computes for nested Rows (src/tree-guides.ts treeGuides, the ×4 app rule): depths → per row the elbow level (null for a root), whether the elbow tees on to a later sibling, and the ancestor levels passing straight through. The Rust core replays it (tree_guides.rs); the painters never compute it.`,
    cases: Object.entries(TREE_GUIDE_DEPTHS).map(([name, depths]) => ({ name, depths, guides: treeGuides(depths) })),
  }
}

/** #VAPP-285: fixtures/tree.json: treeRows over the file tree (literal
 *  `expanded` sets, a bound one, the lazy folder) and treeKey over visible
 *  rows (every key on every row, ltr and rtl). */
const TREE_ROW_CASES: { name: string; expanded: unknown; rail?: unknown }[] = [
  { name: `nothing open`, expanded: [] },
  { name: `apps and web open`, expanded: [`apps`, `apps/web`] },
  { name: `a closed parent hides an open child`, expanded: [`apps/web`] },
  { name: `the lazy folder open: a loading row`, expanded: [`apps`, `apps/desktop`] },
  { name: `an empty folder open shows nothing under it`, expanded: [`packages`, `packages/ui`] },
  { name: `bound: every row, shown and open as calls`, expanded: { "@path": `/open` } },
  // #VAPP-277 × #VAPP-285: a rail Sidebar hands the Tree `rail`.
  { name: `rail: every row shows whatever expanded, no loading row`, expanded: [`apps/desktop`], rail: true },
  { name: `rail false: as without it`, expanded: [`apps`, `apps/desktop`], rail: false },
  { name: `bound rail: every row, shown = cond(rail, …)`, expanded: [`apps`, `apps/desktop`], rail: { "@path": `/collapsed` } },
  { name: `bound rail and bound expanded`, expanded: { "@path": `/open` }, rail: { "@path": `/collapsed` } },
]
const TREE_KEY_ROWS: TreeKeyRow[] = [
  { level: 1, expanded: true },
  { level: 2, expanded: true },
  { level: 3 },
  { level: 3 },
  { level: 2, expanded: false },
  { level: 1, expanded: false },
  { level: 1 },
]
const TREE_KEYS = [`ArrowDown`, `ArrowUp`, `Home`, `End`, `ArrowRight`, `ArrowLeft`, `Enter`]

function treeFixture() {
  return {
    $comment: `${HEADER} #VAPP-285 (docs/round-7-contract.md): the Tree's rules (src/tree.ts; the Rust core's tree.rs replays them). \`rows\`: treeRows(items, expanded) over \`items\` (a repository: two open-able folders, a lazy one, an empty one) for each \`expanded\`: the flat rows the macro draws, depth-first (a literal \`expanded\` keeps only the shown rows; a bound one keeps every row with \`shown\`/\`open\`/\`next\`/\`toggled\` as \`expr\` calls; an open lazy folder gets a \`loading\` row; #VAPP-277 \`rail\` (a rail Sidebar's Tree): true = every row whatever \`expanded\`, no loading row; bound = every row, \`shown\` = cond(rail, true, the shown without it), a loading row's then false). \`keys\`: treeKey(rows, index, key, rtl) over \`keyRows\` (the visible rows: level, expanded on a branch) for every row × key, ltr and rtl: {focus: index} | {toggle: true} | null.`,
    items: FILE_TREE,
    rows: TREE_ROW_CASES.map((c) => ({ name: c.name, expanded: c.expanded, ...(c.rail !== undefined ? { rail: c.rail } : {}), rows: treeRows(FILE_TREE, c.expanded, c.rail) })),
    keyRows: TREE_KEY_ROWS,
    keys: [false, true].flatMap((rtl) => TREE_KEY_ROWS.flatMap((_, index) => TREE_KEYS.map((key) => ({ index, key, rtl, result: treeKey(TREE_KEY_ROWS, index, key, rtl) })))),
  }
}

/** VAPP-163: a style written ON a macro node wins over the macro's part
 *  recipes; the expander lists its keys (sorted) in the root's
 *  `recipe.authored` (a condition block counts as one key). */
function authoredStyleCases(): ComponentCase[] {
  return [
    { name: `Heading/author-style`, node: { id: `heading`, component: `Heading`, props: { text: `On a band`, level: `h1` }, style: { fontSize: 40, color: `$color.primaryForeground`, ":hover": { opacity: 0.9 } } } },
    { name: `Badge/author-style`, node: { id: `badge`, component: `Badge`, props: { text: `New`, variant: `secondary` }, style: { backgroundColor: `$color.success` } } },
  ]
}

/** #VAPP-277: a Sidebar's nav column, for the rail cases: a count and a
 *  trailing slot on Inbox, a leading slot instead of an icon on Issues, a
 *  Row in a closed collapsible Section (the rail shows it: header → rule). */
function railNav(): NestedNode {
  return {
    id: `nav`,
    component: `Stack`,
    props: { gap: `xs` },
    children: [
      { id: `inbox`, component: `Row`, props: { title: `Inbox`, icon: `nav-inbox`, meta: `3`, selected: true, pressable: true }, slots: { trailing: { id: `inbox_new`, component: `Badge`, props: { text: `New` } } } },
      { id: `issues`, component: `Row`, props: { title: `Issues`, pressable: true }, slots: { leading: { id: `issues_dot`, component: `Icon`, props: { name: `nav-issues`, size: `sm` } } } },
      { id: `more`, component: `Section`, props: { title: `More`, collapsible: true, open: false }, children: [{ id: `search`, component: `Row`, props: { title: `Search`, icon: `nav-search`, meta: `120`, pressable: true } }] },
    ],
  }
}

/** #VAPP-277: Sidebar `rail` hands its `collapsed` to the sidebar's Rows
 *  (`$pass`): collapsed = the icon rail, expanded = the Rows as without
 *  `rail`, bound = calls (bind-time.json binds them). */
function railCases(): ComponentCase[] {
  const sidebar = (name: string, collapsed: unknown): ComponentCase => ({
    name: `Sidebar/rail:${name}`,
    node: { id: `shell`, component: `Sidebar`, props: { rail: true, collapsed, collapseBelow: `never` }, slots: { sidebar: railNav() }, children: [{ id: `main`, component: `Text`, props: { text: `Pick a board.` } }] },
  })
  const right = sidebar(`right`, true)
  right.node.props = { ...right.node.props, side: `right` }
  return [sidebar(`collapsed`, true), sidebar(`expanded`, false), sidebar(`bound`, { "@path": `/collapsed` }), right]
}

/** #VAPP-284: the fold cases (helpers above SPECIMEN_EXTRA). */
function foldCases(): ComponentCase[] {
  return [
    { name: `Row/fold:closed`, node: foldRow(false) },
    { name: `Row/fold:open`, node: foldRow(true) },
    { name: `Row/fold:plain-open`, node: { ...foldRow(true), on: { change: FOLD_ON.change } } },
    { name: `Row/fold:bound`, node: foldRow({ "@path": `/open` }) },
    { name: `Row/fold:rtl`, node: { ...foldRow(true), style: { direction: `rtl` } } },
  ]
}

/** #VAPP-277 × #VAPP-284: a foldable parent in a rail Sidebar. Collapsed:
 *  its head is the icon tile and its children follow as tiles whatever
 *  `open` (as a rail Section shows its closed body); expanded: the plain
 *  foldable parent (closed = the children hidden). */
function railFoldCases(): ComponentCase[] {
  const nav: NestedNode = { id: `nav`, component: `Stack`, props: { gap: `xs` }, children: [
    { id: `inbox`, component: `Row`, props: { title: `Inbox`, icon: `nav-inbox`, pressable: true } },
    { ...foldRow(false), props: { ...foldRow(false).props, meta: `2` } },
  ] }
  const sidebar = (name: string, collapsed: boolean): ComponentCase => ({
    name: `Sidebar/rail:${name}`,
    node: { id: `shell`, component: `Sidebar`, props: { rail: true, collapsed, collapseBelow: `never` }, slots: { sidebar: nav }, children: [{ id: `main`, component: `Text`, props: { text: `Pick a board.` } }] },
  })
  return [sidebar(`fold_collapsed`, true), sidebar(`fold_expanded`, false)]
}

/** #VAPP-277 × #VAPP-285: a Tree in a collapsed rail Sidebar: every row a
 *  tile (the closed `packages` folder's children too, the lazy folder
 *  without its loading row), no tree roles, levels or fold glyphs. */
function railTreeCases(): ComponentCase[] {
  return [{
    name: `Sidebar/rail:tree`,
    node: { id: `shell`, component: `Sidebar`, props: { rail: true, collapsed: true, collapseBelow: `never` }, slots: { sidebar: fileTree({ expanded: [`apps`, `apps/desktop`] }, { on: TREE_ON }) }, children: [{ id: `main`, component: `Text`, props: { text: `Pick a file.` } }] },
  }]
}

/** #EXP-1736: the sender's avatar left of an incoming bubble (an `out` one
 *  drops the slot), with the press that opens the sender. */
function messageBubbleCases(): ComponentCase[] {
  const avatar: NestedNode = { id: `bubble_avatar`, component: `Avatar`, props: { name: `Release agent`, seed: `agent-release` } }
  const props = { text: `Merged; the tag is pushed.`, author: `Release agent`, time: `18:04`, href: `https://ui.exponential.at` }
  return [
    { name: `MessageBubble/avatar:in`, node: { id: `bubble`, component: `MessageBubble`, props, slots: { avatar } } },
    { name: `MessageBubble/avatar:out`, node: { id: `bubble`, component: `MessageBubble`, props: { ...props, side: `out` }, slots: { avatar } } },
    { name: `MessageBubble/avatar:press`, node: { id: `bubble`, component: `MessageBubble`, props: { text: props.text, author: props.author }, slots: { avatar }, on: { press: { event: { name: `openSender` } } } } },
  ]
}

function macroCases(all: ComponentCase[]) {
  return [...all, ...boundMacroCases(), ...treeGuideCases(), ...authoredStyleCases(), ...railCases(), ...foldCases(), ...railFoldCases(), ...treeCases(), ...railTreeCases(), ...messageBubbleCases()]
    .filter((c) => coreCatalog.components[c.node.component].kind === `macro`)
    .map((c) => {
      const result = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
      if (result.issues.length > 0) throw new Error(`${c.name}: ${result.issues.map((i) => i.message).join(`; `)}`)
      return { name: c.name, input: c.node, expected: result.root, templates: result.templates }
    })
    // A case whose expansion builds data templates (VAPP-278: Meter/bound:segments)
    // is locked in template-items.json `macro-template-*`, where `templates` replay.
    .filter((c) => Object.keys(c.templates ?? {}).length === 0)
    .map(({ templates: _, ...c }) => c)
}

interface BasicCaseFile {
  $comment: string
  catalogId: string
  cases: { name: string; components: FlatComponent[]; expected?: unknown }[]
}

function basicCases(file: BasicCaseFile) {
  return {
    ...file,
    cases: file.cases.map((c) => ({
      name: c.name,
      components: c.components,
      expected: reduceSurface(c.components, { catalogId: A2UI_BASIC_CATALOG_ID }),
    })),
  }
}

interface ExtensionCaseFile {
  $comment: string
  extension: ExtensionDef
  cases: { name: string; catalogId: string; components: FlatComponent[]; expected?: unknown }[]
}

function extensionCases(file: ExtensionCaseFile) {
  return {
    ...file,
    cases: file.cases.map((c) => ({
      name: c.name,
      catalogId: c.catalogId,
      components: c.components,
      expected: reduceSurface(c.components, { catalogId: c.catalogId, extensions: [file.extension] }),
    })),
  }
}

/** A value a bound prop takes in the bind-time data: the example's, else a
 *  sample by type; the second dataset flips it to its falsy twin (`0` stays
 *  TRUTHY under `truthy`: the number twin exercises the arithmetic, not the
 *  truthiness); the third (`MISSING`) leaves the path out, so every
 *  operand's `fallback{value, default}` takes its default. */
function boundSamples(schema: PropSchema, example: unknown): [unknown, unknown] {
  if (schema.type === `boolean`) return [true, false]
  if (schema.type === `number`) return [typeof example === `number` ? example : 3, 0]
  if (schema.type === `array`) return [example ?? [], []]
  if (schema.type === `object`) return [example ?? {}, null]
  return [typeof example === `string` ? example : `Bound`, ``]
}

function collect(node: UiNode, out: UiNode[] = []): UiNode[] {
  out.push(node)
  for (const slot of Object.values(node.slots ?? {})) collect(slot, out)
  node.children.forEach((c) => collect(c, out))
  return out
}

const BIND_OPTIONS = { strings: DEFAULT_STRINGS }

/** One dataset through the bind pass: the bound tree, every `set` press
 *  (runAction) and, for every ROW-SCOPED component (Table) left in the bound
 *  tree, each row's slot cells bound with the row as their scope
 *  (bindRowSlot; keyed by the row's index in `rows`). */
function bindDataset(root: UiNode, data: Record<string, unknown>) {
  const bound = bindTree(root, data, BIND_OPTIONS)
  const out: Record<string, unknown> = {
    data,
    bound,
    presses: collect(root)
      .filter((n) => n.on?.press?.functionCall)
      .map((n) => ({ id: n.id, outcome: runAction(n.on!.press, data, BIND_OPTIONS) })),
  }
  const unbound = new Map(collect(root).map((n) => [n.id, n]))
  const rowSlots = (bound ? collect(bound) : [])
    .filter((n) => n.slots && hasRowSlots(coreCatalog.components[n.component]))
    .map((n) => {
      const rows = Array.isArray(n.props.rows) ? n.props.rows : []
      const rowsProp = unbound.get(n.id)?.props.rows
      return {
        id: n.id,
        rows: rows.map((_, index) => ({
          index,
          slots: Object.fromEntries(Object.entries(n.slots ?? {}).map(([name, slot]) => [name, bindRowSlot(slot, rowsProp, rows, index, data, BIND_OPTIONS)])),
        })),
      }
    })
  if (rowSlots.length > 0) out.rowSlots = rowSlots
  return out
}

/** Round 1: every bound macro case × three data models → the tree after the
 *  BIND pass (src/dynamic.ts bindTree) and what each press with a `set`
 *  write does (runAction). The renderers' bind-time resolvers replay it. */
function bindTimeCases() {
  return boundMacroCases()
    .filter((c) => c.name.includes(`/bound:`))
    .map((c) => {
      const prop = c.name.split(`/bound:`)[1]
      const def = coreCatalog.components[c.node.component]
      const example = (def.example as Record<string, unknown> | undefined)?.[prop]
      const { root, issues } = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
      if (issues.length > 0) throw new Error(`${c.name}: ${issues.map((i) => i.message).join(`; `)}`)
      const datasets = [...boundSamples(def.props[prop], example).map((value) => ({ [prop]: value })), {}]
      return { name: c.name, input: c.node, expanded: root, datasets: datasets.map((data) => bindDataset(root, data)) }
    })
}

/** Round 1: the hand-written bind cases the per-prop sweep cannot reach —
 *  row-scoped Table slot cells (bound and literal rows), DATA props copied
 *  verbatim, a bound accessible name, an author function meeting a `$set`
 *  (an issue, the author's call kept) and a numeric prop bound to string
 *  data. `issues` = what the reducer reported. */
const BIND_EXTRA_CASES: { name: string; input: NestedNode; datasets: Record<string, unknown>[] }[] = [
  {
    name: `Table/slot-cell:bound-rows`,
    input: {
      id: `table`,
      component: `Table`,
      props: { columns: [{ key: `name`, label: `Name` }, { key: `s`, label: `Status`, type: `slot`, slot: `status` }], rows: { "@path": `/rows` } },
      slots: { status: { id: `cell`, component: `Badge`, props: { text: { "@path": `s` } }, visible: { "@path": `show` } } },
    },
    datasets: [{ rows: [{ id: `a`, name: `Alice`, s: `ok`, show: true }, { id: `b`, name: `Bob`, s: `late`, show: false }, { id: `c`, name: `Cy`, s: 0, show: 0 }] }, { rows: [] }, {}],
  },
  {
    name: `Table/slot-cell:literal-rows`,
    input: {
      id: `table`,
      component: `Table`,
      props: {
        columns: [{ key: `path`, label: `Path` }, { key: `call`, label: `Call` }, { key: `note`, label: `Note` }, { key: `s`, label: `Status`, type: `slot`, slot: `status` }],
        rows: [
          { id: `a`, path: `/etc/hosts`, call: `openUrl`, note: `$string.cancel`, s: `ok` },
          { id: `b`, path: { "@path": `/etc/hosts` }, call: { "@call": `not`, args: { value: true } }, note: `plain`, s: `late` },
        ],
      },
      slots: { status: { id: `cell`, component: `Text`, props: { text: { "@call": `concat`, args: { values: [{ "@path": `s` }, ` · `, { "@path": `/suffix` }] } } } } },
    },
    datasets: [{ etc: { hosts: `PWNED` }, suffix: `today` }, {}],
  },
  {
    name: `Text/accessibility:bound-label`,
    input: { id: `text`, component: `Text`, props: { text: `hi` }, accessibility: { label: { "@path": `/l` }, description: `$string.close` } },
    datasets: [{ l: `Hello` }, {}],
  },
  {
    name: `Section/set:author-function`,
    input: { id: `section`, component: `Section`, props: { title: `More`, collapsible: true, open: { "@path": `/open` } }, on: { change: { functionCall: { "@call": `openUrl`, args: { url: `https://example.com` } } } }, children: [{ id: `r`, component: `Row`, props: { title: `A` } }] },
    datasets: [{ open: true }],
  },
  {
    // #VAPP-284: a foldable Row bound to /open: the head's fold button and
    // the plain head both write the flip back (`set`).
    name: `Row/fold:bound`,
    // No author handlers: a press is the two-way `set` alone (the renderers'
    // bind replays compare the data a press writes).
    input: { ...foldRow({ "@path": `/open` }), props: { ...foldRow({ "@path": `/open` }).props, pressable: true }, on: {} },
    datasets: [{ open: true }, { open: false }, {}],
  },
  {
    name: `Row/fold:bound-plain`,
    input: { ...foldRow({ "@path": `/open` }), on: {} },
    datasets: [{ open: true }, {}],
  },
  {
    // #VAPP-285: a Tree bound to /open and /picked: rows show while every
    // ancestor is open; a branch press writes the toggled list, a leaf the id.
    name: `Tree/bound`,
    input: fileTree({ expanded: { "@path": `/open` }, value: { "@path": `/picked` } }),
    datasets: [{ open: [`apps`, `apps/web`], picked: `apps/web/router.ts` }, { open: [`apps`, `apps/desktop`] }, { open: [] }, {}],
  },
  {
    name: `Pagination/page:string-data`,
    input: { id: `pagination`, component: `Pagination`, props: { totalPages: 5, page: { "@path": `/page` } } },
    datasets: [{ page: `5` }, { page: `1` }, { page: 9 }, {}],
  },
  {
    // VAPP-191: the expression functions and the CEL string form at bind
    // time (no today/now: the fixture has no clock; UTC).
    name: `Text/expressions:bound`,
    input: {
      id: `text`,
      component: `Text`,
      props: { text: { "@call": `formatDuration`, args: { hours: { "@call": `expr`, args: { expr: `round(minutes / 60, 2)`, vars: { minutes: { "@path": `/minutes` } } } }, format: `h:mm` } } },
      accessibility: { label: { "@call": `dateAdd`, args: { date: { "@path": `/day` }, n: 1, unit: `month` } } },
      visible: { "@call": `expr`, args: { expr: `weekday(d) >= 6 || mul(minutes, 2) > 1000`, vars: { d: { "@path": `/day` }, minutes: { "@path": `/minutes` } } } },
    },
    datasets: [{ minutes: 450, day: `2026-01-31` }, { minutes: 600, day: `2026-02-02` }, { minutes: 450, day: `2026-02-02` }, { minutes: `x`, day: `nope` }, {}],
  },
  {
    // #VAPP-277: a rail Sidebar whose `collapsed` is bound: every rail Row
    // part and style is a call the bind pass resolves (rail when true,
    // the plain Rows when false or missing).
    name: `Sidebar/rail:bound-collapsed`,
    input: { id: `shell`, component: `Sidebar`, props: { rail: true, collapsed: { "@path": `/collapsed` }, collapseBelow: `never` }, slots: { sidebar: railNav() } },
    datasets: [{ collapsed: true }, { collapsed: false }, {}],
  },
  // Round 8: a collapsed Stepper bound to /current shows ONE line (each
  // step's line is visible while it is current; none past the last); a
  // Progress whose trailing text and caption are bound.
  {
    name: `Stepper/collapse-always:bound-current`,
    input: { id: `wizard`, component: `Stepper`, props: { steps: [{ label: `Account` }, { label: `Team` }, { label: `Done` }], current: { "@path": `/current` }, collapse: `always` } },
    datasets: [{ current: 0 }, { current: 2 }, { current: 5 }, {}],
  },
  {
    name: `Progress/bound:valueText+detail`,
    input: { id: `download`, component: `Progress`, props: { value: { "@path": `/done` }, valueText: { "@path": `/rate` }, detail: { "@path": `/files` } } },
    datasets: [{ done: 41, rate: `41 MB/s · 12 min`, files: `3 of 8 files` }, { done: 0 }],
  },
  // #EXP-1629: a Tooltip whose hint is bound (the settings MCP servers
  // pane's per-row Reconnect tooltip reads the row's reason).
  {
    name: `Tooltip/bound:content`,
    input: { id: `tip`, component: `Tooltip`, props: { content: { "@path": `/reason` }, side: `top` }, children: [{ id: `reconnect`, component: `Button`, props: { label: `Reconnect` } }] },
    datasets: [{ reason: `Token expired` }, { reason: `` }, {}],
  },
  // #EDE-65: a bubble is pressable only while its href RESOLVES to a
  // non-empty value (a URL, not null, "" or missing) or it has an on.press;
  // a literal href always is. Row follows the same rule.
  {
    name: `MessageBubble/href:bound-resolves`,
    input: { id: `bubble`, component: `MessageBubble`, props: { text: `Merged.`, href: { "@path": `/href` } } },
    datasets: [{ href: `https://exponential.at` }, { href: null }, { href: `` }, {}],
  },
  {
    name: `MessageBubble/href:literal`,
    input: { id: `bubble`, component: `MessageBubble`, props: { text: `Merged.`, href: `https://exponential.at` } },
    datasets: [{}],
  },
  {
    name: `MessageBubble/press:no-href`,
    input: { id: `bubble`, component: `MessageBubble`, props: { text: `Merged.` }, on: { press: { event: { name: `open` } } } },
    datasets: [{}],
  },
  {
    name: `MessageBubble/press:bound-href`,
    input: { id: `bubble`, component: `MessageBubble`, props: { text: `Merged.`, href: { "@path": `/href` } }, on: { press: { event: { name: `open` } } } },
    datasets: [{ href: null }],
  },
  {
    name: `Row/href:bound-resolves`,
    input: { id: `row`, component: `Row`, props: { title: `Docs`, href: { "@path": `/href` } } },
    datasets: [{ href: `https://exponential.at` }, { href: null }, {}],
  },
]

function bindExtraCases() {
  return BIND_EXTRA_CASES.map((c) => {
    const { root, issues } = reduceNested(c.input, { catalogId: CORE_CATALOG_ID })
    return { name: c.name, input: c.input, expanded: root, issues, datasets: c.datasets.map((data) => bindDataset(root, data)) }
  })
}

interface ConditionCaseFile {
  $comment: string
  breakpoints: Record<string, number>
  cases: { name: string; style: Record<string, unknown>; contexts: (ConditionContext & { label: string })[]; expected?: unknown }[]
}

function styleConditionCases(file: ConditionCaseFile) {
  return {
    ...file,
    cases: file.cases.map((c) => ({
      name: c.name,
      style: c.style,
      contexts: c.contexts,
      expected: c.contexts.map((ctx) => resolveConditions(c.style, { ...ctx, breakpoints: ctx.breakpoints ?? file.breakpoints })),
    })),
  }
}

interface CodeCaseFile {
  $comment: string
  cases: { name: string; language: string; code: string; expected?: unknown }[]
}

function codeCases(file: CodeCaseFile) {
  return { ...file, cases: file.cases.map((c) => ({ name: c.name, language: c.language, code: c.code, expected: tokenizeCode(c.code, c.language) })) }
}

interface ShimmerCaseFile {
  $comment: string
  cases: { name: string; color: string; dark: boolean; expected?: unknown }[]
}

function shimmerCases(file: ShimmerCaseFile) {
  return { ...file, cases: file.cases.map((c) => ({ name: c.name, color: c.color, dark: c.dark, expected: textShimmer(c.color, c.dark) })) }
}

interface VoiceWaveCaseFile {
  $comment: string
  cases: { name: string; input: VoiceWaveInput; expected?: unknown }[]
  bars: { name: string; input: VoiceWaveBarsInput; expected?: unknown }[]
}

function voiceWaveCases(file: VoiceWaveCaseFile) {
  return {
    ...file,
    cases: file.cases.map((c) => ({ name: c.name, input: c.input, expected: voiceWave(c.input) })),
    bars: file.bars.map((c) => ({ name: c.name, input: c.input, expected: voiceWaveBars(c.input) })),
  }
}

interface QrCaseFile {
  $comment: string
  cases: { name: string; value: string; level: string; expected?: unknown }[]
}

function qrCases(file: QrCaseFile) {
  return { ...file, cases: file.cases.map((c) => ({ name: c.name, value: c.value, level: c.level, expected: qrMatrix(c.value, c.level) })) }
}

// EXP-1586: fixtures/zoom.json — the inputs are hand-written (named frames,
// `from: "rest"` = the frame's rest view), `expected` = src/zoom.ts.
type ZoomCase = Record<string, unknown> & { name?: string; frame?: string; from?: zoom.ZoomView | `rest`; viewport?: zoom.ZoomSize }
interface ZoomCaseFile { frames: Record<string, zoom.ZoomFrame>; [group: string]: unknown }

function zoomCases(file: ZoomCaseFile) {
  const frameOf = (c: ZoomCase): zoom.ZoomFrame => {
    const base = file.frames[c.frame ?? ``]
    if (!base) throw new Error(`zoom.json: unknown frame ${c.frame}`)
    return c.viewport ? { ...base, viewport: c.viewport } : base
  }
  const fromOf = (c: ZoomCase, f: zoom.ZoomFrame) => (c.from === `rest` || c.from === undefined ? zoom.zoomRestView(f) : (c.from as zoom.ZoomView))
  const group = (key: string, run: (c: ZoomCase) => unknown) =>
    (file[key] as ZoomCase[]).map((c) => {
      const { expected: _drop, ...input } = c
      return { ...input, expected: run(c) }
    })
  return {
    $comment: `${HEADER} EXP-1586: the ZoomableImage maths (catalog/style.json \`zoomableImage\`; src/zoom.ts, the Rust core's \`zoom\`). Inputs hand-written: \`frames\` = named {natural, viewport, initial?, maxScale?}, a case names its frame (\`viewport\` overrides it) and \`from\` = a view {scale, x, y} or "rest" (zoomRestView). \`rest\` = {fit: zoomFitScale, limits: zoomLimits, restScale: zoomRestScale, view: zoomRestView, tall: zoomIsTall}; \`clamp\` = zoomClamp; \`zoomTo\` = zoomTo; \`wheel\` = zoomWheel (⌘/ctrl); \`scroll\` = zoomScroll (a plain wheel); \`ladder\` = zoomLadder; \`steps\` = zoomStepScale; \`doubleTap\` = zoomDoubleTap; \`keys\` = zoomKey (null = not a zoom key); \`rubberBand\` = zoomRubberBand; \`drag\` = zoomDrag (held); \`pinch\` = zoomPinch (held); \`momentum\` = zoomMomentum; \`momentumActive\` = zoomMomentumActive; \`panRoom\` = zoomPanRoom; \`resize\` = zoomResize; \`percent\` = [scale, zoomPercent]; \`menu\` = zoomMenu (fit label "Fit"); \`bounds\` = zoomBounds. Compare numbers within 1e-6.`,
    frames: file.frames,
    rest: group(`rest`, (c) => {
      const f = frameOf(c)
      return { fit: zoom.zoomFitScale(f), limits: zoom.zoomLimits(f), restScale: zoom.zoomRestScale(f), view: zoom.zoomRestView(f), tall: zoom.zoomIsTall(f.natural) }
    }),
    clamp: group(`clamp`, (c) => zoom.zoomClamp(c.view as zoom.ZoomView, frameOf(c))),
    zoomTo: group(`zoomTo`, (c) => {
      const f = frameOf(c)
      return zoom.zoomTo(fromOf(c, f), c.scale as number, c.anchor as zoom.ZoomPoint, f)
    }),
    wheel: group(`wheel`, (c) => {
      const f = frameOf(c)
      return zoom.zoomWheel(fromOf(c, f), c.deltaY as number, c.mode as zoom.WheelDeltaMode, c.anchor as zoom.ZoomPoint, f)
    }),
    scroll: group(`scroll`, (c) => {
      const f = frameOf(c)
      return zoom.zoomScroll(fromOf(c, f), c.deltaX as number, c.deltaY as number, f)
    }),
    ladder: group(`ladder`, (c) => zoom.zoomLadder(frameOf(c))),
    steps: group(`steps`, (c) => zoom.zoomStepScale(c.scale as number, c.direction as 1 | -1, frameOf(c))),
    doubleTap: group(`doubleTap`, (c) => {
      const f = frameOf(c)
      return zoom.zoomDoubleTap(fromOf(c, f), c.point as zoom.ZoomPoint, f)
    }),
    keys: group(`keys`, (c) => {
      const f = frameOf(c)
      return zoom.zoomKey(fromOf(c, f), c.key as string, f)
    }),
    rubberBand: group(`rubberBand`, (c) => zoom.zoomRubberBand(c.overshoot as number, c.extent as number)),
    drag: group(`drag`, (c) => {
      const f = frameOf(c)
      return zoom.zoomDrag(fromOf(c, f), c.dx as number, c.dy as number, f)
    }),
    pinch: group(`pinch`, (c) => {
      const f = frameOf(c)
      return zoom.zoomPinch(fromOf(c, f), c.factor as number, c.anchor as zoom.ZoomPoint, f)
    }),
    momentum: group(`momentum`, (c) => {
      const f = frameOf(c)
      return zoom.zoomMomentum(fromOf(c, f), c.vx as number, c.vy as number, c.t as number, f)
    }),
    momentumActive: group(`momentumActive`, (c) => zoom.zoomMomentumActive(c.vx as number, c.vy as number, c.t as number)),
    panRoom: group(`panRoom`, (c) => {
      const f = frameOf(c)
      return zoom.zoomPanRoom(fromOf(c, f), f)
    }),
    resize: group(`resize`, (c) => {
      const f = frameOf(c)
      const from = c.fromViewport as zoom.ZoomSize
      const start = c.from === `rest` ? zoom.zoomRestView({ ...f, viewport: from }) : (c.from as zoom.ZoomView)
      return zoom.zoomResize(start, from, f)
    }),
    percent: (file.percent as (number | [number, string])[]).map((p) => {
      const s = Array.isArray(p) ? p[0] : p
      return [s, zoom.zoomPercent(s)]
    }),
    menu: group(`menu`, (c) => {
      const f = frameOf(c)
      return zoom.zoomMenu(fromOf(c, f), f, `Fit`)
    }),
    bounds: group(`bounds`, (c) => {
      const f = frameOf(c)
      return zoom.zoomBounds(fromOf(c, f), f)
    }),
  }
}

function promptBudget() {
  const full = catalogPrompt()
  const liteText = catalogPrompt({ lite: true })
  const terse = catalogPrompt({ terse: true })
  // VAPP-108: the sectioned prompt. `index` = rules + the one-line index +
  // functions + style (what a host serves first; components come on demand
  // through componentPrompt); `rules` alone; the per-component blocks.
  const index = catalogPrompt({ sections: [`rules`, `index`, `functions`, `style`] })
  const rules = catalogPrompt({ sections: [`rules`] })
  const blocks = visible.map(([name]) => componentPrompt(name).length)
  const size = (text: string) => ({ chars: text.length, tokens: estimateTokens(text) })
  return {
    $comment: `${HEADER} The size of catalogPrompt() on record: prompt.test.ts fails when the full prompt passes budgetTokens (cut descriptions before components) or when these numbers drift from the catalog without being regenerated. tokens = chars / ${4}, an estimate. VAPP-108: \`index\` = sections rules + index + functions + style (the components then come one at a time through componentPrompt, \`perComponent\`), \`rules\` = the rules alone.`,
    // Danny's ceiling (2026-11-07): 10,000 tokens for the full prompt; trim new descriptions before adding components, no named exceptions.
    budgetTokens: 10000,
    full: { components: visible.length, ...size(full) },
    lite: { components: lite.length, ...size(liteText) },
    terse: { components: visible.length, ...size(terse) },
    index: { components: visible.length, ...size(index) },
    rules: size(rules),
    perComponent: { mean: Math.round(blocks.reduce((a, b) => a + b, 0) / blocks.length), max: Math.max(...blocks), meanTokens: estimateTokens(String(``).padEnd(Math.round(blocks.reduce((a, b) => a + b, 0) / blocks.length))) },
  }
}

// ---------------------------------------------------------------------------
// Render everything
// ---------------------------------------------------------------------------

function readJson<T>(rel: string): T {
  return JSON.parse(readFileSync(join(pkgRoot, rel), `utf8`)) as T
}

/** Every generated file, keyed by its path relative to the package root. */
export function render(): Record<string, string> {
  const files = renderFiles()
  // The manifest counts the fixtures being written, not the bundled ones.
  const read = (name: string) => JSON.parse(files[`fixtures/${name}`] ?? readFileSync(join(pkgRoot, `fixtures`, name), `utf8`))
  return { ...files, "conformance/manifest.json": json(conformanceManifest(read)) }
}

function renderFiles(): Record<string, string> {
  const icons = iconNames()
  const cases = componentCases()
  const kitchen = readJson<NestedNode>(`fixtures/kitchen-sink.json`)
  const kitchenResult = reduceNested(kitchen, { catalogId: CORE_CATALOG_ID })
  if (kitchenResult.issues.length > 0)
    throw new Error(`kitchen-sink.json: ${kitchenResult.issues.map((i) => `${i.id}: ${i.message}`).join(`\n`)}`)
  const themeOutputs = renderThemes(cases, kitchen)
  const rust = renderRust(icons)
  const rustDescriptions = renderRustDescriptions()
  return {
    // Round 9 §11: the 1.0 catalog definitions (catalog_definition.json).
    "catalog/core.schema.json": json(coreSchema(icons)),
    "catalog/core-lite.schema.json": json(coreLiteSchema(icons)),
    // VAPP-106: the runtime catalog and its text, split (src/catalog.described.ts).
    "generated/core.runtime.json": json(catalogSplit.runtime),
    "generated/core.descriptions.json": json(catalogSplit.descriptions),
    // VAPP-160: catalog/style.json without its `$comment`s (the spec prose),
    // what src/ imports so no bundle ships the comments.
    "generated/style.runtime.json": json(stripComments(styleJson)),
    // The host contract's runtime slice: only the keys src/host/contract.ts
    // and src/host/exposes.ts read, so a renderer bundle never ships the package-format spec,
    // the capability scopes or the prose of catalog/host.json.
    "generated/host.runtime.json": json(hostRuntime()),
    "generated/catalog.descriptions.generated.rs": rustDescriptions,
    "../../apps/desktop/crates/eui-core/src/generated/descriptions.rs": rustDescriptions,
    "src/catalog.generated.ts": renderTs(icons),
    "generated/ExponentialUICatalog.generated.swift": renderSwift(),
    "generated/ExponentialUICatalog.generated.kt": renderKotlin(),
    "generated/catalog.generated.rs": rust,
    // VAPP-86: the Rust core's copies (a published crate cannot include! a file
    // outside its own directory); src/generate.test.ts gates them like the rest.
    "../../apps/desktop/crates/eui-core/src/generated/catalog.rs": rust,
    "../../apps/desktop/crates/eui-core/src/generated/themes.rs": themeOutputs[`generated/catalog.themes.generated.rs`],
    "docs/components.generated.json": json(renderDocs()),
    "fixtures/catalog-components.json": json({
      $comment: `${HEADER} Every visible component × its example × every enum value of every enum prop × both values of every boolean prop, then its state cases (a prop that only shows in a state: a pending Button's pendingLabel). Each case validates against core.schema.json; a renderer suite paints each case once.`,
      catalogId: CORE_CATALOG_ID,
      cases,
    }),
    "fixtures/catalog-macros.json": json({
      $comment: `${HEADER} Every macro case of catalog-components.json (plus the */author-style cases: the macro node's own style keys, sorted, land in the root's recipe.authored and win over the part recipes) → the tree the expander produces (macros.json applied, recipes attached). The Rust core replays these byte for byte.`,
      catalogId: CORE_CATALOG_ID,
      cases: macroCases(cases),
    }),
    "fixtures/catalog-basic-map.json": json(basicCases(readJson<BasicCaseFile>(`fixtures/catalog-basic-map.json`))),
    "fixtures/catalog-extension.json": json(extensionCases(readJson<ExtensionCaseFile>(`fixtures/catalog-extension.json`))),
    "fixtures/kitchen-sink.expanded.json": json({
      $comment: `${HEADER} fixtures/kitchen-sink.json reduced and expanded: what every painter receives.`,
      ...kitchenResult,
    }),
    "fixtures/prompt-budget.json": json(promptBudget()),
    "fixtures/tree-guides.json": json(treeGuidesFixture()),
    "fixtures/tree.json": json(treeFixture()),
    "fixtures/bind-time.json": json({
      $comment: `${HEADER} Round 1 (docs/round-1-contract.md §1): every macro prop that takes a binding, bound to /<prop> → the expansion (calls where the input was dynamic) → for three data models (a sample, its falsy twin, and the path MISSING) the tree after the BIND pass (src/dynamic.ts bindTree: props resolved along their schema with DATA props verbatim, styles, recipe props and \`accessibility\` resolved, falsy \`visible\` nodes dropped, \`$string.<id>\` copy resolved through the DEFAULT string table, row-scoped slots left unbound) and every \`set\` press (runAction: context first, then the write). \`extra\` = hand-written cases (Table slot cells bound PER ROW in \`rowSlots\`, literal data rows, a bound accessible name, an author function meeting \`$set\` with the reducer's \`issues\`, a numeric prop bound to string data). Every renderer's bind-time resolver replays both lists.`,
      catalogId: CORE_CATALOG_ID,
      cases: bindTimeCases(),
      extra: bindExtraCases(),
    }),
    "fixtures/style-conditions.json": json(styleConditionCases(readJson<ConditionCaseFile>(`fixtures/style-conditions.json`))),
    "fixtures/code-tokens.json": json(codeCases(readJson<CodeCaseFile>(`fixtures/code-tokens.json`))),
    "fixtures/text-shimmer.json": json(shimmerCases(readJson<ShimmerCaseFile>(`fixtures/text-shimmer.json`))),
    "fixtures/voice-wave.json": json(voiceWaveCases(readJson<VoiceWaveCaseFile>(`fixtures/voice-wave.json`))),
    "fixtures/qr-code.json": json(qrCases(readJson<QrCaseFile>(`fixtures/qr-code.json`))),
    "fixtures/zoom.json": json(zoomCases(readJson<ZoomCaseFile>(`fixtures/zoom.json`))),
    "fixtures/specimens.json": json(renderSpecimens(cases, readJson<NestedNode>(`fixtures/demo-surface.json`))),
    ...themeOutputs,
    // VAPP-289: the UAX #31 identifier tables (round 9 §4).
    ...renderXid(),
    // VAPP-91: the host fixtures (inputs hand-written, expected filled).
    ...renderHost(),
    ...renderA2ui10(),
    // Round 2: the contract fixtures (inputs in scripts/generate-round2.ts).
    ...renderRound2(),
    // Round 4: the styling + semantics contract (inputs in scripts/generate-round4.ts).
    ...renderRound4(),
  }
}

/** Write `content` to `path` through a sibling temp file and a rename. */
export function writeAtomic(path: string, content: string): void {
  const temp = `${path}.${process.pid}.tmp`
  writeFileSync(temp, content)
  renameSync(temp, path)
}

if (import.meta.main) {
  const files = render()
  for (const [rel, content] of Object.entries(files)) {
    const path = join(pkgRoot, rel)
    mkdirSync(dirname(path), { recursive: true })
    const before = existsSync(path) ? readFileSync(path, `utf8`) : null
    // VAPP-103: never rewrite an unchanged file, and write a changed one
    // ATOMICALLY (temp + rename): a test reading a fixture while the
    // generator runs sees the old file or the new one, never half of it.
    if (before !== content) writeAtomic(path, content)
    console.log(`${before === content ? `unchanged` : `wrote    `} ${rel}`)
  }
}
