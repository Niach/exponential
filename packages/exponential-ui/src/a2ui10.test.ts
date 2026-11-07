// VAPP-289 (docs/round-9-contract.md §15): the A2UI 1.0 fixtures replayed
// through the TS reference. `expected` comes from this same reference, so
// the replay locks them against drift; the hand-checked promises sit in the
// `describe` at the end.

import { describe, expect, test } from "bun:test"
import messages from "../fixtures/a2ui10-messages.json"
import functions from "../fixtures/a2ui10-functions.json"
import conformance from "../fixtures/a2ui10-conformance.json"
import { renderA2ui10, renderFlow, renderMint, renderSequence } from "../scripts/generate-a2ui10"
import { isIdentifier, mangle } from "./ids"
import type { AppPackage } from "./host"

type Any = Record<string, any>
const m = messages as unknown as Any
const f = functions as unknown as Any

describe(`a2ui10-messages.json`, () => {
  for (const c of m.ids) test(`id ${JSON.stringify(c.id)}`, () => expect({ identifier: isIdentifier(c.id), mangle: mangle(c.id) }).toEqual(c.expected))
  for (const flow of m.flows) test(`flow: ${flow.name}`, () => expect(renderFlow(flow, m.packages as Record<string, AppPackage>).steps).toEqual(flow.steps))
  for (const c of m.mint) test(`mint: ${c.name}`, () => expect(renderMint(c).steps).toEqual(c.steps))
  test(`every generated section is what the file holds`, async () => {
    const files = renderA2ui10()
    expect(JSON.parse(files[`fixtures/a2ui10-messages.json`]!)).toEqual(m)
    expect(JSON.parse(files[`fixtures/a2ui10-conformance.json`]!)).toEqual(conformance)
    const { sequences: _a, ...rendered } = JSON.parse(files[`fixtures/a2ui10-functions.json`]!) as Any
    const { sequences: _b, ...held } = f
    expect(rendered).toEqual(held)
  })
})

describe(`a2ui10-functions.json`, () => {
  for (const c of f.sequences)
    test(`sequence: ${c.name}`, async () => {
      const { steps, expectedSent, expectedRan, ...input } = c
      const stripped = steps.map(({ expected: _, ...s }: Any) => s)
      const got = await renderSequence({ ...input, steps: stripped })
      expect(got.steps).toEqual(steps)
      expect(got.expectedSent).toEqual(expectedSent)
      expect(got.expectedRan).toEqual(expectedRan)
    })
})

describe(`the promises, by hand`, () => {
  const flow = (name: string) => m.flows.find((x: Any) => x.name.startsWith(name))
  test(`a seen surface id stays refused after deleteSurface`, () => {
    const codes = flow(`§1.4`).steps.map((s: Any) => s.expected[0].message?.error?.code ?? s.expected[0].op)
    expect(codes).toEqual([`create`, `INTEGRITY_ERROR`, `delete`, `INTEGRITY_ERROR`, `SURFACE_NOT_FOUND`])
  })
  test(`the entry check drops the bad entries and keeps the rest`, () => {
    const ops = flow(`§5.2`).steps[1].expected as Any[]
    expect(ops.at(-1)).toMatchObject({ op: `components`, components: [{ id: `ok1` }] })
    expect(ops.slice(0, -1).every((op) => op.message.error.code === `INVALID_MESSAGE` && op.message.error.path.startsWith(`/updateComponents/components/`))).toBe(true)
  })
  test(`routing: the policy is in front of the agent`, () => {
    const decide = (name: string) => f.routing.find((c: Any) => c.name === name).expected
    expect(decide(`policy deny in front of the agent`)).toMatchObject({ route: `fail`, code: `INVALID_FUNCTION_CALL` })
    expect(decide(`no agent: unknown`)).toMatchObject({ route: `fail`, code: `UNKNOWN_FUNCTION` })
    expect(decide(`an unknown un-prefixed name routes to the agent`)).toMatchObject({ route: `agent` })
  })
  test(`the coercion table`, () => {
    const of = (value: unknown) => f.coercion.find((c: Any) => Object.is(c.value, value) || JSON.stringify(c.value) === JSON.stringify(value)).expected.boolean
    expect([of(`TRUE`), of(`hello`), of(0), of(1), of(null), of({ valid: true }), of({ a: 1 })]).toEqual([true, false, false, true, false, true, false])
  })
  test(`a reactive routed call is shared; an action never`, () => {
    const seq = (name: string) => f.sequences.find((c: Any) => c.name.startsWith(name))
    expect(seq(`reactive: pending`).expectedSent.filter((x: Any) => x.callAgentFunction?.callFunction.args.q === `a`)).toHaveLength(1)
    expect(seq(`actions are never shared`).expectedSent.map((x: Any) => x.callAgentFunction?.functionCallId)).toEqual([`s:1`, `s:2`])
  })
  test(`every conformance case names an outcome, and every disagreement says why`, () => {
    for (const c of (conformance as unknown as { cases: Any[] }).cases) if (c.expect !== `skip` && [`ok`, `catalog-ok`].includes(c.expect) !== c.upstreamValid) expect(c.why, `${c.file} ${c.index}`).toBeString()
  })
})
