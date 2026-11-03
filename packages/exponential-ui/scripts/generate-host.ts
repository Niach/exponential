// VAPP-91: the host half of the generator (scripts/generate.ts merges it).
// The fixtures' INPUTS are hand-written; this fills every `expected` from
// the TS reference (src/host/), so the Rust core and every painter replay
// the same answers.
//
//   fixtures/host-transport.json   JSONL / SSE / MCP decoding, the MCP action call
//   fixtures/host-policy.json      function gate, combine, urls, media, sources, negotiation
//   fixtures/host-router.json      package validation + message flows → ops
//   fixtures/host-package.json     VAPP-161 tool consent, extensions, canonical JSON, signatures
//   fixtures/records-query.json    VAPP-189 the records query: parse, sources, values (src/host/query.ts)

import { createPrivateKey, sign as edSign } from "node:crypto"
import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import {
  HostRouter,
  JsonlDecoder,
  SseDecoder,
  clientCapabilities,
  combineDecisions,
  decideFunction,
  decideUrl,
  mcpActionCall,
  mediaRequest,
  messagesFromMcpResult,
  parseSource,
  sourceWithParams,
  supportedCatalogIds,
  validatePackage,
  validateRecord,
  recordAttachmentIds,
  parseRecordsQuery,
  parseRecordsJson,
  parseRecordsSource,
  expandQuerySource,
  recordsValue,
  toolConsent,
  toolConsentKey,
  toolCall,
  canonicalJson,
  packageFileSignedBytes,
  base64UrlEncode,
  AUTHOR_KEY_PREFIX,
  kindTransitions,
  isVersionRange,
  kindChanges,
  migrateRecord,
  migrationRefusal,
  packageMigrations,
  rangeHolds,
  validateMigrations,
  transitionRefusal,
  transitionsFrom,
  validateRecordTransitions,
  resolveScreen,
  navigateTarget,
} from "../src/host"
import { dateBucket, type DateBucket } from "../src/dates"
import type { ClientMessage, Decoded, FunctionDecision, FunctionPolicy, MediaOptions, UrlPolicy, AppPackage, AvailableExtension, PackageTool, RecordOwnership, RecordRefTarget, RecordRow, RecordTransitions, TransitionViewer } from "../src/host"

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), `..`)
const read = (rel: string) => JSON.parse(readFileSync(join(pkgRoot, rel), `utf8`))
const json = (v: unknown) => `${JSON.stringify(v, null, 2)}\n`

function feed(dec: { push(c: string): Decoded; end(): Decoded }, chunks: string[]): Decoded {
  const out: Decoded = { messages: [], issues: [] }
  for (const c of chunks) {
    const d = dec.push(c)
    out.messages.push(...d.messages)
    out.issues.push(...d.issues)
  }
  const e = dec.end()
  out.messages.push(...e.messages)
  out.issues.push(...e.issues)
  return out
}

type Case = Record<string, unknown> & { expected?: unknown }

export function renderHost(): Record<string, string> {
  const transport = read(`fixtures/host-transport.json`)
  transport.jsonl = transport.jsonl.map((c: Case) => ({ ...c, expected: feed(new JsonlDecoder(), c.chunks as string[]) }))
  transport.sse = transport.sse.map((c: Case) => ({ ...c, expected: feed(new SseDecoder(), c.chunks as string[]) }))
  transport.mcp = transport.mcp.map((c: Case) => ({ ...c, expected: messagesFromMcpResult(c.result) }))
  transport.mcpAction = transport.mcpAction.map((c: Case) => ({ ...c, expected: mcpActionCall(c.message as ClientMessage, c.tool as string | undefined) }))

  const policy = read(`fixtures/host-policy.json`)
  policy.functions = policy.functions.map((c: Case) => ({ ...c, expected: decideFunction(c.policy as FunctionPolicy | undefined, c.fn as string, c.registered as boolean) }))
  policy.combine = policy.combine.map((c: Case) => ({ ...c, expected: combineDecisions(c.a as FunctionDecision, c.b as FunctionDecision) }))
  policy.urls = policy.urls.map((c: Case) => ({ ...c, expected: decideUrl(c.policy as UrlPolicy | undefined, c.url as string) }))
  policy.media = policy.media.map((c: Case) => ({ ...c, expected: mediaRequest(c.url as string, c.options as MediaOptions) }))
  policy.sourceParams = policy.sourceParams.map((c: Case) => ({ ...c, expected: sourceWithParams(c.source as string, c.params as Record<string, unknown>) }))
  policy.sources = policy.sources.map((c: Case) => ({ ...c, expected: parseSource(c.uri as string) }))
  policy.negotiation = policy.negotiation.map((c: Case) => ({
    ...c,
    expected: { supportedCatalogIds: supportedCatalogIds(c.extensionIds as string[]), clientCapabilities: clientCapabilities(c.extensionIds as string[]) },
  }))

  const router = read(`fixtures/host-router.json`)
  const packages = router.packages as Record<string, AppPackage>
  const pkgFixture = renderPackageFixture(read(`fixtures/host-package.json`))
  router.validation = Object.entries(packages).map(([id, pkg]) => ({ package: id, expected: validatePackage(pkg) }))
  router.records = router.records.map((c: Case) => ({ ...c, expected: validateRecord(packages[c.package as string]!.records![c.kind as string]!.schema, c.data) }))
  // VAPP-221: the ids a record's attachment fields reference.
  router.attachments = router.attachments.map((c: Case) => ({ ...c, expected: recordAttachmentIds(packages[c.package as string]!.records![c.kind as string]!.schema, c.data) }))
  // VAPP-218: the screens a host opens and the navigate targets.
  router.screens = router.screens.map((c: Case) => ({ ...c, expected: resolveScreen(packages[c.package as string]!, c.screen as string | null, c.params as Record<string, unknown> | null) }))
  router.navigate = router.navigate.map((c: Case) => ({ ...c, expected: navigateTarget(packages[c.package as string]!, c.args) }))
  router.flows = router.flows.map((flow: Case & { steps: { message?: unknown }[] | unknown[] }) => {
    const r = new HostRouter({ extensionIds: (flow.extensionIds as string[]) ?? [] })
    const installIssues: Record<string, unknown> = {}
    for (const id of (flow.packages as string[]) ?? []) installIssues[id] = r.installPackage(packages[id]!)
    const steps = (flow.steps as unknown[]).map((s) => {
      const message = s && typeof s === `object` && `message` in (s as object) && `expected` in (s as object) ? (s as { message: unknown }).message : s
      return { message, expected: r.route(message) }
    })
    const out: Record<string, unknown> = { name: flow.name }
    if (flow.extensionIds) out.extensionIds = flow.extensionIds
    if (flow.packages) {
      out.packages = flow.packages
      out.installIssues = installIssues
    }
    out.steps = steps
    return out
  })

  return {
    "fixtures/host-transport.json": json(transport),
    "fixtures/host-policy.json": json(policy),
    "fixtures/host-router.json": json(router),
    "fixtures/host-package.json": json(pkgFixture),
    "fixtures/records-query.json": json(renderRecordsQuery(read(`fixtures/records-query.json`))),
    "fixtures/record-transitions.json": json(renderRecordTransitions(read(`fixtures/record-transitions.json`))),
    "fixtures/record-migrations.json": json(renderRecordMigrations(read(`fixtures/record-migrations.json`))),
  }
}

/** A JSON pointer write (the fixture's `tamper` pairs). */
function setPointer(root: Record<string, unknown>, pointer: string, value: unknown): void {
  const keys = pointer.split(`/`).slice(1).map((k) => k.replace(/~1/g, `/`).replace(/~0/g, `~`))
  let at = root as Record<string, unknown>
  for (const key of keys.slice(0, -1)) at = at[key] as Record<string, unknown>
  at[keys[keys.length - 1]!] = value
}

const PKCS8_ED25519_PREFIX = `302e020100300506032b657004220420`

/** VAPP-189: `parseCases` = parseRecordsQuery(params, kind), `sourceCases` =
 *  parseRecordsSource(parseSource(source)), `valueCases` = recordsValue(rows,
 *  parseRecordsQuery(params, kind), {me, now, names}). */
function renderRecordsQuery(f: Record<string, any>): Record<string, unknown> {
  f.parseCases = f.parseCases.map((c: Case) => ({ ...c, expected: parseRecordsQuery(c.params as Record<string, string>, c.kind as string | undefined) }))
  f.sourceCases = f.sourceCases.map((c: Case) => {
    const p = parseSource(c.source as string)
    return { ...c, expected: p ? parseRecordsSource(p) : null }
  })
  f.jsonCases = f.jsonCases.map((c: Case) => ({ ...c, expected: parseRecordsJson(c.query, c.kind as string | undefined) }))
  f.bucketCases = f.bucketCases.map((c: Case) => ({ ...c, expected: dateBucket(c.date as string, c.bucket as DateBucket) }))
  f.expandCases = f.expandCases.map((c: Case) => ({ ...c, expected: expandQuerySource(c.source as string, c.queries as Record<string, unknown> | undefined) }))
  f.valueCases = f.valueCases.map((c: Case) => ({
    ...c,
    expected: recordsValue(c.rows as RecordRow[], c.query !== undefined ? parseRecordsJson(c.query, c.kind as string | undefined).query : parseRecordsQuery(c.params as Record<string, string>, c.kind as string | undefined), {
      me: c.me as string | null,
      now: c.now as string,
      today: c.today as string | undefined,
      names: c.names as Record<string, string>,
      computed: c.computed as Record<string, string> | undefined,
      refs: c.refs as Record<string, RecordRefTarget> | undefined,
      issues: c.issues as Record<string, Record<string, unknown>> | undefined,
      transitions: c.transitions ? kindTransitions({ transitions: c.transitions }) : undefined,
      role: c.role as string | undefined,
    }),
  }))
  return f
}

/** eApps R6 (VAPP-222): a case's machine = its own `transitions` (null =
 *  none), else the fixture kind's. */
export function caseTransitions(f: Record<string, any>, c: Case): RecordTransitions | null {
  return `transitions` in c ? kindTransitions({ transitions: c.transitions }) : kindTransitions(f.kind)
}

/** eApps R8 (VAPP-233): record-migrations.json. */
function renderRecordMigrations(f: Record<string, any>): Record<string, unknown> {
  f.rangeCases = f.rangeCases.map((c: Case) => ({ ...c, expected: { holds: rangeHolds(c.version as string, c.range as string), range: isVersionRange(c.range as string) } }))
  f.migrateCases = f.migrateCases.map((c: Case) => ({ ...c, expected: migrateRecord(packageMigrations(c.package), c.version as string, c.kind as string, c.data as Record<string, unknown>) }))
  f.validateCases = f.validateCases.map((c: Case) => ({ ...c, expected: validateMigrations(c.package) }))
  f.changeCases = f.changeCases.map((c: Case) => ({ ...c, expected: kindChanges(c.before, c.after) }))
  f.refusalCases = f.refusalCases.map((c: Case) => ({ ...c, expected: migrationRefusal(c.before, c.after) }))
  return f
}

/** eApps R6: `validateCases`, `refusalCases`, `fromCases` (record-transitions.json). */
function renderRecordTransitions(f: Record<string, any>): Record<string, unknown> {
  f.validateCases = f.validateCases.map((c: Case) => ({ ...c, expected: validateRecordTransitions(c.kind, `/records/${c.kindName}`, c.kindName as string) }))
  f.refusalCases = f.refusalCases.map((c: Case) => ({
    ...c,
    expected: transitionRefusal(caseTransitions(f, c), c.before as Record<string, unknown> | null, c.after as Record<string, unknown>, c.viewer as TransitionViewer, c.row as RecordOwnership | undefined),
  }))
  f.fromCases = f.fromCases.map((c: Case) => ({ ...c, expected: transitionsFrom(caseTransitions(f, c), c.record as Record<string, unknown> | null, c.viewer as TransitionViewer | undefined, c.row as RecordOwnership | undefined) }))
  return f
}

function renderPackageFixture(f: Record<string, any>): Record<string, unknown> {
  f.consent = f.consent.map((c: Case) => {
    const tools = ((c.package as { tools?: Record<string, PackageTool> }).tools ?? {}) as Record<string, PackageTool>
    return {
      package: c.package,
      packageId: c.packageId,
      expected: toolConsent(c.package as { tools?: Record<string, PackageTool> }),
      keys: Object.fromEntries(Object.keys(tools).sort().map((id) => [id, toolConsentKey(c.packageId as string, id, tools[id]!)])),
    }
  })
  f.toolCalls = f.toolCalls.map((c: string | Case) => {
    const name = typeof c === `string` ? c : (c.name as string)
    return { name, expected: toolCall(name) }
  })
  f.extensions = f.extensions.map((c: Case) => {
    const pkg = { formatVersion: 3, id: `x`, name: `X`, version: `1.0.0`, catalogId: supportedCatalogIds()[0]!, templates: { t: { components: [{ id: `root`, component: `Text`, text: `Hi` }] } }, extensions: c.extensions }
    return { ...c, expected: validatePackage(pkg, supportedCatalogIds(c.extensionIds as string[]), c.available as AvailableExtension[] | undefined) }
  })
  f.canonical = f.canonical.map((c: Case) => ({ text: c.text, expected: canonicalJson(JSON.parse(c.text as string)) }))
  const key = createPrivateKey({ key: Buffer.from(PKCS8_ED25519_PREFIX + f.key.seed, `hex`), format: `der`, type: `pkcs8` })
  const publicKey = `${AUTHOR_KEY_PREFIX}${base64UrlEncode(Buffer.from(f.key.publicKey, `hex`))}`
  f.signatures = f.signatures.map((c: Case) => {
    let signed = structuredClone(c.file) as Record<string, unknown>
    if (c.sign) {
      const { signature: _s, author: _a, ...rest } = signed
      const unsigned = { ...rest, author: { name: f.key.name, key: publicKey } }
      signed = { ...unsigned, signature: { alg: `ed25519`, value: base64UrlEncode(edSign(null, packageFileSignedBytes(unsigned), key)) } }
    }
    for (const [pointer, value] of (c.tamper as [string, unknown][] | undefined) ?? []) setPointer(signed, pointer, value)
    const out: Record<string, unknown> = { name: c.name, file: c.file, sign: c.sign }
    if (c.tamper) out.tamper = c.tamper
    out.signed = signed
    out.signedText = new TextDecoder().decode(packageFileSignedBytes(signed))
    if (c.expected !== undefined) out.expected = c.expected
    return out
  })
  return f
}
