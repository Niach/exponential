import { TRPCError } from "@trpc/server"
import {
  defineExtension,
  supportedCatalogIds,
  validatePackage,
  type AppPackage,
  type ExtensionDef,
} from "@exponential_app/ui"
import { catalogPrompt } from "@exponential_app/ui/prompt"
import extensionJson from "@exp/ui/exponential-ui/extension.json"
import { APP_PACKAGE_MAX_BYTES, APP_RECORD_KINDS_MAX, APP_RECORD_MAX_BYTES, APP_TEMPLATE_MAX_COMPONENTS } from "@exp/domain-contract/apps"
import { APP_RECORD_ROW_KEYS } from "@exp/domain-contract/app-records"
import { APP_ACTIONS } from "@exp/domain-contract/app-actions"
import type { RecordSchema } from "@exponential_app/ui"

// VAPP-110: what an app's PACKAGE may be, server-side (tRPC `apps.upsertPackage`
// and MCP `apps upsert` both land here). The SDK's `validatePackage` checks the
// format against the catalogs every client renders (the core, its lite subset,
// A2UI basic and the Exponential app extension); the contract caps its bytes
// and the components per template. A refusal is BAD_REQUEST naming the fix.

/** The Exponential app extension (the app's own natives + macros), validated. */
export const appExtension: ExtensionDef = defineExtension(extensionJson as unknown as ExtensionDef)

/** The catalog ids a package may name. */
export const APP_CATALOG_IDS: readonly string[] = supportedCatalogIds([appExtension.id])

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

export function packageBytes(pkg: unknown): number {
  return new TextEncoder().encode(JSON.stringify(pkg)).byteLength
}

type PackageIssue = { path: string; message: string }

/** Every rule a stored package must pass, in order: the shape and bytes, the
 *  SDK's validator (`sdk` = its issue list), then the app hosts' own rules
 *  (the first one wins, like the throwing form always did). */
function checkPackage(pkg: unknown): { sdk: PackageIssue[]; rules: PackageIssue[] } {
  const none = { sdk: [], rules: [] }
  if (!pkg || typeof pkg !== `object` || Array.isArray(pkg)) {
    return { ...none, rules: [{ path: `/`, message: `package must be an object {id, name, version, catalogId, templates}` }] }
  }
  const bytes = packageBytes(pkg)
  if (bytes > APP_PACKAGE_MAX_BYTES) {
    return { ...none, rules: [{ path: `/`, message: `package is ${bytes} bytes; the cap is ${APP_PACKAGE_MAX_BYTES}: simplify it` }] }
  }
  const sdk = validatePackage(pkg, APP_CATALOG_IDS).map((issue) => ({ path: issue.path || `/`, message: issue.message }))
  if (sdk.length > 0) return { ...none, sdk }
  const valid = pkg as AppPackage
  const rules: PackageIssue[] = []
  // VAPP-107: an app's bindings follow only what it declares, and only the
  // sources every client's app host resolves (the SDK treats a missing list
  // as "anything"; a stored app may not).
  if (!Array.isArray(valid.sources)) {
    rules.push({ path: `/sources`, message: `package must declare sources: the exp: sources its bindings follow, e.g. ["exp:issues"] (or [] for none)` })
  } else {
    const unknown = valid.sources.filter((source) => !APP_SOURCE_NAMES.some((name) => source === name || source.startsWith(`${name}?`)))
    if (unknown.length > 0) {
      rules.push({ path: `/sources`, message: `sources ${unknown.join(`, `)} are not resolved by Exponential's app hosts; use ${APP_SOURCE_NAMES.join(`, `)}` })
    }
  }
  for (const [id, template] of Object.entries(valid.templates)) {
    if (template.components.length > APP_TEMPLATE_MAX_COMPONENTS) {
      rules.push({ path: `/templates/${id}/components`, message: `template ${id} has ${template.components.length} components; the cap is ${APP_TEMPLATE_MAX_COMPONENTS}` })
    }
  }
  // VAPP-122: the SDK checked every kind's schema; the contract caps how many.
  const kinds = Object.keys(valid.records ?? {})
  if (kinds.length > APP_RECORD_KINDS_MAX) {
    rules.push({ path: `/records`, message: `records declares ${kinds.length} kinds; the cap is ${APP_RECORD_KINDS_MAX}` })
  }
  return { sdk: [], rules }
}

/** VAPP-121: everything wrong with a package as `{path, message}` rows (the
 *  import step lists them inline); empty = valid. */
export function appPackageIssues(pkg: unknown): PackageIssue[] {
  const { sdk, rules } = checkPackage(pkg)
  return [...sdk, ...rules]
}

/** The package, validated: a JSON object within the caps the SDK's validator
 *  accepts against the app's catalogs. A refusal is BAD_REQUEST naming the fix. */
export function validateAppPackage(pkg: unknown): AppPackage {
  const { sdk, rules } = checkPackage(pkg)
  if (sdk.length > 0) {
    const listed = sdk.slice(0, 10).map((issue) => `${issue.path}: ${issue.message}`)
    const more = sdk.length > 10 ? `; and ${sdk.length - 10} more` : ``
    throw bad(`package is invalid: ${listed.join(`; `)}${more}`)
  }
  if (rules.length > 0) throw bad(rules[0]!.message)
  return pkg as AppPackage
}

/** VAPP-122: a kind's schema out of a package (null = not declared). */
export function recordKindSchema(pkg: unknown, kind: string): RecordSchema | null {
  const records = pkg && typeof pkg === `object` ? (pkg as { records?: Record<string, { schema?: RecordSchema }> }).records : undefined
  return records?.[kind]?.schema ?? null
}

/** The bindings and host functions the Exponential clients resolve for an
 *  app (web, desktop, iOS, Android alike): the `exp:` sources and the
 *  `harness.*` functions the package's allowlist may name. */
export const APP_HOST_SOURCES: readonly { source: string; value: string }[] = [
  { source: `exp:devices`, value: `{rows: [{id, label, detail, icon, discTone, tone, status}], count, online}: the team's devices, live` },
  {
    source: `exp:issues?board=<id|slug>&status=<anchor>&priority=<p>&assignee=<userId|me|none>&search=<text>&sort=sortOrder|updatedAt|createdAt&order=asc|desc&limit=<n>`,
    value: `{rows: [{id, identifier, title, status, priority, assignee, assigneeName, board, boardSlug, dueDate, updatedAt, value, label}], count}: the team's issues (every filter optional), live; value + label make a row a Select option`,
  },
  { source: `exp:boards`, value: `{rows: [{id, slug, name, prefix, icon, color, value, label}], count}: the team's boards in sidebar order` },
  { source: `exp:members`, value: `{rows: [{id, name, email, avatar, role, value, label}], count, me}: the team's members by name; me = the viewer's user id` },
  { source: `exp:sessions?status=<status>&mine=1&limit=<n>`, value: `{rows: [{id, title, status, busy, needsInput, kind: thread|agent, updatedAt}], count}: the team's threads and agent chats, newest first` },
  { source: `exp:comments?issue=<identifier>&limit=<n>`, value: `{rows: [{id, body, author, authorName, parent, source, createdAt}], count}: an issue's comments, oldest first (null without the issue)` },
  { source: `exp:attachments?issue=<identifier>&limit=<n>`, value: `{rows: [{id, name, type, size, url, width, height, uploader, uploaderName, createdAt}], count}: an issue's files; url loads through the media policy (Image src)` },
  {
    source: `exp:records?kind=<kind>&mine=1&sort=<field>&order=asc|desc&limit=<n>&sum=<field>&by=<dateField>&month=YYYY-MM&<field>=<v>&<field>.gte=<v>`,
    value: `{rows: [{…data, id, createdAt, updatedAt, createdBy, author}], count, sum?, month?: {key, label, prev, next, days: [{date, day, weekday, count, sum}]}}: the app's OWN records of a kind, live (kind required; mine = the viewer's rows; any other key filters a data field: equals, or .gt .gte .lt .lte; sum = Σ of a number field; by + month = one month's rows and every day of it; app=<slug> reads another app's records)`,
  },
]
/** The source names (`scheme:name`) the app hosts resolve ×4. */
export const APP_SOURCE_NAMES: readonly string[] = APP_HOST_SOURCES.map((s) => s.source.split(`?`)[0]!)

export const APP_HOST_FUNCTIONS: readonly { name: string; args: string; effect: string }[] = [
  { name: `harness.toast`, args: `{message, tone?: "error"}`, effect: `shows a toast` },
  { name: `harness.openIssue`, args: `{identifier}`, effect: `opens the issue` },
  { name: `harness.openDevice`, args: `{id, label?}`, effect: `opens the device` },
  { name: `harness.mcp`, args: `{tool, arguments}`, effect: `a tools/call on Exponential's MCP as the viewer, after the person allows it` },
  { name: `harness.records.upsert`, args: `{kind, data, id?}`, effect: `creates a record of the app (or, with id, patches that record's data); no consent card: the app's own rows` },
  { name: `harness.records.delete`, args: `{id}`, effect: `deletes one record of the app` },
]

/** MCP `apps catalog`: everything an agent needs to write a package. */
export function appCatalogText(): string {
  const sources = APP_HOST_SOURCES.map((s) => `- ${s.source} → ${s.value}`).join(`\n`)
  const functions = APP_HOST_FUNCTIONS.map((f) => `- ${f.name}(${f.args}): ${f.effect}`).join(`\n`)
  return [
    `# App package`,
    `A package = {id, name, version, catalogId: "${appExtension.id}" (the Exponential app extension over the core catalog; "${APP_CATALOG_IDS[0]}" = the core alone), templates: {<templateId>: {components: [flat components, one with id "root"], data?: the initial data model, bindings?: [{path: "/json/pointer", source: "exp:…"}]}}, functions?: [the host functions its surfaces may call: a name or "harness.*"], sources: [the exp: sources its bindings follow, REQUIRED, [] for none: a binding to an undeclared source never resolves], theme?: a built-in theme id, icon?: {glyph, color}, description?}. The editor renders the FIRST template. Components are FLAT: every child is an id, children: [ids] or {componentId, path, key?} for a bound list.`,
    `Cap: ${APP_PACKAGE_MAX_BYTES} bytes per package, ${APP_TEMPLATE_MAX_COMPONENTS} components per template.`,
    ``,
    `# Bindings (sources the clients resolve, written into the data model at path)`,
    sources,
    ``,
    `# Host functions (on: {press: {functionCall: {call, args}}}; list the ones you use in functions)`,
    functions,
    `- exponential.<noun>.<op>({…the op's fields}): VAPP-120: every Exponential MCP operation as a typed function, run as the viewer (${APP_ACTIONS.length} names; list what you call in functions, exactly or as exponential.<noun>.*): ${APP_ACTIONS.map((n) => n.slice(`exponential.`.length)).join(`, `)}. A call asks the person with the operation's own words unless a team owner granted that action on the app's settings page (then it runs without asking); a destructive operation (delete, merge, close, end, remove, trash) can never be granted and always asks; harness.mcp stays the raw escape hatch and always asks.`,
    `- bind({path, source, params?}): a built-in: re-follows a data model path from a source with params appended as its query (the month switcher: bind {path: "/entries", source: "exp:records?kind=entry&by=date", params: {month: {path: "/entries/month/prev"}}}); list it in functions too`,
    ``,
    `# Records (the app's own data)`,
    `Declare every kind in the manifest: records: {<kind>: {title?, schema: {type: "object", properties: {<field>: {type: string|number|integer|boolean|array, enum?, items? ({type}), maxLength?, minimum?, maximum?, format?: date|date-time|time, description?}}, required?: [fields], additionalProperties?: false}}} (a kind is lowercase letters, digits, underscores; ${APP_RECORD_KINDS_MAX} kinds at most; a record's data at most ${APP_RECORD_MAX_BYTES} bytes; the keys ${APP_RECORD_ROW_KEYS.join(`, `)} are reserved). Read with the exp:records binding, write with harness.records.upsert (bind a Form's fields to /draft/*, then call it with data: {path: "/draft"}; a row's Checkbox: id: {path: "id"}, data: {done: {path: "done"}}) and harness.records.delete. A write is validated against the kind's schema and lands in every client's binding live. Seed or inspect records yourself with exponential_apps {op: "records_upsert" | "records_list" | "records_delete", slug, kind, data}.`,
    ``,
    catalogPrompt({ extensions: [appExtension] }),
  ].join(`\n`)
}
