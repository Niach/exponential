// VAPP-91: the host half of the generator (scripts/generate.ts merges it).
// The fixtures' INPUTS are hand-written; this fills every `expected` from
// the TS reference (src/host/), so the Rust core and every painter replay
// the same answers.
//
//   fixtures/host-transport.json   JSONL / SSE / MCP decoding, the MCP action call
//   fixtures/host-policy.json      function gate, combine, urls, media, sources, negotiation
//   fixtures/host-router.json      package validation + message flows → ops
//   fixtures/host-package.json     VAPP-161 records queries/values, tool consent, extensions, canonical JSON, signatures

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
  parseRecordsSource,
  recordsValue,
  toolConsent,
  toolConsentKey,
  toolCall,
  canonicalJson,
  packageFileSignedBytes,
  base64UrlEncode,
  AUTHOR_KEY_PREFIX,
} from "../src/host"
import type { ClientMessage, Decoded, FunctionDecision, FunctionPolicy, MediaOptions, UrlPolicy, AppPackage, AvailableExtension, PackageTool, RecordRow } from "../src/host"

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

function renderPackageFixture(f: Record<string, any>): Record<string, unknown> {
  const parsed = (source: string) => {
    const p = parseSource(source)
    return p ? parseRecordsSource(p) : null
  }
  f.recordsQuery = f.recordsQuery.map((c: Case) => ({ source: c.source, expected: parsed(c.source as string) }))
  f.recordsValue = f.recordsValue.map((c: Case) => ({
    ...c,
    expected: recordsValue(c.rows as RecordRow[], parsed(c.source as string)!, { me: c.me as string | null, names: c.names as Record<string, string> }),
  }))
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
    const pkg = { id: `x`, name: `X`, version: `1.0.0`, catalogId: supportedCatalogIds()[0]!, templates: { t: { components: [{ id: `root`, component: `Text`, text: `Hi` }] } }, extensions: c.extensions }
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
