import { TRPCError } from "@trpc/server"
import {
  ACTION_LIMITS,
  PACKAGE_TEST_LIMITS,
  EXPOSES_LIMITS,
  MAX_SCREENS,
  MAX_SCREEN_TITLE,
  ACCEPT_V09,
  PACKAGE_FORMAT_VERSION,
  PACKAGE_FORMAT_VERSION_09,
  bridgePackage,
  availableExtensions,
  defineExtension,
  packageScreens,
  supportedCatalogIds,
  validatePackage,
  type AppPackage,
  type ExtensionDef,
} from "@exponential_app/eui"
import { catalogIndex, catalogPrompt, componentPrompt } from "@exponential_app/eui/prompt"
import extensionJson from "@exp/ui/exponential-ui/extension.json"
import { APP_PACKAGE_MAX_BYTES, APP_RECORD_KINDS_MAX, APP_RECORD_MAX_BYTES, APP_TEMPLATE_MAX_COMPONENTS, APPS_SCREENS } from "@exp/domain-contract/apps"
import { APP_RECORD_ROW_KEYS } from "@exp/domain-contract/app-records"
import { APP_ACTIONS, storedAction } from "@exp/domain-contract/app-actions"
import { validateExposes, type Expose } from "@exp/emcp/exposes"
import { UPSTREAM_BINDING, upstreamDeclareIssues } from "@exp/emcp/upstream"
import { AUTOMATIONS_LIMITS } from "@exp/emcp/automations"
import type { RecordSchema } from "@exponential_app/eui"

// VAPP-110: what an app's PACKAGE may be, server-side (tRPC `apps.upsertPackage`
// and MCP `apps upsert` both land here). The SDK's `validatePackage` checks the
// format against the catalogs every client renders (the core, its lite subset,
// A2UI basic and the Exponential app extension); the contract caps its bytes
// and the components per template. A refusal is BAD_REQUEST naming the fix.

/** The Exponential app extension (the app's own natives + macros), validated. */
export const appExtension: ExtensionDef = defineExtension(extensionJson as unknown as ExtensionDef)

/** The catalog ids a package may name. */
export const APP_CATALOG_IDS: readonly string[] = supportedCatalogIds([appExtension.id])
/** VAPP-161: what a package's `extensions` may declare (the app extension). */
const APP_EXTENSIONS = availableExtensions([appExtension])

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

export function packageBytes(pkg: unknown): number {
  return new TextEncoder().encode(JSON.stringify(pkg)).byteLength
}

type PackageIssue = { path: string; message: string }

/** Every rule a stored package must pass, in order: the shape and bytes, the
 *  SDK's validator (`sdk` = its issue list), then the app hosts' own rules
 *  (the first one wins, like the throwing form always did). */
function checkPackage(pkg: unknown, acceptV09: boolean = ACCEPT_V09): { sdk: PackageIssue[]; rules: PackageIssue[] } {
  const none = { sdk: [], rules: [] }
  if (!pkg || typeof pkg !== `object` || Array.isArray(pkg)) {
    return { ...none, rules: [{ path: `/`, message: `package must be an object {id, name, version, catalogId, templates}` }] }
  }
  const bytes = packageBytes(pkg)
  if (bytes > APP_PACKAGE_MAX_BYTES) {
    return { ...none, rules: [{ path: `/`, message: `package is ${bytes} bytes; the cap is ${APP_PACKAGE_MAX_BYTES}: simplify it` }] }
  }
  // Round 9 §14: a stored package syncs to every client, and a released one
  // reads format 5 alone.
  if (acceptV09 && (pkg as { formatVersion?: unknown }).formatVersion === PACKAGE_FORMAT_VERSION) {
    return { ...none, rules: [{ path: `/formatVersion`, message: `format ${PACKAGE_FORMAT_VERSION} is not stored yet: write formatVersion ${PACKAGE_FORMAT_VERSION_09}` }] }
  }
  // Round 9 §12: format 5 is what the server stores until the flag day; every
  // check reads its upgraded form, what every client runs.
  const upgraded = acceptV09 ? bridgePackage(pkg) : pkg
  const sdk = validatePackage(upgraded, APP_CATALOG_IDS, APP_EXTENSIONS).map((issue) => ({ path: issue.path || `/`, message: issue.message }))
  if (sdk.length > 0) return { ...none, sdk }
  const valid = upgraded as AppPackage
  const rules: PackageIssue[] = []
  // VAPP-107: an app's bindings follow only what it declares, and only the
  // sources every client's app host resolves (the SDK treats a missing list
  // as "anything"; a stored app may not).
  if (!Array.isArray(valid.sources)) {
    rules.push({ path: `/sources`, message: `package must declare sources: the sources its bindings follow, e.g. ["exp:issues", "records:*"] (or [] for none)` })
  } else {
    const unknown = valid.sources.filter((source) => !source.startsWith(`records:`) && !source.startsWith(`${UPSTREAM_BINDING.scheme}:`) && !APP_SOURCE_NAMES.some((name) => source === name || source.startsWith(`${name}?`)))
    if (unknown.length > 0) {
      rules.push({ path: `/sources`, message: `sources ${unknown.join(`, `)} are not resolved by Exponential's app hosts; use ${APP_SOURCE_NAMES.join(`, `)}` })
    }
  }
  // #VAPP-230: an `upstream:` binding (and source) names one of the package's upstreams.
  for (const [id, template] of Object.entries(valid.templates)) {
    const bound = (template.bindings ?? []).map((b) => b.source)
    for (const issue of upstreamDeclareIssues(bound, valid.upstreams ?? [])) {
      rules.push({ path: `/templates/${id}/bindings/${issue.index}/source`, message: upstreamDeclareMessage(issue.code, bound[issue.index]!) })
    }
  }
  // (`upstream:*` = any declared server; the reads check the server again.)
  const listed = (valid.sources ?? []).map((source) => (source === `${UPSTREAM_BINDING.scheme}:*` ? `` : source))
  for (const issue of upstreamDeclareIssues(listed, valid.upstreams ?? [])) {
    rules.push({ path: `/sources/${issue.index}`, message: upstreamDeclareMessage(issue.code, listed[issue.index]!) })
  }
  for (const [id, template] of Object.entries(valid.templates)) {
    if (template.components.length > APP_TEMPLATE_MAX_COMPONENTS) {
      rules.push({ path: `/templates/${id}/components`, message: `template ${id} has ${template.components.length} components; the cap is ${APP_TEMPLATE_MAX_COMPONENTS}` })
    }
  }
  // #EXP-1674: a link's query never carries the editor's own keys, so a
  // contract naming one could never be met.
  ;(valid.screens ?? []).forEach((screen, index) => {
    for (const key of Object.keys(screen.exposed?.params ?? {}).sort()) {
      if ((APPS_SCREENS.reserved as string[]).includes(key)) rules.push({ path: `/screens/${index}/exposed/params/${key}`, message: `${key} is reserved in a link's query: name this param differently` })
    }
  })
  // VAPP-161: the format's tools need a host that runs MCP servers and
  // scripts; Exponential's app hosts run none (an app calls the typed
  // exponential.<noun>.<op> functions or harness.mcp instead).
  if (valid.tools !== undefined) {
    rules.push({ path: `/tools`, message: `Exponential's app hosts run no package tools: call exponential.<noun>.<op> or harness.mcp instead` })
  }
  // VAPP-180: the SDK checked the format of `exposes`; eMCP's contract adds
  // what only this host knows: an action binding names an OFFERED, grantable
  // exponential.<noun>.<op> whose call is not destructive by its arguments.
  if (valid.exposes !== undefined) {
    const ctx = { kinds: Object.keys(valid.records ?? {}), upstreams: valid.upstreams ?? [] }
    for (const error of validateExposes(valid.exposes as unknown as Expose[], ctx)) {
      const message = EXPOSE_HOST_MESSAGES[error.code]
      if (message && error.index !== null) rules.push({ path: `/exposes/${error.index}/run/action`, message })
    }
    // VAPP-181: the broker never chains tools across apps, so an exposed tool
    // never reaches an app (its own included) through exponential.apps.*.
    valid.exposes.forEach((expose, index) => {
      if (`action` in expose.run && expose.run.action.startsWith(`exponential_apps_`)) {
        rules.push({ path: `/exposes/${index}/run/action`, message: `an exposed tool never reaches an app through exponential.apps.*: read or write this app's records with a records binding` })
      }
    })
  }
  // #EXP-1829 (ONE trigger system): automations are the app AGENT's
  // triggers now (`agents.triggers`), never part of the package.
  if ((valid as { automations?: unknown }).automations !== undefined) {
    rules.push({ path: `/automations`, message: AUTOMATIONS_MOVED })
  }
  // VAPP-122: the SDK checked every kind's schema; the contract caps how many.
  const kinds = Object.keys(valid.records ?? {})
  if (kinds.length > APP_RECORD_KINDS_MAX) {
    rules.push({ path: `/records`, message: `records declares ${kinds.length} kinds; the cap is ${APP_RECORD_KINDS_MAX}` })
  }
  return { sdk: [], rules }
}

function upstreamDeclareMessage(code: string, source: string): string {
  return code === UPSTREAM_BINDING.errors.invalid
    ? `${source} is not an upstream:<server>/<tool> source`
    : `${source} reads an upstream the package does not declare: add its server to upstreams`
}

/** VAPP-180: the eMCP codes the SDK leaves to the host (an action binding
 *  over Exponential's typed operations). */
const EXPOSE_HOST_MESSAGES: Record<string, string> = {
  action: `an exposed action is one of the exponential.<noun>.<op> operations (exponential_apps {op: "catalog"} lists them)`,
  ownerOnly: `an owner-only operation is never grantable, so never exposed`,
  destructive: `a destructive call (a delete, merge, close, end, remove or trash, or sessions.send with stop) is never exposed`,
}

/** #EXP-1829: `automations` left the package. */
export const AUTOMATIONS_MOVED = `automations are the app agent's triggers now: set them with exponential_agents_write {op: "update", id: <the app's agentId>, triggers} (apps catalog, section automations)`

/** VAPP-180: a package's `upstreams` name MCP servers of the app's team
 *  (Settings › MCP); unknown names refuse with the ones the team has. A
 *  server removed later only makes its bindings answer upstreamMissing.
 *  #VAPP-254: `known` = names mapped for this app or already declared by it
 *  (upstream.json `mapping`). */
export function upstreamIssues(pkg: AppPackage, teamServers: readonly string[], known: readonly string[] = []): PackageIssue[] {
  const unknown = (pkg.upstreams ?? []).filter((name) => !teamServers.includes(name) && !known.includes(name))
  if (unknown.length === 0) return []
  const have = teamServers.length > 0 ? `the team has ${[...teamServers].sort().join(`, `)}` : `the team has no MCP servers yet (Settings › MCP)`
  return [{ path: `/upstreams`, message: `upstreams ${unknown.join(`, `)} are not MCP servers of this team: ${have}` }]
}

/** VAPP-121: everything wrong with a package as `{path, message}` rows (the
 *  import step lists them inline); empty = valid. */
export function appPackageIssues(pkg: unknown, acceptV09: boolean = ACCEPT_V09): PackageIssue[] {
  const { sdk, rules } = checkPackage(pkg, acceptV09)
  return [...sdk, ...rules]
}

/** Round 9 §14: what the stored-data migration asks of an upgraded package:
 *  what every client's load checks (the SDK's validator, no 0.9 bridge) and
 *  the contract's caps. The upsert rules stay out: a stored package that a
 *  later rule would refuse still runs, and format 5 stops loading on the flag
 *  day. */
export function migratedPackageIssues(pkg: unknown): PackageIssue[] {
  const bytes = packageBytes(pkg)
  if (bytes > APP_PACKAGE_MAX_BYTES) return [{ path: `/`, message: `the upgraded package is ${bytes} bytes; the cap is ${APP_PACKAGE_MAX_BYTES}` }]
  const issues = validatePackage(pkg, APP_CATALOG_IDS, APP_EXTENSIONS).map((issue) => ({ path: issue.path || `/`, message: issue.message }))
  if (issues.length > 0) return issues
  for (const [id, template] of Object.entries((pkg as AppPackage).templates)) {
    if (template.components.length > APP_TEMPLATE_MAX_COMPONENTS) issues.push({ path: `/templates/${id}/components`, message: `template ${id} has ${template.components.length} components; the cap is ${APP_TEMPLATE_MAX_COMPONENTS}` })
  }
  return issues
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
 *  app (web, desktop, iOS, Android alike): the `exp:` sources, the format's
 *  `records:<kind>` and the functions the package's allowlist may name. */
export const APP_HOST_SOURCES: readonly { source: string; value: string }[] = [
  { source: `exp:devices`, value: `{rows: [{id, label, detail, icon, discTone, tone, status}], count, online}: the team's devices, live` },
  {
    source: `exp:issues?board=<id|slug>&status=<anchor>&priority=<p>&assignee=<userId|me|none>&search=<text>&touched=me|<userId>&since=<YYYY-MM-DD|ISO instant>&until=<same>&sort=sortOrder|updatedAt|createdAt|touchedAt&order=asc|desc&limit=<n>`,
    value: `{rows: [{id, identifier, title, status, priority, assignee, assigneeName, board, boardSlug, dueDate, updatedAt, value, label, touchedAt?, touches?}], count}: the team's issues (every filter optional), live; value + label make a row a Select option; touched = only the issues that person touched (an event they acted, a comment they wrote, a session they started linked to it) between since and until (a bare date = that whole day in the team time zone), each row with touchedAt (latest touch) + touches (comment | event | session), newest touch first unless sort is given`,
  },
  { source: `exp:boards`, value: `{rows: [{id, slug, name, prefix, icon, color, value, label}], count}: the team's boards in sidebar order` },
  { source: `exp:members`, value: `{rows: [{id, name, email, avatar, role, value, label}], count, me}: the team's members by name; me = the viewer's user id` },
  { source: `exp:sessions?status=<status>&mine=1&limit=<n>`, value: `{rows: [{id, title, status, busy, needsInput, kind: thread|agent, updatedAt}], count}: the team's root sessions and agent conversations, newest first` },
  { source: `exp:me`, value: `{id, name, email, avatar, role, access}: the viewer; role = their team role (owner | member), access = their app access (read | write). Bind it to read in visible, e.g. {call: "eq", args: {a: {path: "/me/role"}, b: "owner"}}` },
  { source: `exp:comments?issue=<identifier>&limit=<n>`, value: `{rows: [{id, body, author, authorName, parent, source, createdAt}], count}: an issue's comments, oldest first (null without the issue)` },
  { source: `exp:attachments?issue=<identifier>&limit=<n>`, value: `{rows: [{id, name, type, size, url, width, height, uploader, uploaderName, createdAt}], count}: an issue's files; url loads through the media policy (Image src)` },
  {
    source: `records:<kind>?mine=1&sort=<field>&order=asc|desc&limit=<n>&sum=<field>&by=<dateField>&month=YYYY-MM&<field>=<v>&<field>.gte=<v>`,
    value: `{rows: [{…data, id, createdAt, updatedAt, createdBy, authorId, author}], count, sum?, month?: {key, label, prev, next, days: [{date, day, weekday, count, sum}]}}: the app's OWN records of a kind, live (declare "records:*" or "records:<kind>" in sources; mine = the viewer's rows (by author); any other key filters a data field: equals, or .gt .gte .lt .lte; sum = Σ of a number field; by + month = one month's rows and every day of it; app=<slug> reads another app's records)`,
  },
  {
    source: `upstream:<server>/<tool>?<arg>=<v>`,
    value: `the tool's answer (its structuredContent, else its text as JSON or text): a READ-ONLY tool of a team MCP server named in upstreams, run on the server as the viewer with their own connection, cached 60 s (declare "upstream:<server>/<tool>" or "upstream:<server>/*" in sources; args read as JSON when they are numbers, true/false/null, objects or arrays; a tool without readOnlyHint reads only when sources lists it exactly; not connected = the path keeps its data default and the host shows "Connect <server>")`,
  },
]
/** The source names (`scheme:name`) the app hosts resolve ×4 (`records:<kind>`
 *  = any kind, declared as `records:*` or `records:<kind>`). */
export const APP_SOURCE_NAMES: readonly string[] = APP_HOST_SOURCES.map((s) => s.source.split(`?`)[0]!)

export const APP_HOST_FUNCTIONS: readonly { name: string; args: string; effect: string }[] = [
  { name: `harness.toast`, args: `{message, tone?: "error"}`, effect: `shows a toast` },
  { name: `harness.mcp`, args: `{tool, arguments}`, effect: `a tools/call on Exponential's MCP as the viewer, after the person allows it` },
  { name: `records.upsert`, args: `{kind, data, id?}`, effect: `creates a record of the app (or, with id, patches that record's data); no consent card: the app's own rows` },
  { name: `records.delete`, args: `{id}`, effect: `deletes one record of the app` },
]

/** #EXP-1674: `apps get`'s `linkableScreens`: what can be linked to in a
 *  package: its navigation screens and its exposed ones, each with its params
 *  and a ready `~/` link (`<key>` where a required param goes). A nav screen
 *  that is not exposed opens from a person's or an agent's link, not another
 *  app's. */
export function linkableScreens(appSlug: string, pkg: unknown): { screen: string; title: string; nav: boolean; exposed: boolean; description?: string; params: Record<string, { required: boolean; description?: string }>; link: string }[] {
  const app = encodeURIComponent(appSlug)
  return packageScreens((pkg ?? {}) as { screens?: unknown })
    .filter((screen) => screen.nav !== false || screen.exposed)
    .map((screen) => {
      const declared = screen.exposed?.params ?? {}
      const params = Object.fromEntries(
        Object.keys(declared)
          .sort()
          .map((key) => [key, { required: declared[key]?.required === true, ...(declared[key]?.description ? { description: declared[key]!.description } : {}) }])
      )
      const query = Object.keys(params)
        .filter((key) => params[key]!.required)
        .map((key) => `${key}=<${key}>`)
        .join(`&`)
      return {
        screen: screen.template,
        title: screen.title,
        nav: screen.nav !== false,
        exposed: Boolean(screen.exposed),
        ...(screen.exposed ? { description: screen.exposed.description } : {}),
        params,
        link: `~/apps/${app}/${encodeURIComponent(screen.template)}${query ? `?${query}` : ``}`,
      }
    })
}

/** S-3 MCP `apps catalog {only}`: just the named components' props (what
 *  an `exponential_show` surface author asks for; the same props in the
 *  nested node form {id, component, props, children}). */
export function componentCatalogText(names: readonly string[]): string {
  return names
    .map((name) => componentPrompt(name, { extensions: [appExtension] }) || `${name}: no such component`)
    .join(`\n`)
}

/** #FEED-218: the catalog's sections, in the full text's order (`apps catalog {sections}`); `index` = one line per component instead of `components`' full props. */
export const APP_CATALOG_SECTIONS = [`package`, `screens`, `sources`, `functions`, `records`, `queries`, `actions`, `tests`, `expressions`, `exposes`, `automations`, `components`, `index`] as const
export type AppCatalogSection = (typeof APP_CATALOG_SECTIONS)[number]

/** Each section's text (the full catalog = every section but `index`, joined by a blank line). */
function appCatalogSections(): Record<AppCatalogSection, string> {
  const sources = APP_HOST_SOURCES.map((s) => `- ${s.source} → ${s.value}`).join(`\n`)
  const functions = APP_HOST_FUNCTIONS.map((f) => `- ${f.name}(${f.args}): ${f.effect}`).join(`\n`)
  return {
    package: [
      `# App package`,
      `A package = {formatVersion: ${PACKAGE_FORMAT_VERSION_09} (REQUIRED, exactly this), id, name, version, catalogId: "${appExtension.id}" (the Exponential app extension over the core catalog; "${APP_CATALOG_IDS[0]}" = the core alone), templates: {<templateId>: {components: [flat components, one with id "root"], data?: the initial data model, bindings?: [{path: "/json/pointer", source: "exp:…" | "records:<kind>?…"}]}}, functions?: [the host functions its surfaces may call: a name or a prefix like "records.*"], sources: [the sources its bindings follow ("exp:issues", "records:*"…), REQUIRED, [] for none: a binding to an undeclared source never resolves], theme?: a built-in theme id, icon?: {glyph, color}, description?, screens?}. Without screens the app is its FIRST template. Components are FLAT: every child is an id, children: [ids] or {componentId, path, key?} for a bound list.`,
      `Cap: ${APP_PACKAGE_MAX_BYTES} bytes per package, ${APP_TEMPLATE_MAX_COMPONENTS} components per template.`,
    ].join(`\n`),
    screens: [
      `# Screens (the app's pages)`,
      `screens: [{template, title (≤ ${MAX_SCREEN_TITLE}), icon?: an icon name, params?: {<k>: string} (opened as /params/*), nav?: false, exposed?: {description, params?: {<k>: {required?, description?}}}, widget?: {template, size: "s" | "m" | "l"}}]: a screen = one template, named by its template id; templates no screen names are parts (widgets, rows). Array order = the navigation's order; ≤ ${MAX_SCREENS} without nav: false; nav: false = a detail screen only app.navigate or a link opens. The HOST draws the navigation, never the app: sidebar rows on wide frames (desktop, web, the editor's Desktop preview), a bottom tab bar on phones (the Phone preview). Each screen draws only its content; a tile opens a detail screen with on: {press: {functionCall: {call: "app.navigate", args: {screen: "income"}}}} (StatTile, Card, Row).`,
    ].join(`\n`),
    sources: [
      `# Bindings (sources the clients resolve, written into the data model at path)`,
      sources,
    ].join(`\n`),
    functions: [
      `# Host functions (on: {press: {functionCall: {call, args}}}; list the ones you use in functions)`,
      functions,
      `- exponential.<noun>.<op>({…the op's fields}): VAPP-120: every Exponential MCP operation as a typed function, run as the viewer (${APP_ACTIONS.length} names; list what you call in functions, exactly or as exponential.<noun>.*): ${APP_ACTIONS.map((n) => storedAction(n).slice(`exponential.`.length)).join(`, `)}. A call asks the person with the operation's own words unless a team owner granted that action on the app's settings page (then it runs without asking); a destructive operation (delete, merge, close, end, remove, trash) can never be granted and always asks; harness.mcp stays the raw escape hatch and always asks.`,
      `- openUrl({url}), Link.href, Row.href, Button.href: a link through the URL policy; ~/x = the app's team (~/issues/<identifier>, ~/devices, ~/apps/<slug>/<screen>?k=v). href is a literal; a link per list row = on: {press: {functionCall: {call: "openUrl", args: {url: {call: "formatString", args: {value: "~/issues/\${identifier}"}}}}}}. A link at ANOTHER app opens only a screen it declares exposed, with its required params; exponential_apps get lists an app's linkableScreens. app.navigate({screen, params?}) = this app's own screens (a screen with nav: false stays out of the navigation)`,
      `- bind({path, source, params?}): a built-in: re-follows a data model path from a source with params appended as its query (the month switcher: bind {path: "/entries", source: "records:entry?by=date", params: {month: {path: "/entries/month/prev"}}}); list it in functions too`,
    ].join(`\n`),
    records: [
      `# Records (the app's own data)`,
      `Declare every kind in the manifest: records: {<kind>: {title?, policy?: {read?, create?, update?, delete?: "member" | "author" | "owners"} (who may: any member, the record's author or a team owner, team owners only; create never author; default member/member/author/author; read: author keeps a kind private to its author, e.g. personal time entries), schema: {type: "object", properties: {<field>: {type: string|number|integer|boolean|array, enum?, items? ({type}), maxLength?, minimum?, maximum?, format?: date|date-time|time, description?}}, required?: [fields], additionalProperties?: false}}} (a kind is lowercase letters, digits, underscores; ${APP_RECORD_KINDS_MAX} kinds at most; a record's data at most ${APP_RECORD_MAX_BYTES} bytes; the keys ${APP_RECORD_ROW_KEYS.join(`, `)} are reserved). Read with the records:<kind> binding, write with records.upsert (bind a Form's fields to /draft/*, then call it with data: {path: "/draft"}; a row's Checkbox: id: {path: "id"}, data: {done: {path: "done"}}) and records.delete. A write is validated against the kind's schema and lands in every client's binding live. Seed or inspect records yourself with exponential_apps {op: "records_upsert" | "records_list" | "records_delete", slug, kind, data}. Every member id an app stores or compares (a record's author, me, exp:members rows, a test's as) is the USER id (exponential_people id, never its memberId); records_upsert's author also takes a memberId or email.`,
    ].join(`\n`),
    queries: [
      `# Named queries (reports: grouping, totals, running sums)`,
      `queries: {<name>: {from: "<kind>", where?: [[field, "eq" | "gt" | "gte" | "lt" | "lte" | "in" | "between", value(s)]…] (+ one {or: [[…]…]}), group?: [field | "author" | {bucket: <date field>, by: "day" | "week" | "month" | "quarter" | "year"}], aggregate?: {<out>: ["sum" | "avg" | "min" | "max" | "count" | "countDistinct" | "first" | "last", field?]}, running?: {<out>: "sum" | "count" | "avg" | "min" | "max"}, before?: <date>, sort?: [[field, "asc" | "desc"]], limit?: ≤ 500}}; a binding follows one with source "records:<kind>?query=<name>&<param>=…" (string values may hold \${param}); its value gains groups: [{key…, <out>…}], totals: {<out>…} and before. Prefer a named query over hand-summing rows in expressions, and test it with a query test.`,
    ].join(`\n`),
    actions: [
      `# Actions (all-or-nothing writes)`,
      `actions: {<name>: {params?: a records schema {type: "object", properties, required?}, steps: [...]}} (≤ ${ACTION_LIMITS.actions} actions, ≤ ${ACTION_LIMITS.steps} steps): a press runs one with on: {press: {functionCall: {call: "actions.<name>", args: {<param>: a value or {path}}}}}. Steps, in order: {require: "<CEL bool>", message: "<the toast when false>"}, {"records.upsert": {kind, data: {<field>: "<CEL>"}, id?: "<CEL>"}}, {"records.delete": {id: "<CEL>"}}, {set: {path: "/pointer", value: "<CEL>"}}, {call: "harness.toast" (a host function listed in functions), args: {<k>: "<CEL>"}}. A string in a step is CEL (quote text: "'Saved'"), anything else a literal; CEL reads params, data (the surface data at the press) and me (the viewer's id). Every step reads the SAME snapshot (a set never changes a later step); the first false require, error or refused write discards the whole action and nothing lands (the writes are one transaction). Bindings are pure: a write or host function anywhere but an on.* functionCall is refused on upsert.`,
    ].join(`\n`),
    tests: [
      `# Tests (run on every upsert)`,
      `tests: [{name, records?: {<kind>: [rows: data + optional id, author (default "member", never the test's as, so name the author of a row another member reads)]}, now?: ISO instant (UTC), as?: a user id, role?: "member" | "owner" (the test reads and writes under the kinds' policies), expr: {expr, vars?} | query: {name, params?} | action: {name, params?, data?}, expect}] (≤ ${PACKAGE_TEST_LIMITS.tests}). Each runs on its own sample records; a var {source: "records:<kind>?…"} reads them like a binding; an action's result is {records: {<kind>: [{…data, id, author}]}, sets, calls} or {error}. expect matches by the keys an object names (lists item by item). A failing test refuses the upsert as /tests/<i> (expected vs got): add tests for every expression, query and action you write.`,
    ].join(`\n`),
    expressions: [
      `# Expressions (computed values, live)`,
      `{call: "expr", args: {expr: "<CEL>", vars: {<name>: a value, a binding {path} or a call}, default: <shown when it fails, REQUIRED>}} wherever a value may be bound: a CEL subset over vars (number, string, bool, null, list and {'k': e} map literals; a.b, a[0]; + - * / %; == != < <= > >=; && || !; c ? a : b; list.map/filter/all/exists(x, e), one level). ONE number type (7 / 2 = 3.5), no implicit conversion, a missing field is null, an error (1 / 0) gives no value. Functions (also callable directly as {call: "<name>", args: {…}}): mul|div|mod(a, b), round(value, digits?) (half away from zero), floor|ceil|abs(value), min|max(values…), today(), now(), dateAdd(date, n, unit), dateDiff(a, b, unit) (b - a), startOf|endOf(date, unit), weekday(date) (Monday 1), isoWeek(date), hours(minutes), formatDuration(hours, "h:mm" | "hh:mm"), CEL only: size(x), string(x), trim|lower|upper(s), split(s, sep), join(list, sep?), sumOf|avgOf|minOf|maxOf|countOf(list), sortBy(list, field, order?), groupOf(list, field). rows.map(r, {'value': r.id, 'label': r.name}) makes rows Select options. Units: second minute hour day week month quarter year; a date is YYYY-MM-DD, a timestamp an ISO instant; dates run in the team time zone; no working days or holidays (keep those as records). Caps: 1000 characters, no macro inside a macro, 10 000 steps. A field not in its kind's required list (and a var bound to anything untyped) may be missing: guard it with has(r.f) ? … : …, r.f != null, or orElse(r.f, default), else the upsert is refused naming the path. Every expr is type-checked on upsert against its vars (a records binding's kind schema, the template data): e.g. vars {entries: {path: "/entries"}} bound to records:entry, expr "round(entries.sum / 60, 1)" or "entries.rows.filter(r, orElse(r.date, '') >= startOf(today(), 'week')).map(r, orElse(r.hours, 0))".`,
    ].join(`\n`),
    exposes: [
      `# Exposed MCP tools (exposes: the app's tools on Exponential's MCP, /api/mcp/apps)`,
      `exposes: [{name: lowercase_snake (≤ ${EXPOSES_LIMITS.nameMax}, no __), title, description, input: a JSON Schema {type: "object", properties, required?}, access: "read" | "write", run}] (≤ ${EXPOSES_LIMITS.maxTools} tools). A connected MCP client (Claude, ChatGPT…) sees them as <app slug>__<name> and runs them as the CALLING member (attributed "via <app>"); a read-only connection never lists a write tool. run = EXACTLY ONE of: {records: {op: "list" | "get" | "upsert", kind, id?, where?, data?, limit?}} (the app's own records of a declared kind); {action: "exponential.<noun>.<op>", args} (one of the operations above that a team owner could grant: never a destructive or owner-only one; it reads when its last word is list/get/show/catalog/activity/files, else it writes); {upstream: "<server>", tool, args} (a tool of a team MCP server named in upstreams: ["<server>"], with the member's own connection; Settings › MCP lists the team's servers). Every string in run may hold \${input.<field>} (a declared input property; a string that is exactly one template keeps the value's type). Destructive tools (delete, merge, close, end, remove, trash) are never exposed, and a "read" tool may not write. Package tools (\`tools\`) stay refused: expose bindings instead.`,
    ].join(`\n`),
    automations: [
      `# Triggers (the app's automations = its agent's triggers; never in the package)`,
      `The app's AGENT (exponential_agents get, its id = the app's agentId) holds the triggers: exponential_agents_write {op: "update", id, triggers} REPLACES the whole list (keep each kept trigger's id from get). A trigger = {kind, enabled?, prompt?, run?} + its when-part: {kind: "schedule", cron: "m h dom mon dow", tz?} (UTC unless tz names an IANA zone), {kind: "event", name} (the team event stream: issue.created, issue.status_changed, issue.assigned, pr.opened, pr.merged, session.ended, or any events_emit name), {kind: "record", record: "<kind>", op: "create" | "update" | "delete"} (a write of the app's own records), {kind: "webhook", name: lowercase_snake} (a signed POST to /api/apps/hooks/<appId>/<name>; an owner mints its secret). By default a firing WAKES the agent (you): a card in its owner's chat, prompt = its text. With run (an exposes run: records, action, upstream, or {agent: "<another agent's slug>", prompt}; never destructive, an action must be granted by a team owner) it runs that ONE binding on the server instead, no session: on the PUBLISHED version, as the agent's owner (paused while they lack write access; the owners are told); a run's schedule fires at most every ${AUTOMATIONS_LIMITS.minScheduleGapMinutes} minutes. prompt and run strings may hold \${record.<field>} (record triggers: the record's data + id), \${event.<field>} (event and webhook triggers) and \${firedAt}. No chains: what a run writes or emits never fires a trigger; at most ${AUTOMATIONS_LIMITS.runsPerMinute} firings a minute per app; every firing is audited (Settings › MCP › Activity). ≤ 16 triggers. Judgement belongs to the agent (a wake), not a run.`,
    ].join(`\n`),
    components: [
      catalogPrompt({ extensions: [appExtension] }),
    ].join(`\n`),
    index: [`# genUI components (one line each; ask exponential_apps {op: "catalog", only: [names]} for their props)`, ...catalogIndex({ extensions: [appExtension] }).map((line) => `- ${line}`)].join(`\n`),
  }
}

/** MCP `apps catalog`: everything an agent needs to write a package, or just the named `sections` (in the catalog's order). */
export function appCatalogText(sections?: readonly string[]): string {
  const all = appCatalogSections()
  const wanted = sections?.length ? APP_CATALOG_SECTIONS.filter((name) => sections.includes(name)) : APP_CATALOG_SECTIONS.filter((name) => name !== `index`)
  const unknown = (sections ?? []).filter((name) => !(APP_CATALOG_SECTIONS as readonly string[]).includes(name))
  const text = wanted.map((name) => all[name]).join(`\n\n`)
  return unknown.length ? [text, `No such section: ${unknown.join(`, `)} (sections: ${APP_CATALOG_SECTIONS.join(`, `)})`].filter(Boolean).join(`\n\n`) : text
}
