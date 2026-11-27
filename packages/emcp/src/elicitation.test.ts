// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import deletes from "../fixtures/deletes.json" with { type: "json" }
import fixture from "../fixtures/elicitation.json" with { type: "json" }
import {
  CONFIRM_SCHEMA,
  confirmFor,
  confirmGate,
  confirmOutcome,
  elicitationModes,
  pickNeeded,
  pickOutcome,
  pickRequest,
  secretFields,
  urlElicitationAllowed,
  urlRequest,
  type PickKind,
} from "./elicitation"
import { argsDigest, sealRequestState, verifyRequestState, type RequestStatePayload } from "./request-state"

/** `{repeat, times}` = that string repeated; `{concat: [...]}` = the parts joined. */
function expand(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(expand)
  if (value && typeof value === `object`) {
    const v = value as Record<string, unknown>
    if (typeof v.repeat === `string` && typeof v.times === `number`) return v.repeat.repeat(v.times)
    if (Array.isArray(v.concat)) return v.concat.map(expand).join(``)
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, expand(x)]))
  }
  return value
}

describe(`elicitationModes`, () => {
  for (const entry of fixture.modeCases) test(entry.name, () => expect(elicitationModes(entry.capabilities)).toEqual(entry.expected))
})

describe(`confirmGate`, () => {
  for (const entry of fixture.gateCases) {
    test(entry.name, () => {
      const verdict = confirmGate(entry.tool, expand(entry.args), entry.modes)
      const expected = expand(entry.expected) as { kind: string; message?: string }
      expect(verdict.kind).toBe(expected.kind as never)
      if (verdict.kind === `confirm`) {
        expect(verdict.message).toBe(expected.message!)
        expect(verdict.request).toEqual({ method: `elicitation/create`, params: { mode: `form`, message: expected.message!, requestedSchema: CONFIRM_SCHEMA } })
      }
    })
  }

  test(`every destructive op of deletes.json asks`, () => {
    for (const [noun, ops] of Object.entries(deletes.destructiveOps)) {
      for (const [op, key] of Object.entries(ops)) {
        expect(confirmFor(`exponential_${noun}_write`, { op, [key]: `x1` }), `${noun}.${op}`).not.toBeNull()
      }
    }
  })
})

describe(`confirmOutcome`, () => {
  for (const entry of fixture.confirmOutcomeCases) test(entry.name, () => expect(confirmOutcome(entry.response)).toBe(entry.expected as never))
})

describe(`picks`, () => {
  for (const entry of fixture.pickNeededCases) {
    test(`needed: ${entry.name}`, () => expect(pickNeeded(entry.pick as PickKind, entry.tool, entry.args)).toBe(entry.expected))
  }
  for (const entry of fixture.pickCases) {
    test(`request: ${entry.name}`, () => expect(pickRequest(entry.pick as PickKind, entry.choices, entry.modes)).toEqual(entry.expected as never))
  }
  for (const entry of fixture.pickOutcomeCases) {
    test(`outcome: ${entry.name}`, () => expect(pickOutcome(entry.property, entry.offered, entry.response)).toEqual(entry.expected as never))
  }
})

describe(`URL mode`, () => {
  for (const entry of fixture.urlCases) {
    test(`${entry.url} under ${entry.appUrl}`, () => expect(urlElicitationAllowed(entry.url, entry.appUrl)).toBe(entry.expected))
  }
  for (const entry of fixture.urlRequestCases) {
    test(entry.name, () => expect(urlRequest(entry.purpose, entry.subject, entry.appUrl, entry.id, entry.modes)).toEqual(entry.expected as never))
  }
})

describe(`no secrets in form mode`, () => {
  for (const entry of fixture.secretCases) test(entry.name, () => expect(secretFields(entry.schema as never)).toEqual(entry.expected))

  test(`every schema this server builds passes`, () => {
    expect(secretFields(CONFIRM_SCHEMA)).toEqual([])
    for (const entry of fixture.pickCases) {
      const verdict = pickRequest(entry.pick as PickKind, entry.choices, entry.modes)
      if (verdict.kind === `ask` && verdict.request.params.mode === `form`) expect(secretFields(verdict.request.params.requestedSchema)).toEqual([])
    }
  })
})

describe(`requestState`, () => {
  const v = fixture.requestState.vectors
  const payload = { ...v.payload, kind: `elicit` } as RequestStatePayload

  test(`exact vectors`, () => {
    expect(argsDigest(v.arguments)).toBe(v.argsDigest)
    expect(sealRequestState(v.key, payload)).toBe(v.token)
  })

  const token = (spec: unknown): unknown => {
    if (spec === `vector`) return v.token
    if (spec === `vector:payload`) {
      const [prefix, body, mac] = v.token.split(`.`)
      const forged = Buffer.from(Buffer.from(body!, `base64url`).toString(`utf8`).replace(`user:u1`, `user:u2`)).toString(`base64url`)
      return `${prefix}.${forged}.${mac}`
    }
    if (spec === `vector:mac`) return `${v.token.slice(0, -2)}${v.token.endsWith(`AA`) ? `AB` : `AA`}`
    return spec
  }

  for (const entry of fixture.requestState.verifyCases) {
    test(entry.name, () => {
      const result = verifyRequestState(token(entry.token), {
        key: v.key,
        principal: entry.principal,
        tool: entry.tool,
        args: entry.arguments,
        now: entry.now,
        consumed: new Set(entry.consumed),
      })
      expect(result.ok ? null : result.error).toBe(entry.expected as never)
    })
  }
})
