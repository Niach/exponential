// VAPP-91: the host half of the generator (scripts/generate.ts merges it).
// The fixtures' INPUTS are hand-written; this fills every `expected` from
// the TS reference (src/host/), so the Rust core and every painter replay
// the same answers.
//
//   fixtures/host-transport.json   JSONL / SSE / MCP decoding, the MCP action call
//   fixtures/host-policy.json      function gate, combine, urls, media, sources, negotiation
//   fixtures/host-router.json      package validation + message flows → ops

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
  supportedCatalogIds,
  validatePackage,
} from "../src/host"
import type { ClientMessage, Decoded, FunctionDecision, FunctionPolicy, MediaOptions, UrlPolicy, VappPackage } from "../src/host"

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
  policy.sources = policy.sources.map((c: Case) => ({ ...c, expected: parseSource(c.uri as string) }))
  policy.negotiation = policy.negotiation.map((c: Case) => ({
    ...c,
    expected: { supportedCatalogIds: supportedCatalogIds(c.extensionIds as string[]), clientCapabilities: clientCapabilities(c.extensionIds as string[]) },
  }))

  const router = read(`fixtures/host-router.json`)
  const packages = router.packages as Record<string, VappPackage>
  router.validation = Object.entries(packages).map(([id, pkg]) => ({ package: id, expected: validatePackage(pkg) }))
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
  }
}
