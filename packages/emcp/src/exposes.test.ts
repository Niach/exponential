// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"

import actions from "@exp/domain-contract/fixtures/app-actions.json" with { type: "json" }
import fixture from "../fixtures/exposes.json" with { type: "json" }
import {
  actionAccess,
  listToolNames,
  provenanceMeta,
  qualifiedToolName,
  resolveArgs,
  shortHash,
  splitToolName,
  upstreamAccess,
  upstreamCallError,
  upstreamDestructive,
  validateExposes,
  type Expose,
  type ExposeContext,
} from "./exposes"

test(`destructiveWords = app-actions.json's`, () => {
  expect(fixture.destructiveWords).toEqual(actions.destructiveWords)
})

test(`every error code is listed`, () => {
  const source = readFileSync(new URL(`./exposes.ts`, import.meta.url), `utf8`)
  const used = new Set([...source.matchAll(/fail\(`([a-zA-Z]+)`\)|code: `([a-zA-Z]+)`/g)].map((m) => m[1] ?? m[2]))
  for (const code of used) expect(fixture.errors).toContain(code!)
})

describe(`actionAccess`, () => {
  for (const entry of fixture.actionAccessCases) {
    test(entry.name, () => expect(actionAccess(entry.name)).toBe(entry.expected as never))
  }
})

describe(`upstream annotations`, () => {
  for (const entry of fixture.upstreamCases) {
    test(`${entry.tool} ${JSON.stringify(entry.annotations)}`, () => {
      expect(upstreamDestructive(entry.tool, entry.annotations)).toBe(entry.destructive)
      expect(upstreamAccess(entry.annotations)).toBe(entry.access as never)
    })
  }
})

describe(`validateExposes`, () => {
  for (const entry of fixture.validateCases) {
    test(entry.name, () => {
      expect(validateExposes(entry.exposes as unknown as Expose[], entry.context as ExposeContext)).toEqual(entry.expected)
    })
  }
  test(`too many`, () => {
    const one = fixture.validateCases[1]!.exposes[0] as unknown as Expose
    const many = Array.from({ length: fixture.tooManyCase.count }, (_, i) => ({ ...one, name: `t${i}` }))
    expect(validateExposes(many, { kinds: [], upstreams: [] })).toEqual(fixture.tooManyCase.expected)
  })
})

describe(`upstreamCallError`, () => {
  for (const entry of fixture.upstreamCallCases) {
    test(entry.name, () => {
      const expose = { access: entry.access, run: { upstream: `m5`, tool: entry.tool } } as unknown as Expose
      expect(upstreamCallError(expose, entry.annotations)).toBe(entry.expected)
    })
  }
})

describe(`resolveArgs`, () => {
  for (const entry of fixture.resolveCases) {
    test(entry.name, () => expect(resolveArgs(entry.args, entry.input as Record<string, unknown>)).toEqual(entry.expected))
  }
})

describe(`tool names`, () => {
  for (const entry of fixture.nameCases) {
    test(`${entry.slug} ${entry.name}`, () => {
      const name = qualifiedToolName(entry.slug, entry.name)
      expect(name).toBe(entry.expected)
      expect(name.length).toBeLessThanOrEqual(fixture.limits.toolNameMax)
      expect(splitToolName(name)?.tool).toBe(entry.name)
    })
  }
  for (const entry of fixture.hashCases) {
    test(`shortHash(${JSON.stringify(entry.text)})`, () => expect(shortHash(entry.text)).toBe(entry.expected))
  }
  for (const entry of fixture.splitCases) {
    test(`splitToolName(${entry.name})`, () => expect(splitToolName(entry.name)).toEqual(entry.expected))
  }
  for (const entry of fixture.listCases) {
    test(entry.name, () => expect(listToolNames(entry.apps)).toEqual(entry.expected))
  }
})

describe(`provenanceMeta`, () => {
  for (const entry of fixture.metaCases) {
    test(`${entry.slug} ${entry.expose.name}`, () => {
      expect(provenanceMeta(entry.slug, entry.expose as unknown as Expose)).toEqual(entry.expected as Record<string, string>)
    })
  }
})
