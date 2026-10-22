import { TRPCError } from "@trpc/server"
import {
  catalogPrompt,
  defineExtension,
  supportedCatalogIds,
  validatePackage,
  type AppPackage,
  type ExtensionDef,
} from "@exponential-at/ui"
import extensionJson from "@exp/ui/exponential-ui/extension.json"
import { APP_PACKAGE_MAX_BYTES, APP_TEMPLATE_MAX_COMPONENTS } from "@exp/domain-contract/apps"

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

/** The package, validated: a JSON object within the caps the SDK's validator
 *  accepts against the app's catalogs. */
export function validateAppPackage(pkg: unknown): AppPackage {
  if (!pkg || typeof pkg !== `object` || Array.isArray(pkg)) {
    throw bad(`package must be an object {id, name, version, catalogId, templates}`)
  }
  const bytes = packageBytes(pkg)
  if (bytes > APP_PACKAGE_MAX_BYTES) {
    throw bad(`package is ${bytes} bytes; the cap is ${APP_PACKAGE_MAX_BYTES}: simplify it`)
  }
  const issues = validatePackage(pkg, APP_CATALOG_IDS)
  if (issues.length > 0) {
    const listed = issues.slice(0, 10).map((issue) => `${issue.path || `/`}: ${issue.message}`)
    const more = issues.length > 10 ? `; and ${issues.length - 10} more` : ``
    throw bad(`package is invalid: ${listed.join(`; `)}${more}`)
  }
  const valid = pkg as AppPackage
  for (const [id, template] of Object.entries(valid.templates)) {
    if (template.components.length > APP_TEMPLATE_MAX_COMPONENTS) {
      throw bad(`template ${id} has ${template.components.length} components; the cap is ${APP_TEMPLATE_MAX_COMPONENTS}`)
    }
  }
  return valid
}

/** The bindings and host functions the Exponential clients resolve for an
 *  app (web, desktop, iOS, Android alike): the `exp:` sources and the
 *  `harness.*` functions the package's allowlist may name. */
export const APP_HOST_SOURCES: readonly { source: string; value: string }[] = [
  { source: `exp:devices`, value: `{rows: [{id, label, detail, icon, discTone, tone, status}], count, online}: the team's devices, live` },
  { source: `exp:issues?board=<boardId>&limit=<n>`, value: `{rows: [{id, identifier, title, status}], count}: a board's issues in list order, live` },
]
export const APP_HOST_FUNCTIONS: readonly { name: string; args: string; effect: string }[] = [
  { name: `harness.toast`, args: `{message, tone?: "error"}`, effect: `shows a toast` },
  { name: `harness.openIssue`, args: `{identifier}`, effect: `opens the issue` },
  { name: `harness.openDevice`, args: `{id, label?}`, effect: `opens the device` },
  { name: `harness.mcp`, args: `{tool, arguments}`, effect: `a tools/call on Exponential's MCP as the viewer, after the person allows it` },
]

/** MCP `apps catalog`: everything an agent needs to write a package. */
export function appCatalogText(): string {
  const sources = APP_HOST_SOURCES.map((s) => `- ${s.source} → ${s.value}`).join(`\n`)
  const functions = APP_HOST_FUNCTIONS.map((f) => `- ${f.name}(${f.args}): ${f.effect}`).join(`\n`)
  return [
    `# App package`,
    `A package = {id, name, version, catalogId: "${appExtension.id}" (the Exponential app extension over the core catalog; "${APP_CATALOG_IDS[0]}" = the core alone), templates: {<templateId>: {components: [flat components, one with id "root"], data?: the initial data model, bindings?: [{path: "/json/pointer", source: "exp:…"}]}}, functions?: [the host functions its surfaces may call: a name or "harness.*"], theme?: a built-in theme id, icon?: {glyph, color}, description?}. The editor renders the FIRST template. Components are FLAT: every child is an id, children: [ids] or {componentId, path, key?} for a bound list.`,
    `Cap: ${APP_PACKAGE_MAX_BYTES} bytes per package, ${APP_TEMPLATE_MAX_COMPONENTS} components per template.`,
    ``,
    `# Bindings (sources the clients resolve, written into the data model at path)`,
    sources,
    ``,
    `# Host functions (on: {press: {functionCall: {call, args}}}; list the ones you use in functions)`,
    functions,
    ``,
    catalogPrompt({ extensions: [appExtension] }),
  ].join(`\n`)
}
