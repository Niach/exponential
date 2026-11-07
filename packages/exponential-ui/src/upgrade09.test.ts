// VAPP-289 (round 9 contract §14): replays fixtures/upgrade09.json against
// the TS reference, plus determinism and the pattern rename.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/upgrade09.json" with { type: "json" }
import {
  emptyUpgrade09State,
  rename09,
  renamePattern09,
  upgradeCard09,
  upgradeClientMessage09,
  upgradeComponents09,
  upgradeMessage09,
  upgradePackage09,
  upgradePackageBody09,
} from "./upgrade09"
import type { Upgrade09State } from "./upgrade09"

interface Case {
  name: string
  kind: `message` | `stream` | `client` | `card` | `package`
  input?: unknown
  inputs?: unknown[]
  verify?: boolean
  expected: unknown
  expectedState?: Upgrade09State
}

const cases = fixture.cases as Case[]
/** Byte-level: key order included. */
const bytes = (v: unknown) => JSON.stringify(v)

describe(`upgrade09.json`, () => {
  for (const c of cases) {
    test(`${c.kind}: ${c.name}`, async () => {
      switch (c.kind) {
        case `message`: {
          const r = upgradeMessage09(c.input, emptyUpgrade09State())
          expect(r.messages).toHaveLength(1)
          expect(bytes(r.messages[0])).toBe(bytes(c.expected))
          expect(r.state).toEqual(c.expectedState!)
          break
        }
        case `stream`: {
          let state = emptyUpgrade09State()
          const out: unknown[] = []
          for (const m of c.inputs!) {
            const r = upgradeMessage09(m, state)
            expect(r.messages).toHaveLength(1)
            out.push(...r.messages)
            state = r.state
          }
          expect(bytes(out)).toBe(bytes(c.expected))
          expect(state).toEqual(c.expectedState!)
          break
        }
        case `client`:
          expect(bytes(upgradeClientMessage09(c.input))).toBe(bytes(c.expected))
          break
        case `card`:
          expect(bytes(upgradeCard09(c.input))).toBe(bytes(c.expected))
          break
        case `package`:
          if (c.verify) expect(bytes(await upgradePackage09(c.input))).toBe(bytes(c.expected))
          else expect(bytes(upgradePackageBody09(c.input))).toBe(bytes(c.expected))
          break
      }
    })
  }

  test(`covers every kind`, () => {
    expect(new Set(cases.map((c) => c.kind))).toEqual(new Set([`message`, `stream`, `client`, `card`, `package`]))
    expect(cases.filter((c) => c.verify).map((c) => (c.expected as { status: string }).status).sort()).toEqual([`invalid`, `unsigned`, `valid`])
  })
})

describe(`determinism`, () => {
  test(`a 0.9 message upgraded twice from the same state gives the same result and mutates nothing`, () => {
    for (const c of cases.filter((c) => c.kind === `stream`)) {
      let state = emptyUpgrade09State()
      for (const m of c.inputs!) {
        const before = bytes(m)
        const stateBefore = bytes(state)
        const a = upgradeMessage09(m, state)
        const b = upgradeMessage09(m, state)
        expect(bytes(a)).toBe(bytes(b))
        expect(bytes(m)).toBe(before)
        expect(bytes(state)).toBe(stateBefore)
        state = a.state
      }
    }
  })

  test(`cards, client messages and packages are pure`, async () => {
    for (const c of cases.filter((c) => c.kind !== `message` && c.kind !== `stream`)) {
      const before = bytes(c.input)
      if (c.kind === `card`) expect(bytes(upgradeCard09(c.input))).toBe(bytes(upgradeCard09(c.input)))
      if (c.kind === `client`) expect(bytes(upgradeClientMessage09(c.input))).toBe(bytes(upgradeClientMessage09(c.input)))
      if (c.kind === `package`) expect(bytes(upgradePackageBody09(c.input))).toBe(bytes(upgradePackageBody09(c.input)))
      expect(bytes(c.input)).toBe(before)
    }
  })

  test(`a format 6 package body is returned unchanged`, () => {
    const pkg = { id: `a`, formatVersion: 6, functions: [`harness.toast`] }
    expect(upgradePackageBody09(pkg)).toBe(pkg)
  })

  test(`upgradePackage09 runs the given verifier on the original file`, async () => {
    const seen: unknown[] = []
    const file = { id: `a`, formatVersion: 5, signature: { alg: `ed25519`, value: `x` } }
    const r = await upgradePackage09(file, async (f) => (seen.push(f), { status: `valid`, author: { name: `A`, key: `ed25519:k` } }))
    expect(seen).toEqual([file])
    expect(r).toEqual({ package: { id: `a`, formatVersion: 6 }, status: `valid`, author: { name: `A`, key: `ed25519:k` } })
  })

  test(`upgradeComponents09 threads its own copy of the state`, () => {
    const state = emptyUpgrade09State()
    const r = upgradeComponents09([{ id: `a-b`, component: `Text`, text: { path: `/x` } }], undefined, state, `s`)
    expect(r.components).toEqual([{ id: `a_b`, component: `Text`, text: { "@path": `/x` } }])
    expect(r.state.surfaces).toEqual({ s: { "a-b": `a_b` } })
    expect(state.surfaces).toEqual({})
  })
})

describe(`rename`, () => {
  test(`names`, () => {
    expect(rename09(`app.navigate`)).toBe(`app_navigate`)
    expect(rename09(`exponential.mcp_servers.create`)).toBe(`exponential_mcp_servers_create`)
    expect(rename09(`tools.my-crm.search.v2`)).toBe(`tools_my_crm__search_v2`)
    expect(rename09(`upstream.git hub.list`)).toBe(`upstream_git_hub__list`)
    expect(rename09(`formatDate`)).toBe(`formatDate`)
    expect(rename09(`acme.thing`)).toBe(`acme.thing`)
  })

  test(`patterns keep the *`, () => {
    expect(renamePattern09(`*`)).toBe(`*`)
    expect(renamePattern09(`harness.*`)).toBe(`harness_*`)
    expect(renamePattern09(`exponential.issues.*`)).toBe(`exponential_issues_*`)
    expect(renamePattern09(`tools.*`)).toBe(`tools_*`)
    expect(renamePattern09(`tools.crm.*`)).toBe(`tools_crm__*`)
    expect(renamePattern09(`upstream.github.*`)).toBe(`upstream_github__*`)
    expect(renamePattern09(`tools.crm.se*`)).toBe(`tools_crm__se*`)
    expect(renamePattern09(`harness.to*`)).toBe(`harness_to*`)
    expect(renamePattern09(`acme.*`)).toBe(`acme.*`)
    expect(renamePattern09(`records.upsert`)).toBe(`records_upsert`)
  })
})
