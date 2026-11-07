// VAPP-289 (docs/round-9-contract.md §15): the generator half of the A2UI 1.0
// fixtures. The inputs of a2ui10-messages.json and a2ui10-functions.json are
// hand-written; this fills every `expected` from the TS reference, so the
// Rust core replays the same answers. a2ui10-conformance.json is generated
// whole from the vendored upstream cases (vendor/a2ui/v1_0/test/cases).

import { readFileSync, readdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { cardV10, cardProblems, reduceNested } from "../src/reducer"
import { checkCatalogDefinition } from "../src/schema"
import { coerceBoolean, present } from "../src/expr"
import { evaluateCheck, resolveWithFailure, shownResult, checkResult, type Failure } from "../src/dynamic"
import { isIdentifier, mangle } from "../src/ids"
import { A2UI_BASIC_CATALOG_ID as BASIC_ID, CORE_CATALOG_ID } from "../src/catalog"
import { ExponentialHost, HostRouter, MemoryTransport, actionMessage, callAgentFunctionMessage, decideRoute, errorMessage, rendererCapabilities, rendererFunctionResponse, routedCallKey } from "../src/host"
import type { ClientMessage, FunctionPolicy, HostFunction, MessageMetadata, RouteDecisionInput } from "../src/host"
import type { NestedNode } from "../src/types"
import type { AppPackage } from "../src/host"

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), `..`)
const read = (rel: string) => JSON.parse(readFileSync(join(pkgRoot, rel), `utf8`))
const json = (v: unknown) => `${JSON.stringify(v, null, 2)}\n`

type Any = Record<string, any>

/** One router per flow, as host-router.json: `{name, extensionIds?, inlineCatalogs?, packages?, steps: [{message, trusted?, acceptV09?}]}`. */
export function renderFlow(flow: Any, packages: Record<string, AppPackage>): Any {
  const r = new HostRouter({ extensionIds: flow.extensionIds ?? [] })
  for (const id of flow.inlineCatalogs ?? []) r.registerInlineCatalog(id)
  const installIssues: Record<string, unknown> = {}
  for (const id of flow.packages ?? []) installIssues[id] = r.installPackage(packages[id]!)
  const steps = flow.steps.map((s: Any) => {
    const options = { ...(s.trusted ? { trusted: true } : {}), ...(s.acceptV09 === false ? { acceptV09: false } : {}) }
    return { ...s, expected: r.route(s.message, options) }
  })
  return { ...flow, ...(flow.packages ? { installIssues } : {}), steps }
}

/** §1.4: `steps` = `{create: id}` | `{delete: id}` | `{mint: base}` over one router. */
export function renderMint(c: Any): Any {
  const r = new HostRouter()
  const steps = c.steps.map((s: Any) => {
    if (s.create !== undefined) return { ...s, expected: r.route({ version: `1.0`, createSurface: { surfaceId: s.create, catalogId: CORE_CATALOG_ID } }).map((op) => op.op) }
    if (s.delete !== undefined) return { ...s, expected: r.route({ version: `1.0`, deleteSurface: { surfaceId: s.delete } }).map((op) => op.op) }
    return { ...s, expected: r.mintSurfaceId(s.mint) }
  })
  return { ...c, steps }
}

function renderMessages(f: Any): Any {
  const packages = f.packages as Record<string, AppPackage>
  return {
    ...f,
    ids: f.ids.map((c: Any) => ({ ...c, expected: { identifier: isIdentifier(c.id), mangle: mangle(c.id) } })),
    flows: f.flows.map((flow: Any) => renderFlow(flow, packages)),
    mint: f.mint.map(renderMint),
    errors: f.errors.map((c: Any) => ({ ...c, expected: errorMessage(c.code, c.target, c.message, c.path) })),
    actions: f.actions.map((c: Any) => ({ ...c, expected: actionMessage(c.input) })),
    calls: f.calls.map((c: Any) => ({ ...c, expected: callAgentFunctionMessage(c.surfaceId, c.functionCallId, c.name, c.catalogId, c.args) })),
    responses: f.responses.map((c: Any) => ({ ...c, expected: rendererFunctionResponse(c.functionCallId, c.result) })),
    capabilities: f.capabilities.map((c: Any) => ({ ...c, expected: rendererCapabilities(c.extensionIds, c.inline ? { inline: c.inline } : {}) })),
    cards: f.cards.map((c: Any) => {
      const out = cardV10(c.input, c.acceptV09 ?? true)
      if (`error` in out) return { ...c, expected: { error: out.error } }
      const { root, issues } = reduceNested(out.card.root as NestedNode, { catalogId: CORE_CATALOG_ID })
      return { ...c, expected: { card: out.card, problems: cardProblems(out.card.root as NestedNode), issues, root } }
    }),
  }
}

/** One routing case on a real host: the reactive read(s) or the action, the
 *  messages sent, and (`after`) what a later `agentFunctionResponse` does. */
export async function renderSequence(c: Any): Promise<Any> {
  const sent: { message: ClientMessage; metadata?: MessageMetadata }[] = []
  const transport = new MemoryTransport()
  const send = transport.send.bind(transport)
  transport.send = (message: ClientMessage, metadata?: MessageMetadata) => {
    sent.push(metadata ? { message, metadata } : { message })
    send(message)
  }
  const ran: unknown[] = []
  const functions: Record<string, HostFunction> = Object.fromEntries((c.registered ?? []).map((name: string) => [name, (args: Record<string, unknown>) => void ran.push({ name, args })]))
  const host = new ExponentialHost({ transport, agent: c.agent === true, functions, policy: { functions: c.policy as FunctionPolicy | undefined, onFunctionCall: () => c.consent === true } })
  host.connect()
  host.receive({ version: `1.0`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID, ...(c.sendDataModel ? { sendDataModel: true } : {}), dataModel: c.data ?? {} } }, c.trusted ? { trusted: true } : {})
  const steps: Any[] = []
  for (const step of c.steps) {
    if (step.read) {
      const { value, failure } = resolveWithFailure(step.read, host.surface(`s`)!.data, { route: host.reactiveRoute(`s`), catalogId: CORE_CATALOG_ID })
      if (failure) host.reportFailure(`s`, failure)
      steps.push({ ...step, expected: failure ? { value, failure } : { value } })
    } else if (step.press) {
      const outcome = await host.callFunction({ surfaceId: `s`, componentId: `b`, name: step.press, args: step.args ?? {} })
      steps.push({ ...step, expected: outcome })
    } else if (step.pressAsync) {
      void host.callFunction({ surfaceId: `s`, componentId: `b`, name: step.pressAsync, args: step.args ?? {} })
      for (let i = 0; i < 4; i++) await Promise.resolve()
      steps.push(step)
    } else {
      host.receive({ version: `1.0`, agentFunctionResponse: step.respond })
      await Promise.resolve()
      await Promise.resolve()
      steps.push(step)
    }
  }
  return { ...c, steps, expectedSent: sent.map((m) => (`metadata` in m ? m : m.message)), expectedRan: ran }
}

export function renderFunctions(f: Any): Any {
  return {
    ...f,
    coercion: f.coercion.map((c: Any) => ({ ...c, expected: { boolean: coerceBoolean(c.value), present: present(c.value) } })),
    checks: f.checks.map((c: Any) => ({ ...c, expected: evaluateCheck(c.rule, c.data ?? {}, { catalogId: CORE_CATALOG_ID }) })),
    results: f.results.map((c: Any) => ({ ...c, expected: checkResult(c.condition, c.message) })),
    shown: f.shown.map((c: Any) => ({ ...c, expected: shownResult(c.results) })),
    evaluations: f.evaluations.map((c: Any) => {
      const { value, failure } = resolveWithFailure(c.value, c.data ?? {}, { catalogId: c.catalogId ?? CORE_CATALOG_ID, position: c.position ?? `reactive`, ...(c.scope ? { scope: c.scope } : {}), ...(c.available ? { available: c.available } : {}) })
      return { ...c, expected: failure ? { value, failure: failure as Failure } : { value } }
    }),
    routing: f.routing.map((c: Any) => ({ ...c, expected: decideRoute(c.input as RouteDecisionInput) })),
    keys: f.keys.map((c: Any) => ({ ...c, expected: routedCallKey(c.catalogId, c.fn, c.args) })),
  }
}

/** §15: one entry per upstream case: `expect` = our outcome. */
export function renderConformance(): Any {
  const dir = `vendor/a2ui/v1_0/test/cases`
  const schemaMessage = /^(agent_to_renderer|renderer_to_agent)/
  const cases: Any[] = []
  for (const file of readdirSync(join(pkgRoot, dir)).sort()) {
    const doc = read(`${dir}/${file}`) as { schema?: string; tests?: Any[] } | Any[]
    const tests = (Array.isArray(doc) ? doc : (doc.tests ?? [])) as Any[]
    const fileSchema = Array.isArray(doc) ? undefined : doc.schema
    tests.forEach((t, index) => {
      const schema = (t.schema ?? fileSchema ?? ``).replace(/^.*\//, ``).replace(/\.json$/, ``)
      const entry: Any = { file, index, description: t.description ?? t.name ?? ``, schema, data: t.data, upstreamValid: t.valid }
      if (schema === `catalog_definition`) {
        const issues = checkCatalogDefinition(t.data, { flat: false })
        Object.assign(entry, { expect: issues.length ? `catalog-error` : `catalog-ok` })
      } else if (schema === `agent_to_renderer` || schema === `agent_to_renderer_list` || schema === `server_to_client`) {
        const r = new HostRouter()
        const list = Array.isArray(t.data) ? t.data : [t.data]
        // A case without its createSurface runs on a surface of the catalog it
        // tests (upstream validates one message; the router keeps state).
        for (const m of list) {
          const sid = m?.updateComponents?.surfaceId ?? m?.updateDataModel?.surfaceId ?? m?.deleteSurface?.surfaceId
          if (typeof sid === `string` && !r.wasCreated(sid)) r.route({ version: `1.0`, createSurface: { surfaceId: sid, catalogId: BASIC_ID } })
        }
        let first: string = `ok`
        for (const m of list) {
          const code = errorOf(r.route(m))
          if (code && first === `ok`) first = code
        }
        entry.expect = first
      } else if (schemaMessage.test(schema)) {
        entry.expect = `skip`
        entry.why = `renderer → agent messages are built by the renderer, never parsed`
      } else {
        entry.expect = `skip`
        entry.why = `a JSON-Schema-only detail (${schema || `no schema`}) the router does not read`
      }
      if (entry.expect !== `skip` && entry.why === undefined && [`ok`, `catalog-ok`].includes(entry.expect) !== t.valid)
        entry.why = entry.expect.startsWith(`catalog`) ? `an upstream fragment: checkCatalogDefinition reads a whole file (protocolVersion, $defs)` : entry.expect === `ok` ? `the reducer decides it on a kept component (a VALIDATION_FAILED issue, §5.2), or a JSON-Schema-only detail` : `§16: the entry check drops what upstream accepts`
      cases.push(entry)
    })
  }
  return {
    $comment: `GENERATED by \`bun run --filter @exponential_app/eui generate\` from vendor/a2ui/v1_0/test/cases/*.json (round 9 §15). One entry per upstream case {file, index, description, schema, data, upstreamValid, expect, why?}. expect = our router's outcome for an agent message on a fresh router (\`ok\` or the first error code; a list routes in order), \`catalog-ok\` / \`catalog-error\` for a catalog definition (checkCatalogDefinition {flat: false}), \`skip\` + \`why\` where upstream tests a JSON-Schema-only detail our router does not read. Where upstreamValid and expect disagree the divergence is §16's (the entry check drops an entry, unknown catalogs answer at the router).`,
    cases,
  }
}

function errorOf(ops: { op: string; message?: unknown }[]): string | null {
  for (const op of ops) {
    const error = (op.message as { error?: { code: string } } | undefined)?.error
    if (op.op === `send` && error) return error.code
  }
  return null
}

export function renderA2ui10(): Record<string, string> {
  return {
    "fixtures/a2ui10-messages.json": json(renderMessages(read(`fixtures/a2ui10-messages.json`))),
    "fixtures/a2ui10-functions.json": json(renderFunctions(read(`fixtures/a2ui10-functions.json`))),
    "fixtures/a2ui10-conformance.json": json(renderConformance()),
  }
}

/** `bun run scripts/generate-a2ui10.ts`: re-records the async `sequences`
 *  (render() stays synchronous, a test replays them). */
if (import.meta.main) {
  const f = read(`fixtures/a2ui10-functions.json`)
  f.sequences = await Promise.all(f.sequences.map(renderSequence))
  await Bun.write(join(pkgRoot, `fixtures/a2ui10-functions.json`), json(f))
}
